use std::env;

use async_std::{
    io::ReadExt,
    os::unix::net::{UnixListener, UnixStream},
    stream::StreamExt,
    task,
};
use lazy_static::lazy_static;
use log::{debug, info};

use records::ContainerManager;

mod cmd;
mod container;
mod error;
mod records;

pub use cmd::CLI;

pub const ROOT_PATH: &str = "/tmp/rtain";
pub const SOCKET_PATH: &str = "/tmp/rtain_demons.sock";

lazy_static! {
    static ref RECORD_MANAGER: ContainerManager = ContainerManager::init(ROOT_PATH)
        .expect("Fatal, failed to initialize the container manager");
}

pub fn daemon() {
    if let Err(e) = task::block_on(run_daemon()) {
        eprintln!("Error: {}", e);
    }
}

async fn run_daemon() -> async_std::io::Result<()> {
    env::set_var("RUST_LOG", "debug");
    env_logger::init();

    // Delete the old socket file
    if std::fs::exists(SOCKET_PATH).is_ok() {
        std::fs::remove_file(SOCKET_PATH)?;
    }

    let listener = UnixListener::bind(SOCKET_PATH).await?;
    let mut incomming = listener.incoming();

    info!(
        "[Daemon]: Daemon is running and listening on {}",
        SOCKET_PATH
    );

    while let Some(stream) = incomming.next().await {
        let stream = stream?;
        debug!("[Daemon]: Accepted client connection");

        handler(stream).await?;
    }

    info!("[Daemon]: Daemon is exiting");
    Ok(())
}

async fn handler(mut stream: UnixStream) -> async_std::io::Result<()> {
    let mut message = String::new();

    loop {
        let size = stream.read_to_string(&mut message).await?;
        if size == 0 {
            // OK, connection done
            debug!("[Daemon]: Client disconnected");
            break;
        }

        let cli = serde_json::from_str::<CLI>(&message)?;
        debug!("[Daemon] cli: {:#?}", cli);
    }

    // match cli.command {
    //     Commands::Run(run_args) => run_container(run_args),
    //     Commands::Start(start_args) => start_container(start_args),
    //     Commands::Exec(exec_args) => exec_container(exec_args),
    //     Commands::Stop(stop_args) => stop_container(stop_args),
    //     Commands::RM(rm_args) => remove_container(rm_args),
    //     Commands::PS(ps_args) => list_containers(ps_args),
    //     Commands::Logs(logs_args) => show_logs(logs_args),
    //     Commands::Commit(commit_args) => container::commit_container(commit_args),
    // }

    Ok(())
}
