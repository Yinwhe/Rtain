use clap::{Args, Parser, Subcommand};
use serde::{Deserialize, Serialize};

#[derive(Parser, Debug, Serialize, Deserialize, Clone)]
#[command(name = "rtain")]
#[command(about = "rtain is a simple container runtime implemented in Rust.")]
pub struct CLI {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug, Serialize, Deserialize, Clone)]
pub enum Commands {
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

#[derive(Args, Debug, Serialize, Deserialize, Clone)]
pub struct RunArgs {
    /// Name of the container.
    #[arg(short, long)]
    pub name: Option<String>,

    /// Memory limit for the container.
    #[arg(short, long, value_parser(parse_memory_size))]
    pub memory: Option<i64>,

    /// Stabilize using the volume mount.
    #[arg(short, long)]
    pub volume: Option<String>,

    /// Detach the container.
    #[arg(short, long)]
    pub detach: bool,

    /// Image to run.
    #[arg(required = true)]
    pub image: String,

    /// Command to run in the container.
    #[arg(allow_hyphen_values = true, required = true)]
    pub command: Vec<String>,
}

#[derive(Args, Debug, Serialize, Deserialize, Clone)]
pub struct StartArgs {
    /// Name of the container.
    #[arg(required = true)]
    pub name: String,
    /// Interactive mode.
    #[arg(short, long)]
    pub interactive: bool,
}

#[derive(Args, Debug, Serialize, Deserialize, Clone)]
pub struct ExecArgs {
    /// Name of the container.
    #[arg(short, long)]
    pub name: String,

    /// Command to run in the container.
    #[arg(allow_hyphen_values = true, required = true)]
    pub command: Vec<String>,
}

#[derive(Args, Debug, Serialize, Deserialize, Clone)]
pub struct StopArgs {
    pub name: String,
}

#[derive(Args, Debug, Serialize, Deserialize, Clone)]
pub struct RMArgs {
    pub name: String,
}

#[derive(Args, Debug, Serialize, Deserialize, Clone)]
pub struct PSArgs {
    #[arg(short, long)]
    pub all: bool,
}

#[derive(Args, Debug, Serialize, Deserialize, Clone)]
pub struct LogsArgs {
    pub name: String,
}

#[derive(Args, Debug, Serialize, Deserialize, Clone)]
pub struct CommitArgs {
    /// Name of the container to commit.
    pub name: String,
    /// Committed image name.
    pub image: String,
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
