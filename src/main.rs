use std::env;
use std::process::exit;

use clap::{Parser, Subcommand};
use log::{error, info};

mod container;
use container::{init, run};

#[derive(Parser, Debug)]
#[command(name = "rtain")]
#[command(about = "rtain is a simple container runtime implemented in Rust.")]
struct CLI {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    Init { command: String },

    Run { command: String },
}

fn main() {
    env::set_var("RUST_LOG", "info");
    env_logger::init();

    let cli = CLI::parse();

    info!("Enter main function");

    match cli.command {
        Commands::Run { command } => {
            run(command);
        }
        Commands::Init { command } => {
            info!("Initializing container...");
            if let Err(e) = init(command) {
                error!("Failed to initialize container: {:?}", e);
                exit(-1);
            }
        }
    }
}
