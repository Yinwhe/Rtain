use std::env;

use async_std::{io::{ReadExt, WriteExt}, os::unix::net::UnixStream, task};
use clap::Parser;
use log::{info, debug};

use crate::core::{CLI, SOCKET_PATH};

async fn run_client() -> async_std::io::Result<()> {
    env::set_var("RUST_LOG", "debug");
    env_logger::init();

    // Connect to the daemon
    let mut stream = UnixStream::connect(SOCKET_PATH).await?;
    info!("[Client]: Connected to daemon");

    let cli = CLI::parse();
    let cli_str = serde_json::to_string(&cli)?;

    stream.write_all(cli_str.as_bytes()).await?;

    // let mut msg = String::new();
    // stream.read_to_string(&mut msg).await?;

    // debug!("[Client]: Received message: {}", msg);

    Ok(())
}

pub fn client() {
    if let Err(e) = task::block_on(run_client()) {
        eprintln!("Error: {}", e);
    }
}
