use std::{
    ffi::CString,
    fs,
    os::fd::{AsRawFd, OwnedFd},
    path::Path,
    process::exit,
};

use cgroups_rs::{cgroup_builder::CgroupBuilder, Cgroup, CgroupPid};
use log::{debug, error, info};
use nix::{
    libc::SIGCHLD,
    mount::{mount, umount2, MntFlags, MsFlags},
    sched::{clone, CloneFlags},
    sys::wait::waitpid,
    unistd::{chdir, execvp, pipe, pivot_root, read, write, Pid},
};
use rand::{thread_rng, Rng};

use crate::{
    container::image::{delete_workspace, new_workspace},
    RunArgs,
};

/// When run a container command, it first creates a new process with new
/// namespaces and then runs the init command.
pub fn run(run_args: RunArgs) {
    // Create pipes
    let (read_fd, write_fd) = match pipe() {
        Ok((read_fd, write_fd)) => (read_fd, write_fd),
        Err(err) => {
            error!("Failed to create pipe: {:?}", err);
            exit(-1);
        }
    };

    // Create a new process with new namespaces
    let child = match new_container_process(&run_args.command, read_fd) {
        Ok(child) => child,
        Err(err) => {
            error!("Failed to create new namespace process: {:?}", err);
            exit(-1);
        }
    };

    // Generate name-id.
    let name_id = format!("rtain-{}", random_id());

    // Setting up cgroups
    let cg = match setup_cgroup(&name_id, child) {
        Ok(cg) => cg,
        Err(e) => {
            error!("Failed to setup cgroup: {:?}", e);

            // Clean up the child.
            write(write_fd, b"EXIT").unwrap();
            let _ = waitpid(child, None);

            exit(-1);
        }
    };

    // Here we create the new rootfs
    if let Err(e) = new_workspace("/tmp/rtain", "/tmp/rtain/mnt", &run_args.volume) {
        error!("Failed to create new workspace: {:?}", e);

        // Clean up...
        write(write_fd, b"EXIT").unwrap();
        let _ = waitpid(child, None);
        let _ = cg.delete();

        exit(-1);
    }

    // Let the init to continue.
    write(write_fd, b"CONT").unwrap();

    // Form the container record.
    // let cr = ContainerRecord::new(
    //     &name_id[..5],
    //     &name_id[5..],
    //     child.as_raw(),
    //     &run_args.command.join(" "),
    // );

    if !run_args.detach {
        match waitpid(child, None) {
            Ok(status) => {
                info!("Child process exited with status: {:?}", status);

                let _ = cg.delete();
                let _ = delete_workspace("/tmp/rtain", "/tmp/rtain/mnt", &run_args.volume);
            }
            Err(err) => {
                error!("Failed to wait for child process: {:?}", err);
            }
        }
    }
    // Or detach from its child.
}

/// This is the first process in the new namespace.
fn do_init(command: &Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    setup_mount()?;

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
    command: &Vec<String>,
    read_fd: OwnedFd,
) -> Result<Pid, Box<dyn std::error::Error>> {
    let flags = CloneFlags::CLONE_NEWUTS
        | CloneFlags::CLONE_NEWPID
        | CloneFlags::CLONE_NEWNS
        | CloneFlags::CLONE_NEWNET
        | CloneFlags::CLONE_NEWIPC;

    const STACK_SIZE: usize = 1 * 1024 * 1024;
    let mut child_stack: Vec<u8> = vec![0; STACK_SIZE];

    // Child function
    let child_func = || {
        // Wait for cgroups setting
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

fn setup_mount() -> Result<(), Box<dyn std::error::Error>> {
    // Make the mount namespace private
    mount(
        None::<&str>,
        "/",
        None::<&str>,
        MsFlags::MS_REC | MsFlags::MS_PRIVATE,
        None::<&str>,
    )?;

    // Switch to new root
    switch_root("/tmp/rtain/mnt")?;

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
    fs::create_dir_all(&pivot_dir)?;

    // Execute `pivot_root` to switch the new root to `root`
    pivot_root(root, pivot_dir.as_str())?;

    // To the new working directory
    chdir("/")?;

    // Unmount the old root
    let pivot_dir_old = "/.pivot_root";
    umount2(pivot_dir_old, MntFlags::MNT_DETACH)?;

    // Remove the old root
    fs::remove_dir_all(pivot_dir_old)?;

    Ok(())
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

fn random_id() -> String {
    let mut rng = thread_rng();
    let random_bytes: [u8; 16] = rng.gen();

    random_bytes
        .iter()
        .map(|byte| format!("{:02x}", byte))
        .collect()
}
