use std::{env, io::Write, os::unix::net::UnixStream};

use clap::Parser;
use log::info;

use crate::core::CLI;

pub fn client() -> std::io::Result<()> {
    env::set_var("RUST_LOG", "debug");
    env_logger::init();

    let socket_path = "/tmp/rtain_demons.sock";

    // Connect to the daemon
    let mut stream = UnixStream::connect(socket_path)?;

    info!("[Client]: Connected to daemon");

    let cli = CLI::parse();
    let cli_str = serde_json::to_string(&cli)?;

    stream.write_all(cli_str.as_bytes())?;

    Ok(())
}
