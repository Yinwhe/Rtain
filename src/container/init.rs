use std::{ffi::CString, os::fd::AsRawFd, process::exit};

use log::{error, info};
use nix::{
    libc::SIGCHLD,
    mount::{mount, MsFlags},
    sched::{clone, CloneFlags},
    sys::wait::waitpid,
    unistd::{execv, Pid},
};

// When run a container command, it first creates a new process with new
// namespaces and then runs the init command.
pub fn run(command: String) {
    let child = match new_parent_process(true, &command) {
        Ok(child) => child,
        Err(err) => {
            error!("Failed to create new namespace process: {:?}", err);
            exit(-1);
        }
    };

    info!("Child process created: {:?}", child);

    match waitpid(child, None) {
        Ok(status) => {
            info!("Child process exited with status: {:?}", status);
        }
        Err(err) => {
            error!("Failed to wait for child process: {:?}", err);
            exit(-1);
        }
    }
}

// This is the first process in the new namespace.
pub fn init(command: String) -> Result<(), Box<dyn std::error::Error>> {
    info!("Init Command: {}", command);

    let default_mount_flags = MsFlags::MS_NOEXEC | MsFlags::MS_NOSUID | MsFlags::MS_NODEV;

    // Mount /proc
    mount(
        Some("proc"),
        "/proc",
        Some("proc"),
        default_mount_flags,
        None::<&str>,
    )?;

    let command_cstr = CString::new(command)?;
    let args_cstr: Vec<CString> = Vec::new();

    execv(&command_cstr, &args_cstr)?;

    Ok(())
}

fn new_parent_process(tty: bool, command: &str) -> Result<Pid, nix::Error> {
    let flags = CloneFlags::CLONE_NEWUTS
        | CloneFlags::CLONE_NEWPID
        | CloneFlags::CLONE_NEWNS
        | CloneFlags::CLONE_NEWNET
        | CloneFlags::CLONE_NEWIPC;

    const STACK_SIZE: usize = 1024 * 1024;
    let mut child_stack: Vec<u8> = vec![0; STACK_SIZE];

    // clone new child process
    let child_pid = unsafe {
        clone(
            Box::new(|| child_func(tty, command)),
            &mut child_stack,
            flags,
            Some(SIGCHLD),
        )
    }?;

    Ok(child_pid)
}

fn child_func(tty: bool, command: &str) -> isize {
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

    println!("Child pid: {}", nix::unistd::getpid());
    std::thread::sleep(std::time::Duration::from_secs(10));
    println!("2");
    0

    // let args = vec![
    //     CString::new("init").unwrap(),
    //     CString::new(command).unwrap(),
    // ];

    // let prog = CString::new("/proc/self/exe").unwrap();
    // match execv(&prog, &args) {
    //     Ok(_) => 0,
    //     Err(err) => {
    //         error!("Failed to execv: {:?}", err);
    //         -1
    //     }
    // }
}
