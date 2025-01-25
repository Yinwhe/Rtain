use std::{ffi::CString, os::fd::BorrowedFd};

use log::{debug, error, info};
use nix::{
    fcntl::{open, OFlag},
    libc::SIGCHLD,
    sched::{clone, setns, CloneFlags},
    sys::{stat::Mode, wait::waitpid},
    unistd::execvp,
};

use crate::core::{cmd::ExecArgs, metas::CONTAINER_METAS};

/// Enter a container.
pub async fn exec_container(exec_args: ExecArgs) {
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
            return;
        }
    };

    if !meta.status.is_running() {
        error!(
            "Failed to exec container {}, it's not running",
            exec_args.name
        );
        return;
    }

    let pid = meta.get_pid().unwrap();
    // Clone and exec into the container.
    const STACK_SIZE: usize = 1 * 1024 * 1024;
    let mut child_stack: Vec<u8> = vec![0; STACK_SIZE];

    let child_func = || {
        // Enter the namespaces of the container.
        match enter_ns(pid) {
            Ok(_) => {}
            Err(e) => {
                error!("Failed to enter namespaces: {}", e);
                return -1;
            }
        }

        // Now we can exec into the container.
        let command_cstr = CString::new(exec_args.command[0].clone()).unwrap();
        let args_cstr: Vec<CString> = exec_args
            .command
            .iter()
            .map(|arg| CString::new(arg.clone()).unwrap())
            .collect();

        debug!(
            "Ready to exec into container {} with command {:?}",
            exec_args.name, command_cstr
        );

        if let Err(e) = execvp(&command_cstr, &args_cstr) {
            error!("Failed to exec into container: {}", e);
            return -1;
        }

        return 0;
    };

    let child = unsafe {
        match clone(
            Box::new(child_func),
            &mut child_stack,
            CloneFlags::empty(),
            Some(SIGCHLD),
        ) {
            Ok(pid) => pid,
            Err(e) => {
                error!("Failed to exec container {}, due to: {}", exec_args.name, e);
                return;
            }
        }
    };

    match waitpid(child, None) {
        Ok(status) => {
            info!("Exec process exited with status: {:?}", status);
        }
        Err(err) => {
            error!("Failed to wait for exec process: {:?}", err);
        }
    };
}

fn enter_ns(pid: i32) -> Result<(), Box<dyn std::error::Error>> {
    debug!("Entering namespace for container with pid {}", pid);

    // Now we can exec into the container.
    for ns in ["ipc", "uts", "net", "pid", "mnt"] {
        let nspath = format!("/proc/{}/ns/{}", pid, ns);
        let fd = open(nspath.as_str(), OFlag::O_RDONLY, Mode::empty())?;

        setns(unsafe { BorrowedFd::borrow_raw(fd) }, CloneFlags::empty())?;
    }

    Ok(())
}
