use std::env;

use lazy_static::lazy_static;
use log::{debug, info};
use tokio::{
    net::{UnixListener, UnixStream},
    task,
};

mod cmd;
mod container;
mod error;
mod msg;
mod records;

use container::*;
use records::ContainerManager;

pub use cmd::*;
pub use msg::*;

pub const ROOT_PATH: &str = "/tmp/rtain";
pub const SOCKET_PATH: &str = "/tmp/rtain_daemons.sock";

lazy_static! {
    static ref RECORD_MANAGER: ContainerManager = ContainerManager::init(ROOT_PATH)
        .expect("Fatal, failed to initialize the container manager");
}

async fn run_daemon() -> tokio::io::Result<()> {
    env::set_var("RUST_LOG", "debug");
    env_logger::init();

    // Delete the old socket file
    if std::fs::exists(SOCKET_PATH).unwrap() {
        std::fs::remove_file(SOCKET_PATH)?;
    }

    let listener = UnixListener::bind(SOCKET_PATH)?;

    info!(
        "[Daemon]: Daemon is running and listening on {}",
        SOCKET_PATH
    );

    while let Ok((stream, _addr)) = listener.accept().await {
        debug!("[Daemon]: Accepted client connection");

        // FIXME: sync and resource shall be taken care.
        let _handler = task::spawn(handler(stream));
    }

    info!("[Daemon]: Daemon is exiting");
    Ok(())
}

async fn handler(mut stream: UnixStream) -> tokio::io::Result<()> {
    let msg = Msg::recv_from(&mut stream).await.unwrap();
    debug!("Received msg: {:?}", msg);

    let cli = msg.get_req().unwrap();

    match cli.command {
        Commands::Run(run_args) => run_container(run_args, stream).await,
        // Commands::Start(start_args) => start_container(start_args),
        // Commands::Exec(exec_args) => exec_container(exec_args),
        // Commands::Stop(stop_args) => stop_container(stop_args),
        // Commands::RM(rm_args) => remove_container(rm_args),
        // Commands::PS(ps_args) => list_containers(ps_args),
        // Commands::Logs(logs_args) => show_logs(logs_args),
        // Commands::Commit(commit_args) => container::commit_container(commit_args),
        _ => unimplemented!(),
    };

    debug!("[Daemon]: Task done, daemon disconnected");
    Ok(())
}

pub fn daemon() {
    if let Err(e) = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(run_daemon())
    {
        eprintln!("Error: {}", e);
    }
}
