use std::{env, process::exit};

use clap::Parser;
use tokio::{net::UnixStream, runtime::Runtime};

use crate::core::{Commands, Msg, CLI, SOCKET_PATH};

use super::ops::*;

async fn run_client() -> tokio::io::Result<()> {
    env::set_var("RUST_LOG", "debug");
    env_logger::init();

    // Connect to the daemon
    let mut stream = UnixStream::connect(SOCKET_PATH).await?;

    let cli = CLI::parse();
    if let Err(e) = Msg::Req(cli.clone()).send_to(&mut stream).await {
        eprintln!("Failed to send request to daemon: {}", e);
        return Err(e);
    }

    match cli.command {
        Commands::Run(run_args) => client_run_container(run_args, stream).await,
        Commands::Start(start_args) => client_start_container(start_args, stream).await,
        Commands::Exec(exec_args) => client_exec_container(exec_args, stream).await,
        Commands::Stop(stop_args) => client_stop_container(stop_args, stream).await,
        Commands::RM(rm_args) => client_remove_container(rm_args, stream).await,
        Commands::PS(ps_args) => client_list_containers(ps_args, stream).await,
        Commands::Logs(logs_args) => client_show_logs(logs_args, stream).await,
        Commands::Commit(commit_args) => client_commit_container(commit_args, stream).await,
        Commands::Network(network_commands) => match network_commands {
            crate::core::NetworkCommands::Create(netcreate_args) => {
                client_create_network(netcreate_args, stream).await
            }
        },
    }

    Ok(())
}

pub fn client() {
    let runtime = match Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("Failed to create tokio runtime: {}", e);
            exit(-1);
        }
    };

    if let Err(e) = runtime.block_on(run_client()) {
        eprintln!("Error: {}", e);
        exit(-1)
    } else {
        exit(0);
    }
}
