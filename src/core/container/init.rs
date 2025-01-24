use std::{ffi::CString, os::fd::AsRawFd, path::Path, sync::Arc};

use cgroups_rs::{cgroup_builder::CgroupBuilder, Cgroup, CgroupPid};
use log::{debug, error, info};
use nix::{
    libc::SIGCHLD,
    mount::{mount, umount2, MntFlags, MsFlags},
    pty::{openpty, OpenptyResult},
    sched::{clone, CloneFlags},
    sys::wait::{waitpid, WaitStatus},
    unistd::{chdir, dup2, execvp, pivot_root, read, write, Pid},
};
use rand::{thread_rng, Rng};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{unix::pipe, UnixStream},
    sync::Mutex,
};

use crate::core::{
    cmd::RunArgs,
    container::stop::do_stop,
    metas::{ContainerMeta, CONTAINER_METAS},
    Msg, ROOT_PATH,
};

use super::image::{delete_workspace, new_workspace};

/// Run a new container from given image.
pub async fn run_container(run_args: RunArgs, stream: UnixStream) {
    let detach = run_args.detach;
    let (pty, pipe, meta) = match run_prepare(run_args).await {
        Ok(res) => res,
        Err(e) => {
            error!("Failed to run container: {:?}", e);
            // FIXME: Error return.

            return;
        }
    };

    let pid = Pid::from_raw(meta.get_pid().unwrap());
    do_run(meta.name, meta.id, pid, pty, pipe, stream, detach).await;
}

pub async fn do_run(
    name: String,
    id: String,
    child: Pid,
    pty: OpenptyResult,
    pipe: (pipe::Sender, pipe::Receiver),
    stream: UnixStream,
    detach: bool,
) {
    let (stream_reader, stream_writer) = stream.into_split();
    let stream_reader = Arc::new(Mutex::new(stream_reader));
    let stream_writer = Arc::new(Mutex::new(stream_writer));

    let name_id = format!("{name}-{id}");
    let root_path = format!("{}/{}", ROOT_PATH, name_id);

    let _slave_fd = pty.slave;
    let master_fd = Arc::new(pty.master);
    let mut log_file = match tokio::fs::File::options()
        .write(true)
        .truncate(true)
        .create(true)
        .open(format!("{}/log.log", root_path))
        .await
    {
        Ok(f) => f,
        Err(e) => {
            error!("Failed to open log file: {:?}", e);
            write(&pipe.0, b"EXIT").unwrap();

            return;
        }
    };

    let (container_reader, mut container_sender) = tokio::io::simplex(1);
    let container_reader = Arc::new(Mutex::new(container_reader));

    // Capture container outs.
    let master_reader = Arc::clone(&master_fd);
    let read_from_pty = tokio::spawn(async move {
        let mut buffer = vec![0u8; 1024];
        loop {
            match read(master_reader.as_raw_fd(), &mut buffer) {
                Ok(0) => break, // EOF
                Ok(n) => {
                    if let Err(e) = container_sender.write_all(&buffer[..n]).await {
                        error!("Error writing to client: {}", e);
                        break;
                    }
                }
                Err(_e) => break,
            }
        }
    });

    write(&pipe.0, b"CONT").unwrap();

    if !detach {
        debug!("[Daemon]: Attach, redirecting stdio to PTY");

        Msg::Continue
            .send_to(&mut *stream_writer.lock().await)
            .await
            .unwrap();

        // PTY writes to the client.
        let client_writer = Arc::clone(&stream_writer);
        let pty_reader = Arc::clone(&container_reader);
        let pty_to_client = tokio::spawn(async move {
            let mut buffer = vec![0u8; 1024];
            let mut client_writer = client_writer.lock().await;
            let mut pty_reader = pty_reader.lock().await;
            loop {
                match pty_reader.read(&mut buffer).await {
                    Ok(0) => break, // EOF
                    Ok(n) => {
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
        let client_reader = Arc::clone(&stream_reader);
        let client_to_pty = tokio::spawn(async move {
            let mut buffer = vec![0u8; 1024];
            let mut client_reader = client_reader.lock().await;
            loop {
                match client_reader.read(&mut buffer).await {
                    Ok(0) => break, // EOF
                    Ok(n) => {
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
        let check_child_exit = tokio::spawn(async move { waitpid(child, None) });

        tokio::select! {
            _ = client_to_pty => {
                // Write to PTY finished, client exits, and in current impl, we end the container here.
                pty_to_client.abort();
            }
            wait_res = check_child_exit => {
                // Child process exited.
                debug!("[Daemon]: Container exited");

                // The container exit, inform the client.
                pty_to_client.abort();
                match wait_res.unwrap() {
                    Ok(status) => {
                        match status {
                            WaitStatus::Exited(_, code) => {
                                let msg = format!( "Container exited with code: {code}");
                                stream_writer.lock().await.write_all(msg.as_bytes()).await.unwrap();

                                info!("{}", msg);
                            }
                            _ => unimplemented!("Other wait status are not implemented currently"),
                        }
                    }
                    Err(e) => unimplemented!("Waitpid error: {:?}", e),
                };
                stream_writer.lock().await.shutdown().await.unwrap();
            }
        }
    } else {
        debug!("[Daemon]: Detach, redirecting stdio to log file");
        let pty_to_log = tokio::spawn(async move {
            let mut buffer = vec![0u8; 1024];
            let mut pty_reader = container_reader.lock().await;
            loop {
                match pty_reader.read(&mut buffer).await {
                    Ok(0) => break, // EOF
                    Ok(n) => {
                        log_file.write_all(&buffer[..n]).await.unwrap();
                    }
                    Err(_e) => break,
                }
            }
        });

        // Child exits watcher
        let _ = tokio::join!(tokio::spawn(async move { waitpid(child, None) }));

        read_from_pty.abort();
        pty_to_log.abort();
    }

    do_stop(name, id);
}

async fn run_prepare(
    run_args: RunArgs,
) -> anyhow::Result<(OpenptyResult, (pipe::Sender, pipe::Receiver), ContainerMeta)> {
    // Generate name-id.
    let id = random_id();
    let name = run_args.name.unwrap_or_else(|| id.clone());
    let name_id = format!("{}-{}", name, id);

    // Root is where we store needed info and the image for the container.
    let root_path = format!("{}/{}", ROOT_PATH, name_id);
    // And the mnt is where we mount the image as container's sysroot.
    let mnt_path = format!("{}/{}/mnt", ROOT_PATH, name_id);

    // If not detach, we need to stream the container io to clients.
    let pty = openpty(None, None)?;

    // Sync between daemon and new child process (container).
    let pipe_p = pipe::pipe()?;
    let pipe_c = pipe::pipe()?;
    let pipe = (pipe_p.0, pipe_c.1);
    let mut buf = [0u8; 4];

    // Here we create the whole workspace.
    new_workspace(&run_args.image, &root_path, &mnt_path, &run_args.volume).await?;

    // Create a new process with new namespaces.
    let child =
        match new_container_process(&mnt_path, (&pipe_c.0, &pipe_p.1), &pty, &run_args.command) {
            Ok(child) => child,
            Err(e) => {
                // Clone child failure, clean up.
                let _ = delete_workspace(&root_path, &mnt_path, &run_args.volume).await;
                return Err(e);
            }
        };

    // Wait for child ready.
    read(pipe.1.as_raw_fd(), &mut buf).unwrap();
    if buf == *b"EXIT" {
        // Child failed to initialize, clean up.
        delete_workspace(&root_path, &mnt_path, &run_args.volume).await?;

        return Err(anyhow::anyhow!(
            "Failed to initialize container: child unexpected exit"
        ));
    }

    // Setting up cgroups
    let cg = match setup_cgroup(&name_id, child) {
        Ok(cg) => cg,
        Err(e) => {
            write(&pipe.0, b"EXIT").unwrap();
            let _ = delete_workspace(&root_path, &mnt_path, &run_args.volume).await;

            return Err(anyhow::anyhow!("Failed to setup cgroup: {:?}", e));
        }
    };

    // Form the container record.
    let cm = ContainerMeta::new(name, id, child.as_raw(), run_args.command);

    if let Err(e) = CONTAINER_METAS.register(cm.clone()).await {
        write(&pipe.0, b"EXIT").unwrap();
        let _ = delete_workspace(&root_path, &mnt_path, &run_args.volume).await;
        let _ = cg.delete();

        return Err(anyhow::anyhow!("Failed to register container: {:?}", e));
    }

    Ok((pty, pipe, cm))
}

/// This is the first process in the new namespace.
fn do_init(command: &Vec<String>) -> anyhow::Result<()> {
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
pub fn new_container_process(
    mnt_path: &str,
    pipe: (&pipe::Sender, &pipe::Receiver),
    pty: &OpenptyResult,
    command: &Vec<String>,
) -> anyhow::Result<Pid> {
    // NOTICE: In current impl, we always create new namespaces for the container, rather than
    // keep alive the old ones.
    let flags = CloneFlags::CLONE_NEWUTS
        | CloneFlags::CLONE_NEWPID
        | CloneFlags::CLONE_NEWNS
        | CloneFlags::CLONE_NEWNET
        | CloneFlags::CLONE_NEWIPC;
    const STACK_SIZE: usize = 1 * 1024 * 1024;
    let mut child_stack: Vec<u8> = vec![0; STACK_SIZE];

    let child_func = || {
        let setup_stdio = || -> anyhow::Result<()> {
            let _master_fd = pty.master.try_clone()?;
            let slave_fd = pty.slave.try_clone()?;

            // Redirect stdio.
            dup2(slave_fd.as_raw_fd(), nix::libc::STDIN_FILENO)?;
            dup2(slave_fd.as_raw_fd(), nix::libc::STDOUT_FILENO)?;
            dup2(slave_fd.as_raw_fd(), nix::libc::STDERR_FILENO)?;

            Ok(())
        };

        if let Err(e) = setup_stdio() {
            write(pipe.0, b"EXIT").unwrap();

            error!("Container initializer failure: {:?}", e);
            return -1;
        }

        // Switch root here.
        if let Err(e) = setup_mount(mnt_path) {
            write(pipe.0, b"EXIT").unwrap();

            error!("Container initializer failure: {:?}", e);
            return -1;
        }

        // Inform the parents ready.
        write(pipe.0, b"WAIT").unwrap();

        // Wait for parent ready.
        let mut buf = [0u8; 4];
        read(pipe.1.as_raw_fd(), &mut buf).unwrap();

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

fn setup_cgroup(cg_name: &str, child: Pid) -> anyhow::Result<Cgroup> {
    let hier = cgroups_rs::hierarchies::auto();
    let cg = match CgroupBuilder::new(&cg_name).build(hier) {
        Ok(cg) => cg,
        Err(e) => return Err(anyhow::anyhow!("Failed to create cgroup: {:?}", e)),
    };

    if let Err(e) = cg.add_task_by_tgid(CgroupPid::from(child.as_raw() as u64)) {
        cg.delete()?;
        return Err(anyhow::anyhow!("Failed to add task to cgroup: {:?}", e));
    }

    Ok(cg)
}

fn setup_mount(mnt_path: &str) -> anyhow::Result<()> {
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

fn switch_root(root: &str) -> anyhow::Result<()> {
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
