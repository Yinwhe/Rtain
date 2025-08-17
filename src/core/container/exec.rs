use std::{
    ffi::CString,
    io::{Read, Write},
    os::{
        fd::{AsRawFd, BorrowedFd},
        unix::net::UnixStream as StdUnixStream,
    },
    process::exit,
};

use log::error;
use nix::{
    fcntl::{open, OFlag},
    libc::SIGCHLD,
    pty::{openpty, OpenptyResult},
    sched::{clone, setns, CloneFlags},
    sys::{
        stat::Mode,
        wait::{waitpid, WaitStatus},
    },
    unistd::{dup2, execvp, fork, ForkResult, Pid},
};
use tokio::net::UnixStream;

use crate::core::{
    cmd::ExecArgs,
    metas::{ContainerMeta, CONTAINER_METAS},
    Msg,
};

use super::init::do_run;

/// Enter a container.
pub async fn exec_container(exec_args: ExecArgs, mut stream: UnixStream) {
    // Let's first get the container pid.
    let meta = match CONTAINER_METAS
        .get()
        .unwrap()
        .get_meta_by_name(&exec_args.name)
        .await
    {
        Some(meta) => meta,
        None => {
            error!(
                "Failed to exec container {}, record does not exist",
                &exec_args.name
            );
            let _ = Msg::Err(format!(
                "Failed to exec container {}, record does not exist",
                &exec_args.name
            ))
            .send_to(&mut stream)
            .await;

            return;
        }
    };

    if !meta.state.status.is_running() {
        error!(
            "Failed to exec container {}, it's not running",
            exec_args.name
        );
        let _ = Msg::Err(format!(
            "Failed to exec container {}, it's not running",
            &exec_args.name
        ))
        .send_to(&mut stream)
        .await;

        return;
    }

    let (pty, sock, child) = match exec_prepare(&meta).await {
        Ok(res) => res,
        Err(e) => {
            error!("Failed to start container: {:?}", e);
            let _ = Msg::Err(e.to_string()).send_to(&mut stream).await;

            return;
        }
    };

    log::warn!("exec with child {child}");

    do_run(meta.name, meta.id, child, pty, sock, stream, false, false).await;
}

async fn exec_prepare(meta: &ContainerMeta) -> anyhow::Result<(OpenptyResult, StdUnixStream, Pid)> {
    // let name_id = format!("{}-{}", &meta.name, &meta.id);

    let pty = openpty(None, None)?;

    // Sync between daemon and child process.
    let (mut p_sock, c_sock) = StdUnixStream::pair()?;
    let mut buf = [0u8; 4];

    // Create a new process in the container ns.
    let container_pid = match meta.get_pid() {
        Some(pid) => pid,
        None => return Err(anyhow::anyhow!("Container is not running")),
    };

    let child = match exec_container_process(container_pid, c_sock, &pty, &meta.command) {
        Ok(child) => child,
        Err(e) => {
            return Err(e);
        }
    };

    // Wait for child ready.
    p_sock.read_exact(&mut buf).unwrap();
    match &buf {
        b"EXIT" => {
            return Err(anyhow::anyhow!(
                "Failed to initialize container: child unexpected exit"
            ));
        }
        b"WAIT" => {}
        _ => unreachable!(),
    }

    // Setup cgroup settings
    // let hier = cgroups_rs::hierarchies::auto();
    // let cg = Cgroup::load(hier, name_id);
    // if let Err(e) = cg.add_task_by_tgid(CgroupPid::from(child.as_raw() as u64)) {
    //     p_sock.write(b"EXIT").unwrap();

    //     return Err(anyhow::anyhow!("Failed to add task to cgroup: {:?}", e));
    // }

    Ok((pty, p_sock, child))
}

fn exec_container_process(
    container: i32,
    mut c_sock: StdUnixStream,
    pty: &OpenptyResult,
    command: &Vec<String>,
) -> anyhow::Result<Pid> {
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

        // Enter the namespaces of the container first.
        if let Err(e) = enter_ns(container) {
            c_sock.write(b"EXIT").unwrap();

            error!(
                "Failed to exec in container, cannot enter namespace: {:?}",
                e
            );
            return -1;
        }

        // SAFETY: fork() is used to create a child process that will execute
        // commands in the container namespace. This is a standard pattern
        // for container exec implementations.
        match unsafe { fork() } {
            Err(e) => {
                c_sock.write(b"EXIT").unwrap();

                error!("Failed to exec in container, cannot fork: {:?}", e);
                return -1;
            }
            Ok(forkresult) => {
                match forkresult {
                    ForkResult::Parent { child } => {
                        let code = match waitpid(child, None).unwrap() {
                            WaitStatus::Exited(_, code) => code as i32,
                            WaitStatus::Signaled(_, sig, _) => sig as i32,
                            _ => -1,
                        };

                        exit(code)
                    }
                    ForkResult::Child => {
                        if let Err(e) = setup_stdio() {
                            c_sock.write(b"EXIT").unwrap();

                            error!("Failed to exec in container, cannot redirect io: {:?}", e);
                            return -1;
                        }

                        // Inform the parents ready.
                        c_sock.write(b"WAIT").unwrap();

                        // Wait for parent ready.
                        let mut buf = [0u8; 4];
                        c_sock.read_exact(&mut buf).unwrap();

                        match &buf {
                            b"EXIT" => {
                                return -1;
                            }
                            b"CONT" => {}
                            _ => unreachable!(),
                        }

                        if let Err(e) = do_exec(command) {
                            error!("Failed to exec in container: {:?}", e);
                            return -1;
                        }
                    }
                }
            }
        }

        return 0;
    };

    let child = unsafe {
        clone(
            Box::new(child_func),
            &mut child_stack,
            CloneFlags::empty(),
            Some(SIGCHLD),
        )
    }?;

    Ok(child)
}

fn enter_ns(pid: i32) -> anyhow::Result<()> {
    for ns in ["ipc", "uts", "net", "pid", "mnt"] {
        let nspath = format!("/proc/{}/ns/{}", pid, ns);
        let fd = open(nspath.as_str(), OFlag::O_RDONLY, Mode::empty())?;

        // SAFETY: BorrowedFd::borrow_raw is used to create a borrowed file descriptor
        // from the raw fd. This is safe as long as the fd is valid (which it is,
        // since open() succeeded) and we don't use it after it's closed.
        setns(unsafe { BorrowedFd::borrow_raw(fd) }, CloneFlags::empty())?;
    }

    Ok(())
}

fn do_exec(command: &Vec<String>) -> anyhow::Result<()> {
    let command_cstr = CString::new(command[0].clone())?;
    let args_cstr: Vec<CString> = command
        .iter()
        .map(|arg| CString::new(arg.clone()).unwrap())
        .collect();

    execvp(&command_cstr, &args_cstr)?;

    Ok(())
}
