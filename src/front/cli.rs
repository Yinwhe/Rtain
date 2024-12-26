use std::{
    env,
    io::{Read, Write},
};

use clap::Parser;
use log::{debug, info};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::UnixStream,
};

use crate::core::{Response, CLI, SOCKET_PATH};

async fn run_client() -> tokio::io::Result<()> {
    env::set_var("RUST_LOG", "debug");
    env_logger::init();

    // Connect to the daemon
    let mut stream = UnixStream::connect(SOCKET_PATH).await?;
    info!("[Client]: Connected to daemon");

    let mut cli_str = serde_json::to_string(&CLI::parse()).unwrap();
    cli_str.push('\n');

    stream.write_all(cli_str.as_bytes()).await?;

    let mut bufreader = BufReader::new(&mut stream);
    let mut response = String::new();
    let size = bufreader.read_line(&mut response).await?;

    if size > 0 {
        debug!("[Client] msg: {:?}", response);
    } else {
        debug!("[Client] No data received, is daemon dead?");
    }

    let resp: Response = serde_json::from_str(&response)?;
    if resp.is_cont() {
        let (mut reader, mut writer) = stream.into_split();
        // Read stdin and send to daemon.
        let write_to_daemon = tokio::spawn(async move {
            let mut stdin = std::io::stdin();
            let mut buffer = vec![0u8; 1024];
            loop {
                let bytes_read = stdin.read(&mut buffer)?;
                if bytes_read == 0 {
                    // Stdin closed.
                    break;
                }
                writer.write_all(&buffer[..bytes_read]).await?;
            }
            Ok::<(), tokio::io::Error>(())
        });

        // From daemon to stdout.
        let read_from_daemon = tokio::spawn(async move {
            let mut stdout = std::io::stdout();
            let mut buffer = vec![0u8; 1024];
            loop {
                let bytes_read = reader.read(&mut buffer).await?;
                if bytes_read == 0 {
                    // Daemon closed.
                    break;
                }
                stdout.write_all(&buffer[..bytes_read])?;
                stdout.flush()?;
            }
            Ok::<(), tokio::io::Error>(())
        });

        let _ = tokio::join!(write_to_daemon, read_from_daemon);
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
