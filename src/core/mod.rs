use std::env;

use log::{debug, error, info};
use metas::{ContainerManager, CONTAINER_METAS};
use network::{create_network, NETWORKS};
use tokio::{
    net::{UnixListener, UnixStream},
    task,
};

mod cmd;
mod container;
mod metas;
mod msg;
mod network;

use container::*;

pub use cmd::*;
pub use msg::*;

pub const ROOT_PATH: &str = "/tmp/rtain";
pub const SOCKET_PATH: &str = "/tmp/rtain_daemons.sock";

async fn run_daemon() -> tokio::io::Result<()> {
    env::set_var("RUST_LOG", "debug");
    env_logger::init();
    console_subscriber::init();

    let container_metas = ContainerManager::default()
        .await
        .expect("Fatal, failed to init container metas");
    CONTAINER_METAS
        .set(container_metas)
        .expect("Fatal, failed to set container metas");

    let networks = network::Networks::load(format!("{ROOT_PATH}/net/networks"))
        .expect("Fatal, failed to init network metas");
    NETWORKS
        .set(tokio::sync::Mutex::new(networks))
        .expect("Fatal, failed to set network metas");

    // Delete the old socket file
    if std::fs::exists(SOCKET_PATH).unwrap_or(false) {
        std::fs::remove_file(SOCKET_PATH)?;
    }

    let listener = UnixListener::bind(SOCKET_PATH)?;

    info!(
        "[Daemon]: Daemon is running and listening on {}",
        SOCKET_PATH
    );

    while let Ok((stream, addr)) = listener.accept().await {
        debug!("[Daemon]: Accepted client connection on {addr:?}");

        let _handler = task::spawn(handler(stream));
    }

    info!("[Daemon]: Daemon is exiting");
    Ok(())
}

async fn handler(mut stream: UnixStream) -> tokio::io::Result<()> {
    let msg = match Msg::recv_from(&mut stream).await {
        Ok(msg) => msg,
        Err(e) => {
            error!("[Daemon] failed to get msg: {:?}", e);
            return Err(e);
        }
    };

    let cli = match msg.get_req() {
        Some(cli) => cli,
        None => {
            error!("[Daemon] Invalid message format");
            return Ok(());
        }
    };
    match cli.command {
        Commands::Run(run_args) => run_container(run_args, stream).await,
        Commands::Start(start_args) => start_container(start_args, stream).await,
        Commands::Exec(exec_args) => exec_container(exec_args, stream).await,
        Commands::Stop(stop_args) => stop_container(stop_args, stream).await,
        Commands::RM(rm_args) => remove_container(rm_args, stream).await,
        Commands::PS(ps_args) => list_containers(ps_args, stream).await,
        Commands::Logs(logs_args) => show_logs(logs_args, stream).await,
        Commands::Commit(commit_args) => commit_container(commit_args, stream).await,
        Commands::Network(network_commands) => match network_commands {
            NetworkCommands::Create(netcreate_args) => create_network(netcreate_args, stream).await,
        },
    };

    debug!("[Daemon]: Task done, daemon disconnected");
    Ok(())
}

pub fn daemon() {
    let runtime = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("Failed to create tokio runtime: {}", e);
            return;
        }
    };
    
    if let Err(e) = runtime.block_on(run_daemon()) {
        eprintln!("Error: {}", e);
    }
}
