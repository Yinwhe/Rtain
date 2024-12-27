use std::{
    ffi::CString,
    os::fd::{AsRawFd, OwnedFd},
    path::Path,
    sync::Arc,
};

use cgroups_rs::{cgroup_builder::CgroupBuilder, Cgroup, CgroupPid};
use log::{debug, error, info};
use nix::{
    libc::SIGCHLD,
    mount::{mount, umount2, MntFlags, MsFlags},
    pty::{openpty, OpenptyResult},
    sched::{clone, CloneFlags},
    sys::wait::waitpid,
    unistd::{chdir, dup2, execvp, pipe, pivot_root, read, write, Pid},
};
use rand::{thread_rng, Rng};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::UnixStream,
};

use crate::core::{
    cmd::RunArgs,
    records::{ContainerRecord, ContainerStatus},
    response::Response,
    RECORD_MANAGER, ROOT_PATH,
};

use super::image::{delete_workspace, new_workspace};

/// Run a new container from given image.
pub async fn run_container(run_args: RunArgs, stream: UnixStream) -> tokio::io::Result<Response> {
    // FIXME: change error formats.
    let (mut client_reader, mut client_writer) = stream.into_split();

    let (pty, pipe, child) = match run_prepare(&run_args) {
        Ok(res) => res,
        Err(e) => {
            error!("Failed to run container: {:?}", e);

            return Ok(Response::Err(format!("Failed to run container: {:?}", e)));
        }
    };

    if let Some(pty) = pty {
        debug!("[Daemon]: Redirecting stdio to PTY");

        let _slave_fd = pty.slave;
        let master_fd = Arc::new(pty.master);

        // Inform the clients here.
        let mut resp = serde_json::to_string(&Response::Continue).unwrap();
        resp.push('\n');
        client_writer.write_all(resp.as_bytes()).await.unwrap();

        // PTY writes to the client.
        let master_reader = Arc::clone(&master_fd);
        let read_from_pty = tokio::spawn(async move {
            let mut buffer = vec![0u8; 1024];
            loop {
                match read(master_reader.as_raw_fd(), &mut buffer) {
                    Ok(0) => break, // EOF
                    Ok(n) => {
                        // debug!("Read from pty, send: {}", String::from_utf8_lossy(&buffer[..n]));

                        if let Err(e) = client_writer.write_all(&buffer[..n]).await {
                            error!("Error writing to client: {}", e);
                            break;
                        }
                    }
                    Err(_e) => break,
                }
            }
        });

        // Client writes to the pty.
        let master_writer = Arc::clone(&master_fd);
        let write_to_pty = tokio::spawn(async move {
            let mut buffer = vec![0u8; 1024];
            loop {
                match client_reader.read(&mut buffer).await {
                    Ok(0) => break, // EOF
                    Ok(n) => {
                        // debug!("Write to pty: {}", String::from_utf8_lossy(&buffer));

                        if let Err(e) = write(&master_writer, &buffer[..n]) {
                            error!("Error writing to client: {}", e);
                            break;
                        }
                    }
                    Err(e) => {
                        error!("Error reading from client: {}", e);
                        break;
                    }
                }
            }
        });

        // Child exits watcher
        let child_exit = tokio::spawn(async move {
            let _ = waitpid(child, None);
        });

        write(&pipe.1, b"CONT").unwrap();

        tokio::select! {
            _ = write_to_pty => {
                debug!("Write to PTY finished, means the client exits");
            }
            _ = child_exit => {
                debug!("Child process exited");
            }
        }
        read_from_pty.abort();

        // FIXME: What shall we do when exits.

        // std::thread::sleep(std::time::Duration::from_secs(1));
        // So here we detach from it.
    } else {
        debug!("[Daemon]: Detach, redirecting stdio to log file");
        write(&pipe.1, b"CONT").unwrap();
    }

    Ok(Response::Ok)
}

fn run_prepare(
    run_args: &RunArgs,
) -> Result<(Option<OpenptyResult>, (OwnedFd, OwnedFd), Pid), Box<dyn std::error::Error>> {
    // Generate name-id.
    let id = random_id();
    let name = run_args.name.clone().unwrap_or_else(|| id.clone());
    let name_id = format!("{}-{}", name, id);

    // Root is where we store needed info and the image for the container.
    let root_path = format!("{}/{}", ROOT_PATH, name_id);
    // And the mnt is where we mount the image as container's sysroot.
    let mnt_path = format!("{}/{}/mnt", ROOT_PATH, name_id);

    // If not detach, we need to stream the container io to clients.
    let pty = if !run_args.detach {
        Some(openpty(None, None)?)
    } else {
        None
    };

    // Sync between daemon and new child process.
    let pipe_p = pipe()?;
    let pipe_c = pipe()?;
    let pipe = (pipe_p.0, pipe_c.1);
    let mut buf = [0u8; 4];

    // Here we create the whole workspace.
    new_workspace(&run_args.image, &root_path, &mnt_path, &run_args.volume)?;

    // Create a new process with new namespaces.
    let child = match new_container_process(
        &root_path,
        &mnt_path,
        (&pipe_c.0, &pipe_p.1),
        pty.as_ref(),
        &run_args.command,
    ) {
        Ok(child) => child,
        Err(e) => {
            // Clone child failure, clean up.
            delete_workspace(&root_path, &mnt_path, &run_args.volume)?;
            return Err(e);
        }
    };

    // Wait for child ready.
    read(pipe.0.as_raw_fd(), &mut buf).unwrap();
    if buf == *b"EXIT" {
        // Child failed to initialize, clean up.
        delete_workspace(&root_path, &mnt_path, &run_args.volume)?;

        return Err("Failed to initialize container".into());
    }

    // Setting up cgroups
    match setup_cgroup(&name_id, child) {
        Ok(cg) => cg,
        Err(e) => {
            write(&pipe.1, b"EXIT").unwrap();

            // CGroup error, clean up.
            delete_workspace(&root_path, &mnt_path, &run_args.volume)?;

            return Err(e);
        }
    };

    // Form the container record.
    let cr = ContainerRecord::new(
        &name,
        &id,
        &child.to_string(),
        &run_args.command.join(" "),
        ContainerStatus::Running,
    );
    RECORD_MANAGER.register(cr);

    Ok((pty, pipe, child))
}

/// This is the first process in the new namespace.
fn do_init(command: &Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    let command_cstr = CString::new(command[0].clone())?;
    let args_cstr: Vec<CString> = command
        .iter()
        .map(|arg| CString::new(arg.clone()).unwrap())
        .collect();

    info!("Ready to run command: {:?}", command);
    execvp(&command_cstr, &args_cstr)?;

    Ok(())
}

/// Create a new process with new namespaces and return its pid.
fn new_container_process(
    root_path: &str,
    mnt_path: &str,
    pipe: (&OwnedFd, &OwnedFd),
    pty: Option<&OpenptyResult>,
    command: &Vec<String>,
) -> Result<Pid, Box<dyn std::error::Error>> {
    let flags = CloneFlags::CLONE_NEWUTS
        | CloneFlags::CLONE_NEWPID
        | CloneFlags::CLONE_NEWNS
        | CloneFlags::CLONE_NEWNET
        | CloneFlags::CLONE_NEWIPC;
    const STACK_SIZE: usize = 1 * 1024 * 1024;
    let mut child_stack: Vec<u8> = vec![0; STACK_SIZE];

    let child_func = || {
        let setup_stdio = || -> Result<(), Box<dyn std::error::Error>> {
            if let Some(pty) = pty {
                let _master_fd = pty.master.try_clone()?;
                let slave_fd = pty.slave.try_clone()?;

                // Redirect stdio.
                dup2(slave_fd.as_raw_fd(), nix::libc::STDIN_FILENO)?;
                dup2(slave_fd.as_raw_fd(), nix::libc::STDOUT_FILENO)?;
                dup2(slave_fd.as_raw_fd(), nix::libc::STDERR_FILENO)?;
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
            write(pipe.1, b"EXIT").unwrap();

            error!("Container initializer failure: {:?}", e);
            return -1;
        }

        // Switch root here.
        if let Err(e) = setup_mount(mnt_path) {
            write(pipe.1, b"EXIT").unwrap();

            error!("Container initializer failure: {:?}", e);
            return -1;
        }

        // Inform the parents ready.
        write(pipe.1, b"WAIT").unwrap();

        // Wait for parent ready.
        let mut buf = [0u8; 4];
        read(pipe.0.as_raw_fd(), &mut buf).unwrap();

        match &buf {
            b"EXIT" => {
                debug!("Parent failed to initialize container");
                return -1;
            }
            b"CONT" => {}
            _ => unreachable!(),
        }

        if let Err(e) = do_init(command) {
            error!("Failed to initialize container: {:?}", e);
            return -1;
        }

        return 0;
    };

    // This new process will run `child_func`
    let child_pid = unsafe { clone(Box::new(child_func), &mut child_stack, flags, Some(SIGCHLD)) }?;

    Ok(child_pid)
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
