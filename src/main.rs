use std::fs;
use std::process::{exit, Command};
use std::os::unix::prelude::CommandExt;

use clap::{Parser, Subcommand};
use nix::sched::{self, CloneFlags};

#[derive(Parser, Debug)]
#[command(name = "rtain")]
#[command(about = "rtain is a simple container runtime implemented in Rust.")]
struct CLI {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Initializes something
    Init,

    /// Runs a command with optional flags
    Run {
        /// Command to run
        command: Option<String>,
    },
}

fn main() {
    let cli = CLI::parse();

    match cli.command {
        Commands::Run { command } => {
            if let Some(c) = command {
                run(c);
            } else {
                println!("No command provided.");
            }
        }
        Commands::Init => {
            println!("Initializing container...");
        }
    }
}

fn run(command: String) {
    let mut parent = new_parent_process(command);
    match parent.spawn() {
        Ok(mut child) => {
            match child.wait() {
                Ok(status) => {
                    if !status.success() {
                        println!("Child process exited with error: {:?}", status);
                        exit(-1);
                    }
                }
                Err(e) => {
                    println!("Failed to wait for child process: {:?}", e);
                    exit(-1);
                }
            }

        }
        Err(e) => {
            println!("Failed to start parent process: {:?}", e);
            exit(-1);
        }
    }
}

fn new_parent_process(command: String) -> Command {
    let args = vec!["init", command.as_str()];

    // create new namespaces
    let mut cmd = Command::new("/proc/self/exe");
    cmd.args(&args);

    unsafe {
        cmd.pre_exec(|| {
            // 设置命名空间
            sched::unshare(
                CloneFlags::CLONE_NEWUTS
                    | CloneFlags::CLONE_NEWPID
                    | CloneFlags::CLONE_NEWNS
                    | CloneFlags::CLONE_NEWNET
                    | CloneFlags::CLONE_NEWIPC,
            )?;
            Ok(())
        });
    }

    cmd.stdin(fs::File::open("/dev/tty").unwrap());
    cmd.stdout(fs::File::open("/dev/tty").unwrap());
    cmd.stderr(fs::File::open("/dev/tty").unwrap());

    cmd
}
