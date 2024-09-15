use std::{
    ffi::CString,
    fs,
    os::fd::{AsRawFd, OwnedFd},
    path::Path,
    process::exit,
};

use cgroups_rs::{cgroup_builder::CgroupBuilder, CgroupPid};
use log::{error, info};
use nix::{
    libc::SIGCHLD,
    mount::{mount, MsFlags},
    sched::{clone, CloneFlags},
    sys::wait::waitpid,
    unistd::{execv, pipe, read, write, Pid},
};

// When run a container command, it first creates a new process with new
// namespaces and then runs the init command.
pub fn run(mem_limit: Option<i64>, command: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    // Create pipes
    let (read_fd, write_fd) = pipe()?;

    let child = match new_container_process(true, &command, read_fd) {
        Ok(child) => child,
        Err(err) => {
            error!("Failed to create new namespace process: {:?}", err);
            exit(-1);
        }
    };

    // setting up cgroups
    let mut cg = None;
    if let Some(mem_limit) = mem_limit {
        let hier = cgroups_rs::hierarchies::auto();
        let cg_inner = CgroupBuilder::new("rtain_cg")
            .memory()
            .kernel_memory_limit(mem_limit)
            .memory_hard_limit(mem_limit)
            .done()
            .build(hier)?;

        if let Err(e) = cg_inner.add_task_by_tgid(CgroupPid::from(child.as_raw() as u64)) {
            cg_inner.delete()?;
            return Err(Box::new(e));
        }

        cg = Some(cg_inner);
    }

    // Let the init to continue.
    write(write_fd, b"CONT")?;

    match waitpid(child, None) {
        Ok(status) => {
            info!("Child process exited with status: {:?}", status);
            if let Some(cg) = cg {
                cg.delete()?;
            }
            Ok(())
        }
        Err(err) => Err(Box::new(err)),
    }
}

// This is the first process in the new namespace.
fn do_init(command: &Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    // Make the mount namespace private
    mount(
        None::<&str>,
        "/",
        None::<&str>,
        MsFlags::MS_REC | MsFlags::MS_PRIVATE,
        None::<&str>,
    )?;

    // Mount new proc fs
    if !Path::new("/proc").exists() {
        fs::create_dir("/proc")?;
    }

    mount(
        Some("proc"),
        "/proc",
        Some("proc"),
        MsFlags::MS_NOEXEC | MsFlags::MS_NOSUID | MsFlags::MS_NODEV,
        None::<&str>,
    )?;

    let command_cstr = CString::new(command[0].clone())?;
    let args_cstr: Vec<CString> = command
        .iter()
        .map(|arg| CString::new(arg.clone()).unwrap())
        .collect();

    execv(&command_cstr, &args_cstr)?;

    Ok(())
}

fn new_container_process(
    tty: bool,
    command: &Vec<String>,
    read_fd: OwnedFd,
) -> Result<Pid, nix::Error> {
    let flags = CloneFlags::CLONE_NEWUTS
        | CloneFlags::CLONE_NEWPID
        | CloneFlags::CLONE_NEWNS
        | CloneFlags::CLONE_NEWNET
        | CloneFlags::CLONE_NEWIPC;

    const STACK_SIZE: usize = 1 * 1024 * 1024;
    let mut child_stack: Vec<u8> = vec![0; STACK_SIZE];

    // Child function
    let child_func = || {
        // if enable tty, inherit the tty from parent
        if tty {
            let stdin_fd = std::io::stdin().as_raw_fd();
            let stdout_fd = std::io::stdout().as_raw_fd();
            let stderr_fd = std::io::stderr().as_raw_fd();

            unsafe {
                use nix::libc::dup2;
                if dup2(stdin_fd, 0) == -1 {
                    error!("Failed to dup2 stdin");
                    return -1;
                }
                if dup2(stdout_fd, 1) == -1 {
                    error!("Failed to dup2 stdout");
                    return -1;
                }
                if dup2(stderr_fd, 2) == -1 {
                    error!("Failed to dup2 stderr");
                    return -1;
                }
            }
        }

        // Wait for cgroups setting
        let mut buffer = [0u8; 4];
        read(read_fd.as_raw_fd(), &mut buffer).unwrap();

        if &buffer != b"CONT" {
            error!("Container received an unexpected signal: {:?}", buffer);
            return -1;
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
