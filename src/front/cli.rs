use std::{env, process::exit};

use clap::Parser;
use tokio::{net::UnixStream, runtime::Runtime};

use crate::{
    core::{Commands, Msg, CLI, SOCKET_PATH},
    front::ops::client_run_container,
};

async fn run_client() -> tokio::io::Result<()> {
    env::set_var("RUST_LOG", "debug");
    env_logger::init();

    // Connect to the daemon
    let mut stream = UnixStream::connect(SOCKET_PATH).await?;

    let cli = CLI::parse();
    Msg::Req(cli.clone()).send_to(&mut stream).await.unwrap();

    match cli.command {
        Commands::Run(run_args) => client_run_container(stream, run_args).await,
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
