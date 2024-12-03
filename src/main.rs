use std::{env, sync::Mutex};

use clap::{Args, Parser, Subcommand};

mod container;
use container::{
    exec_container, list_containers, remove_container, run_container, show_logs, start_container,
    stop_container,
};

mod records;
use records::ContainerManager;

lazy_static::lazy_static! {
    pub static ref RECORD_MANAGER: Mutex<ContainerManager> = Mutex::new(ContainerManager::init().unwrap());
}

#[derive(Parser, Debug)]
#[command(name = "rtain")]
#[command(about = "rtain is a simple container runtime implemented in Rust.")]
struct CLI {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Running a container from images.
    Run(RunArgs),
    /// Start a stoped container.
    Start(StartArgs),
    /// Enter a running container.
    Exec(ExecArgs),
    /// Stop a container.
    Stop(StopArgs),
    /// Remove a stopped container.
    RM(RMArgs),
    /// List containers.
    PS(PSArgs),
    /// Show a container's log.
    Logs(LogsArgs),
    /// Commit a container to an image.
    Commit(CommitArgs),
}

#[derive(Args, Debug)]
struct RunArgs {
    /// Name of the container.
    #[arg(short, long)]
    name: Option<String>,

    /// Memory limit for the container.
    #[arg(short, long, value_parser(parse_memory_size))]
    memory: Option<i64>,

    /// Stabilize using the volume mount.
    #[arg(short, long)]
    volume: Option<String>,

    /// Detach the container.
    #[arg(short, long)]
    detach: bool,

    /// Image to run.
    #[arg(required = true)]
    image: String,

    /// Command to run in the container.
    #[arg(allow_hyphen_values = true, required = true)]
    command: Vec<String>,
}

#[derive(Args, Debug)]
struct StartArgs {
    /// Name of the container.
    #[arg(required = true)]
    name: String,
    /// Interactive mode.
    #[arg(short, long)]
    interactive: bool,
}

#[derive(Args, Debug)]
struct ExecArgs {
    /// Name of the container.
    #[arg(short, long)]
    name: String,

    /// Command to run in the container.
    #[arg(allow_hyphen_values = true, required = true)]
    command: Vec<String>,
}

#[derive(Args, Debug)]
struct StopArgs {
    name: String,
}

#[derive(Args, Debug)]
struct RMArgs {
    name: String,
}

#[derive(Args, Debug)]
struct PSArgs {
    #[arg(short, long)]
    all: bool,
}

#[derive(Args, Debug)]
struct LogsArgs {
    name: String,
}

#[derive(Args, Debug)]
struct CommitArgs {
    /// Name of the container to commit.
    name: String,
    /// Committed image name.
    image: String,
}


fn main() {
    env::set_var("RUST_LOG", "debug");
    env_logger::init();

    let cli = CLI::parse();

    match cli.command {
        Commands::Run(run_args) => run_container(run_args),
        Commands::Start(start_args) => start_container(start_args),
        Commands::Exec(exec_args) => exec_container(exec_args),
        Commands::Stop(stop_args) => stop_container(stop_args),
        Commands::RM(rm_args) => remove_container(rm_args),
        Commands::PS(ps_args) => list_containers(ps_args),
        Commands::Logs(logs_args) => show_logs(logs_args),
        Commands::Commit(commit_args) => container::commit_container(commit_args),
    }
}

/// Parse a memory size string into bytes.
fn parse_memory_size(input: &str) -> Result<i64, String> {
    let input = input.trim().to_lowercase();

    let (number, multiplier): (&str, i64) = if input.ends_with("g") {
        (&input[..input.len() - 1], 1024 * 1024 * 1024) // GB
    } else if input.ends_with("m") {
        (&input[..input.len() - 1], 1024 * 1024) // MB
    } else if input.ends_with("k") {
        (&input[..input.len() - 1], 1024) // KB
    } else if input.chars().all(|c| c.is_digit(10)) {
        (input.as_str(), 1) // default is B
    } else {
        return Err("Invalid memory size".into());
    };

    let number: i64 = match number.parse() {
        Ok(n) => n,
        Err(e) => return Err(e.to_string()),
    };

    Ok(number * multiplier)
}
