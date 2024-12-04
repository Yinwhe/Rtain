use std::{
    env, fs,
    io::{BufReader, Read},
    os::unix::net::{UnixListener, UnixStream},
};

use lazy_static::lazy_static;
use log::{debug, info};

use records::ContainerManager;

mod cmd;
mod error;
mod container;
mod records;

pub use cmd::CLI;

pub const ROOT_PATH: &str = "/tmp/rtain";

lazy_static! {
    static ref RECORD_MANAGER: ContainerManager = ContainerManager::init(ROOT_PATH)
        .expect("Fatal, failed to initialize the container manager");
}

pub fn daemon() -> std::io::Result<()> {
    env::set_var("RUST_LOG", "debug");
    env_logger::init();

    let socket_path = "/tmp/rtain_demons.sock";

    // Delete the old socket file
    if fs::metadata(socket_path).is_ok() {
        fs::remove_file(socket_path)?;
    }

    // Create the UNIX socket listener
    let listener = UnixListener::bind(socket_path)?;
    info!(
        "[Daemon]: Daemon is running and listening on {}",
        socket_path
    );

    for stream in listener.incoming() {
        let stream = stream?;

        debug!("[Daemon]: Accepted client connection");
        handler(stream)?;
    }

    info!("[Daemon]: Daemon is exiting");
    Ok(())
}

fn handler(stream: UnixStream) -> std::io::Result<()> {
    let mut reader = BufReader::new(&stream);
    let mut message = String::new();

    reader.read_to_string(&mut message)?;
    let cli: CLI = serde_json::from_str(&message).unwrap();

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
