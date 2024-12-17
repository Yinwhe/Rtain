use std::{
    ffi::CString,
    os::fd::{AsRawFd, FromRawFd, OwnedFd},
    path::Path,
};

use async_std::{
    io::{ReadExt, WriteExt},
    os::unix::net::{UnixListener, UnixStream},
    stream::StreamExt,
};
use cgroups_rs::{cgroup_builder::CgroupBuilder, Cgroup, CgroupPid};
use log::{debug, error, info};
use nix::{
    libc::SIGCHLD,
    mount::{mount, umount2, MntFlags, MsFlags},
    pty::openpty,
    sched::{clone, CloneFlags},
    sys::wait::waitpid,
    unistd::{chdir, dup2, execvp, pipe, pivot_root, read, write, Pid},
};
use rand::{thread_rng, Rng};

use crate::core::{
    cmd::RunArgs,
    records::{ContainerRecord, ContainerStatus},
    RECORD_MANAGER, ROOT_PATH,
};

use super::image::new_workspace;

/// When run a container command, it first creates a new container process
/// and then runs the command.
pub fn run_container(run_args: RunArgs, mut ctrl_stream: UnixStream) {
    // Create pipes
    let (read_fd, write_fd) = match pipe() {
        Ok((read_fd, write_fd)) => (read_fd, write_fd),
        Err(err) => {
            error!("Failed to create pipe: {:?}", err);

            return;
        }
    };

    // Generate name-id.
    let id: String = random_id();
    let name = run_args.name.unwrap_or_else(|| id.clone());
    let name_id = format!("{}-{}", name, id);

    let root_path = format!("{}/{}", ROOT_PATH, name_id);
    let mnt_path: String = format!("{}/{}/mnt", ROOT_PATH, name_id);

    // Create a new process with new namespaces
    let (child, master_fd, slave_fd) = match new_container_process(
        &root_path,
        &mnt_path,
        read_fd,
        run_args.detach,
        false,
        &run_args.command,
    ) {
        Ok(child) => child,
        Err(err) => {
            error!("Failed to create new namespace process: {:?}", err);
            return;
        }
    };

    // Setting up cgroups
    let cg = match setup_cgroup(&name_id, child) {
        Ok(cg) => cg,
        Err(e) => {
            error!("Failed to setup cgroup: {:?}", e);

            // Clean up the child.
            write(&write_fd, b"EXIT").unwrap();
            let _ = waitpid(child, None);

            return;
        }
    };

    // Here we create the new rootfs
    if let Err(e) = new_workspace(&run_args.image, &root_path, &mnt_path, &run_args.volume) {
        error!("Failed to create new workspace: {:?}", e);

        // Clean up...
        write(&write_fd, b"EXIT").unwrap();
        let _ = waitpid(child, None);
        let _ = cg.delete();

        return;
    }

    // Form the container record.
    let cr = ContainerRecord::new(
        &name,
        &id,
        &child.to_string(),
        &run_args.command.join(" "),
        ContainerStatus::Running,
    );
    RECORD_MANAGER.register(cr);

    // Ok before we continue, we need to check the stdio redirection.
    if !run_args.detach {
        debug!("[Daemon]: Redirecting stdio to PTY");
        nix::unistd::close(slave_fd).unwrap();

        let data_socket_path = format!("{root_path}/data.sock");

        // Wrappting the master_fd into File.
        let master = unsafe { async_std::fs::File::from_raw_fd(master_fd) };
        async_std::task::block_on(async move {
            let listener = match UnixListener::bind(&data_socket_path).await {
                Ok(l) => l,
                Err(e) => {
                    eprintln!("Failed to bind data socket: {}", e);
                    return;
                }
            };
            let mut incomming = listener.incoming();

            // Send new data socket path to the client.
            ctrl_stream
                .write_all(data_socket_path.as_bytes())
                .await
                .unwrap();

            if let Some(Ok(data_stream)) = incomming.next().await {
                // Client to PTY (sdtin).
                let mut from_client = data_stream.clone();
                let mut to_master = master.clone();

                let write_task = async_std::task::spawn(async move {
                    let mut buf = [0u8; 1024];
                    loop {
                        match from_client.read(&mut buf).await {
                            Ok(0) => {
                                debug!(
                                    "[Daemon] Client disconnected from data socket (write task)"
                                );
                                break;
                            }
                            Ok(n) => {
                                to_master.write_all(&buf[..n]).await.unwrap();
                            }
                            Err(e) => {
                                error!("[Daemon] Failed to read from data socket: {}", e);
                                break;
                            }
                        }
                    }
                });

                // PTY to Client (stdout).
                let mut from_master = master.clone();
                let mut to_client = data_stream.clone();
                let read_task = async_std::task::spawn(async move {
                    let mut buf = [0u8; 1024];
                    loop {
                        match from_master.read(&mut buf).await {
                            Ok(0) => {
                                debug!("[Daemon] PTY closed (read task)");
                                break;
                            }
                            Ok(n) => {
                                to_client.write_all(&buf[..n]).await.unwrap();
                            }
                            Err(e) => {
                                error!("[Daemon] Failed to read from PTY: {}", e);
                                break;
                            }
                        }
                    }
                });

                // Let the init to continue.
                write(&write_fd, b"CONT").unwrap();

                // Wait for the tasks.
                let _ = write_task.await;
                let _ = read_task.await;
            }
        });
        // Client to PTY
    } else {
        // Or simply run the container
        write(&write_fd, b"CONT").unwrap();
    }
}

/// This is the first process in the new namespace.
fn do_init(mnt_path: &str, command: &Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    // FIXME: can we setup the mount first ?
    setup_mount(mnt_path)?;

    let command_cstr = CString::new(command[0].clone())?;
    let args_cstr: Vec<CString> = command
        .iter()
        .map(|arg| CString::new(arg.clone()).unwrap())
        .collect();

    info!("Ready to run command: {:?}", command);
    execvp(&command_cstr, &args_cstr)?;

    Ok(())
}

/// Create a new process with new namespaces.
/// This process will then do the initialization.
fn new_container_process(
    root_path: &str,
    mnt_path: &str,
    read_fd: OwnedFd,
    detach: bool,
    _interactive: bool,
    command: &Vec<String>,
) -> Result<(Pid, i32, i32), Box<dyn std::error::Error>> {
    let flags = CloneFlags::CLONE_NEWUTS
        | CloneFlags::CLONE_NEWPID
        | CloneFlags::CLONE_NEWNS
        | CloneFlags::CLONE_NEWNET
        | CloneFlags::CLONE_NEWIPC;

    const STACK_SIZE: usize = 1 * 1024 * 1024;
    let mut child_stack: Vec<u8> = vec![0; STACK_SIZE];

    let (master_fd, slave_fd) = if !detach {
        // If not detach, we have to use the PTY to redirect the stdio.
        // FIXME: lift pty up.
        let pty = openpty(None, None)?;
        (pty.master.as_raw_fd(), pty.slave.as_raw_fd())
    } else {
        (-1, -1)
    };

    debug!("[Daemon] master_fd: {}, slave_fd: {}", master_fd, slave_fd);

    let child_func = || {
        let setup_stdio = || -> Result<(), Box<dyn std::error::Error>> {
            if !detach {
                // nix::unistd::close(master_fd.as_raw_fd())?;

                // // Redirect stdio.
                // dup2(slave_fd.as_raw_fd(), nix::libc::STDIN_FILENO)?;
                // dup2(slave_fd.as_raw_fd(), nix::libc::STDOUT_FILENO)?;
                // dup2(slave_fd.as_raw_fd(), nix::libc::STDERR_FILENO)?;

                // nix::unistd::close(slave_fd.as_raw_fd())?;

                nix::unistd::close(master_fd.as_raw_fd()).unwrap();

                // Redirect stdio.
                dup2(slave_fd.as_raw_fd(), nix::libc::STDIN_FILENO).unwrap();
                dup2(slave_fd.as_raw_fd(), nix::libc::STDOUT_FILENO).unwrap();
                dup2(slave_fd.as_raw_fd(), nix::libc::STDERR_FILENO).unwrap();

                nix::unistd::close(slave_fd.as_raw_fd()).unwrap();
            } else {
                let log_file = std::fs::OpenOptions::new()
                    .create(true)
                    .write(true)
                    .append(true)
                    .open(format!("{root_path}/stdout.log"))?;

                dup2(log_file.as_raw_fd(), nix::libc::STDOUT_FILENO)?;
            }

            Ok(())
        };

        if let Err(e) = setup_stdio() {
            error!("Failed to initialize container: {:?}", e);
            return -1;
        }

        // Wait for cgroups and workspaces setting
        let mut buffer = [0u8; 4];
        read(read_fd.as_raw_fd(), &mut buffer).unwrap();

        match &buffer {
            b"CONT" => (),
            b"EXIT" => return 0,
            _ => {
                error!("Container received an unexpected signal: {:?}", buffer);
                return -1;
            }
        }

        if let Err(e) = do_init(mnt_path, command) {
            error!("Failed to initialize container: {:?}", e);
            return -1;
        }

        // FIXME: test on child crash.

        return 0;
    };

    // This new process will run `child_func`
    let child_pid = unsafe { clone(Box::new(child_func), &mut child_stack, flags, Some(SIGCHLD)) }?;

    Ok((child_pid, master_fd, slave_fd))
}

fn setup_cgroup(cg_name: &str, child: Pid) -> Result<Cgroup, Box<dyn std::error::Error>> {
    let hier = cgroups_rs::hierarchies::auto();
    let cg = match CgroupBuilder::new(&cg_name).build(hier) {
        Ok(cg) => cg,
        Err(e) => return Err(Box::new(e)),
    };

    match cg.add_task_by_tgid(CgroupPid::from(child.as_raw() as u64)) {
        Ok(_) => Ok(cg),
        Err(e) => {
            cg.delete()?;
            Err(Box::new(e))
        }
    }
}

fn setup_mount(mnt_path: &str) -> Result<(), Box<dyn std::error::Error>> {
    // Make the mount namespace private
    mount(
        None::<&str>,
        "/",
        None::<&str>,
        MsFlags::MS_REC | MsFlags::MS_PRIVATE,
        None::<&str>,
    )?;

    // Switch to new root
    switch_root(mnt_path)?;

    // Mount new proc fs
    if !Path::new("/proc").exists() {
        std::fs::create_dir("/proc")?;
    }

    mount(
        Some("proc"),
        "/proc",
        Some("proc"),
        MsFlags::MS_NOEXEC | MsFlags::MS_NOSUID | MsFlags::MS_NODEV,
        None::<&str>,
    )?;

    Ok(())
}

fn switch_root(root: &str) -> Result<(), Box<dyn std::error::Error>> {
    debug!("Switch root to: {}", root);

    // Mount new root to cover the old root
    mount(
        Some(root),
        root,
        None::<&str>,
        MsFlags::MS_BIND | MsFlags::MS_REC,
        None::<&str>,
    )?;

    // Create a new directory to save the old root
    let pivot_dir = format!("{}/.pivot_root", root);
    std::fs::create_dir_all(&pivot_dir)?;

    // Execute `pivot_root` to switch the new root to `root`
    pivot_root(root, pivot_dir.as_str())?;

    // To the new working directory
    chdir("/")?;

    // Unmount the old root
    let pivot_dir_old = "/.pivot_root";
    umount2(pivot_dir_old, MntFlags::MNT_DETACH)?;

    // Remove the old root
    std::fs::remove_dir_all(pivot_dir_old)?;

    Ok(())
}

fn random_id() -> String {
    let mut rng = thread_rng();
    let random_bytes: [u8; 16] = rng.gen();

    random_bytes
        .iter()
        .map(|byte| format!("{:02x}", byte))
        .collect()
}
