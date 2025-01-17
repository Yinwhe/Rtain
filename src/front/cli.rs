use std::env;

use clap::Parser;
use tokio::{io::AsyncWriteExt, net::UnixStream};

use crate::{
    core::{Commands, CLI, SOCKET_PATH},
    front::ops::client_run_container,
};

async fn run_client() -> tokio::io::Result<()> {
    env::set_var("RUST_LOG", "debug");
    env_logger::init();

    // Connect to the daemon
    let mut stream = UnixStream::connect(SOCKET_PATH).await?;

    let cli = CLI::parse();

    let mut cli_str = serde_json::to_string(&cli).unwrap();
    cli_str.push('\n');
    stream.write_all(cli_str.as_bytes()).await?;

    match cli.command {
        Commands::Run(run_args) => client_run_container(stream, run_args).await,
        _ => unimplemented!(),
    }

    Ok(())
}

pub fn client() {
    if let Err(e) = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(run_client())
    {
        eprintln!("Error: {}", e);
    }
}
