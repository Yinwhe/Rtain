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
    Msg::Req(cli.clone()).send_to(&mut stream).await.unwrap();

    match cli.command {
        Commands::Run(run_args) => client_run_container(run_args, stream).await,
        Commands::Start(start_args) => client_start_container(start_args, stream).await,
        // // Commands::Exec(exec_args) => exec_container(exec_args),
        Commands::Stop(stop_args) => client_stop_container(stop_args, stream).await,
        // // Commands::RM(rm_args) => remove_container(rm_args),
        Commands::PS(ps_args) => client_list_containers(ps_args, stream).await,
        Commands::Logs(logs_args) => client_show_logs(logs_args, stream).await,
        // // Commands::Commit(commit_args) => container::commit_container(commit_args),
        _ => unimplemented!(),
    }

    Ok(())
}

pub fn client() {
    if let Err(e) = Runtime::new().unwrap().block_on(run_client()) {
        eprintln!("Error: {}", e);
        exit(-1)
    } else {
        exit(0);
    }
}
