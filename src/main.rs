use std::env;
use std::process::exit;

use clap::{Parser, Subcommand};
use log::error;

mod container;
use container::run;

#[derive(Parser, Debug)]
#[command(name = "rtain")]
#[command(about = "rtain is a simple container runtime implemented in Rust.")]
struct CLI {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    Run {
        /// Memory limit for the container.
        #[arg(short, long, value_parser(parse_memory_size))]
        memory: Option<i64>,

        /// Stablize using the volume mount.
        #[arg(short, long)]
        volume: Option<String>,

        /// Command to run in the container.
        #[arg(allow_hyphen_values = true)]
        command: Vec<String>,
    },
}

fn main() {
    env::set_var("RUST_LOG", "debug");
    env_logger::init();

    let cli = CLI::parse();

    match cli.command {
        Commands::Run {
            memory,
            volume,
            command,
        } => {
            if let Err(e) = run(memory, volume, command) {
                error!("Failed to run container: {:?}", e);
                exit(-1);
            }
        }
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
