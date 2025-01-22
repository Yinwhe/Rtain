use std::{
    ffi::CString,
    os::fd::{AsRawFd, OwnedFd},
};

use cgroups_rs::{Cgroup, CgroupPid};
use log::{error, info};
use nix::{
    libc::SIGCHLD,
    mount::{mount, umount2, MntFlags, MsFlags},
    pty::{openpty, OpenptyResult},
    sched::{clone, CloneFlags},
    unistd::{chdir, dup2, execvp, pipe, pivot_root, read, write, Pid},
};
use tokio::net::UnixStream;

use super::init::do_run;
use crate::core::records::{ContainerRecord, ContainerStatus};
use crate::core::{cmd::StartArgs, error::SimpleError};
use crate::core::{RECORD_MANAGER, ROOT_PATH};

pub async fn start_container(start_args: StartArgs, stream: UnixStream) {
    let mut cr = match RECORD_MANAGER.get_record(&start_args.name) {
        Some(cr) => cr,
        None => {
            error!(
                "Failed to start container {}, record does not exist",
                &start_args.name
            );
            // FIXME: Error return.

            return;
        }
    };

    if cr.status.is_running() {
        error!(
            "Failed to start container {}, it's already running",
            &start_args.name
        );
        // FIXME: Error return.

        return;
    }

    let (pty, pipe, child) = match start_prepare(&cr) {
        Ok(res) => res,
        Err(e) => {
            error!("Failed to start container: {:?}", e);
            // FIXME: Error return.

            return;
        }
    };

    cr.pid = child.as_raw();

    // Updates records.
    RECORD_MANAGER.set_pid(&cr.id, cr.pid);
    RECORD_MANAGER.set_status(&cr.id, ContainerStatus::Running);

    do_run(
        &cr.name,
        &cr.id,
        Pid::from_raw(cr.pid),
        pty,
        pipe,
        stream,
        start_args.detach,
    )
    .await;
}

fn start_prepare(
    cr: &ContainerRecord,
) -> Result<(OpenptyResult, (OwnedFd, OwnedFd), Pid), SimpleError> {
    let name_id = format!("{}-{}", &cr.name, &cr.id);
    // And the mnt is where we mount the image as container's sysroot.
    let mnt_path = format!("{}/{}/mnt", ROOT_PATH, name_id);

    let pty = openpty(None, None)?;

    // Sync between daemon and new child process (container).
    let pipe_p = pipe()?;
    let pipe_c = pipe()?;
    let pipe = (pipe_p.0, pipe_c.1);
    let mut buf = [0u8; 4];

    // Create a new process with old namespaces.
    let child = match new_container_process(&mnt_path, (&pipe_c.0, &pipe_p.1), &pty, &cr.command) {
        Ok(child) => child,
        Err(e) => {
            // Clone child failure, clean up.
            return Err(e);
        }
    };

    // Wait for child ready.
    read(pipe.0.as_raw_fd(), &mut buf).unwrap();
    if buf == *b"EXIT" {
        // Child failed to start, clean up.
        return Err("child exits".into());
    }

    // Get the old cgroups
    let hier = cgroups_rs::hierarchies::auto();
    let cg = Cgroup::load(hier, name_id);

    cg.add_task_by_tgid(CgroupPid::from(child.as_raw() as u64))?;

    Ok((pty, pipe, child))
}

/// Run the commands
fn do_start(command: &Vec<String>) -> Result<(), SimpleError> {
    let command_cstr = CString::new(command[0].clone())?;
    let args_cstr: Vec<CString> = command
        .iter()
        .map(|arg| CString::new(arg.clone()).unwrap())
        .collect();

    info!("Ready to start command: {:?}", command);
    execvp(&command_cstr, &args_cstr)?;

    Ok(())
}

/// Create a new process with the namespaces and return its pid.
fn new_container_process(
    mnt_path: &str,
    pipe: (&OwnedFd, &OwnedFd),
    pty: &OpenptyResult,
    command: &Vec<String>,
) -> Result<Pid, SimpleError> {
    // NOTICE: In current impl, we won't keep ns alive, so reset them here.
    let flags = CloneFlags::CLONE_NEWUTS
        | CloneFlags::CLONE_NEWPID
        | CloneFlags::CLONE_NEWNS
        | CloneFlags::CLONE_NEWNET
        | CloneFlags::CLONE_NEWIPC;
    const STACK_SIZE: usize = 1 * 1024 * 1024;
    let mut child_stack: Vec<u8> = vec![0; STACK_SIZE];

    let child_func = || {
        let setup_stdio = || -> Result<(), SimpleError> {
            let _master_fd = pty.master.try_clone()?;
            let slave_fd = pty.slave.try_clone()?;

            // Redirect stdio.
            dup2(slave_fd.as_raw_fd(), nix::libc::STDIN_FILENO)?;
            dup2(slave_fd.as_raw_fd(), nix::libc::STDOUT_FILENO)?;
            dup2(slave_fd.as_raw_fd(), nix::libc::STDERR_FILENO)?;

            Ok(())
        };

        if let Err(e) = setup_stdio() {
            write(pipe.1, b"EXIT").unwrap();

            error!("Container start failure: {:?}", e);
            return -1;
        }

        // Switch root here (no need to set up the mount again).
        if let Err(e) = setup_mount(mnt_path) {
            write(pipe.1, b"EXIT").unwrap();

            error!("Container start failure: {:?}", e);
            return -1;
        }

        // Inform the parents ready.
        write(pipe.1, b"WAIT").unwrap();

        // Wait for parent ready.
        let mut buf = [0u8; 4];
        read(pipe.0.as_raw_fd(), &mut buf).unwrap();

        match &buf {
            b"EXIT" => {
                error!("Failed to start container");
                return -1;
            }
            b"CONT" => {}
            _ => unreachable!(),
        }

        if let Err(e) = do_start(command) {
            error!("Failed to start container: {:?}", e);
            return -1;
        }

        return 0;
    };

    // This new process will run `child_func`
    let child_pid = unsafe { clone(Box::new(child_func), &mut child_stack, flags, Some(SIGCHLD)) }?;

    Ok(child_pid)
}

fn setup_mount(mnt_path: &str) -> Result<(), SimpleError> {
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
    let _ = std::fs::create_dir("/proc");

    mount(
        Some("proc"),
        "/proc",
        Some("proc"),
        MsFlags::MS_NOEXEC | MsFlags::MS_NOSUID | MsFlags::MS_NODEV,
        None::<&str>,
    )?;

    Ok(())
}

fn switch_root(root: &str) -> Result<(), SimpleError> {
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
    let _ = std::fs::create_dir_all(&pivot_dir);

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
