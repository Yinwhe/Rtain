use std::io::{Read, Write};

use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::UnixStream,
};

use crate::core::{Response, RunArgs};

pub async fn client_run_container(mut stream: UnixStream, args: RunArgs) {
    if args.detach {
        // Detach run, just exit with no more oprations.
    } else {
        let resp = Response::recv_from(&mut stream).await;
        match resp {
            Ok(Response::Continue) => {} // Ok continue the process.
            _ => {
                eprintln!("Unexpected response from daemon: {:?}", resp);
                return;
            }
        }

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
}
