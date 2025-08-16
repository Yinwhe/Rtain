use std::io::{Read, Write};

use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::UnixStream,
};

use crate::core::*;

pub async fn client_run_container(args: RunArgs, stream: UnixStream) {
    client_do_run(args.detach, stream).await;
}

pub async fn client_start_container(args: StartArgs, stream: UnixStream) {
    client_do_run(args.detach, stream).await;
}

pub async fn client_exec_container(_args: ExecArgs, stream: UnixStream) {
    client_do_run(false, stream).await;
}

pub async fn client_stop_container(args: StopArgs, mut stream: UnixStream) {
    match Msg::recv_from(&mut stream).await {
        Ok(msg) => match msg {
            Msg::OkContent(cont) => println!("{cont}"),
            Msg::Err(e) => eprintln!("Failed to stop container {}, due to: {e}", args.name),
            _ => unreachable!(),
        },
        Err(e) => {
            eprintln!("Failed to recv msg from daemon: {e}");
        }
    }
}

pub async fn client_list_containers(_args: PSArgs, mut stream: UnixStream) {
    match Msg::recv_from(&mut stream).await {
        Ok(msg) => match msg {
            Msg::OkContent(cont) => println!("{cont}"),
            Msg::Err(e) => eprintln!("Failed to list containers, due to: {e}"),
            _ => unreachable!(),
        },
        Err(e) => {
            eprintln!("Failed to recv msg from daemon: {e}");
        }
    }
}

pub async fn client_show_logs(args: LogsArgs, mut stream: UnixStream) {
    match Msg::recv_from(&mut stream).await {
        Ok(msg) => match msg {
            Msg::OkContent(cont) => println!("{cont}"),
            Msg::Err(e) => eprintln!(
                "Failed to show log for container {}, due to: {e}",
                args.name
            ),
            _ => unreachable!(),
        },
        Err(e) => {
            eprintln!("Failed to recv msg from daemon: {e}");
        }
    }
}

pub async fn client_remove_container(_args: RMArgs, mut stream: UnixStream) {
    match Msg::recv_from(&mut stream).await {
        Ok(msg) => match msg {
            Msg::OkContent(cont) => println!("{cont}"),
            Msg::Err(e) => eprintln!("Failed to rm containers, due to: {e}"),
            _ => unreachable!(),
        },
        Err(e) => {
            eprintln!("Failed to recv msg from daemon: {e}");
        }
    }
}

pub async fn client_commit_container(_args: CommitArgs, mut stream: UnixStream) {
    match Msg::recv_from(&mut stream).await {
        Ok(msg) => match msg {
            Msg::OkContent(cont) => println!("{cont}"),
            Msg::Err(e) => eprintln!("Failed to commit container, due to: {e}"),
            _ => unreachable!(),
        },
        Err(e) => {
            eprintln!("Failed to recv msg from daemon: {e}");
        }
    }
}

#[inline]
async fn client_do_run(detach: bool, mut stream: UnixStream) {
    if detach {
        // Detach run, just exit with no more oprations.
    } else {
        let resp = Msg::recv_from(&mut stream).await;
        match resp {
            Ok(Msg::Continue) => {} // Ok continue the process.
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
            }
            Ok::<(), tokio::io::Error>(())
        });

        // let _ = tokio::join!(write_to_daemon, read_from_daemon);
        let _ = tokio::join!(read_from_daemon);
        write_to_daemon.abort();
    }
}

pub async fn client_create_network(args: crate::core::NetCreateArgs, mut stream: UnixStream) {
    match Msg::recv_from(&mut stream).await {
        Ok(msg) => match msg {
            Msg::OkContent(cont) => println!("{cont}"),
            Msg::Err(e) => eprintln!(
                "Failed to create network {}, due to: {e}",
                args.name
            ),
            _ => eprintln!("Unexpected response from daemon"),
        },
        Err(e) => {
            eprintln!("Failed to recv msg from daemon: {e}");
        }
    }
}
