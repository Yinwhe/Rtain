use std::fs::read_to_string;
use std::io::Write;

use log::error;
use tabwriter::TabWriter;
use tokio::net::UnixStream;

use crate::core::cmd::{LogsArgs, PSArgs};
use crate::core::metas::CONTAINER_METAS;
use crate::core::{Msg, ROOT_PATH};

pub async fn list_containers(_ps_args: PSArgs, mut stream: UnixStream) {
    let metas = CONTAINER_METAS.get().unwrap().get_all_metas().await;

    let mut tw = TabWriter::new(vec![]);
    let _ = tw.write_all(b"ID\tNAME\tPID\tCOMMAND\tSTATUS\n");

    for meta in metas {
        let _ = writeln!(
            tw,
            "{}\t{}\t{}\t{}\t{:?}",
            meta.id,
            meta.name,
            meta.get_pid().unwrap_or(0),
            meta.command.join(" "),
            meta.status
        );
    }

    match tw.into_inner() {
        Ok(data) => {
            let _ = Msg::OkContent(String::from_utf8(data).unwrap())
                .send_to(&mut stream)
                .await;
        }
        Err(e) => {
            error!("Failed to write to tab writer: {}", e);

            let _ = Msg::Err(format!("Failed to write to tab writer: {}", e))
                .send_to(&mut stream)
                .await;
        }
    }
}

pub async fn show_logs(log_args: LogsArgs, mut stream: UnixStream) {
    let meta = match CONTAINER_METAS
        .get()
        .unwrap()
        .get_meta_by_name(&log_args.name)
        .await
    {
        Some(meta) => meta,
        None => {
            error!(
                "Failed to get container {} meta, does not exist",
                &log_args.name
            );

            let _ = Msg::Err(format!(
                "Failed to get container {} meta, does not exist",
                &log_args.name
            ))
            .send_to(&mut stream)
            .await;

            return;
        }
    };

    let name_id = format!("{}-{}", meta.name, meta.id);

    let path = format!("{}/{}/stdout.log", ROOT_PATH, name_id);
    let logs = match read_to_string(path) {
        Ok(logs) => logs,
        Err(e) => {
            error!("Failed to read logs: {}", e);

            let _ = Msg::Err(format!("Failed to read logs: {}", e))
                .send_to(&mut stream)
                .await;

            return;
        }
    };

    let _ = Msg::OkContent(logs).send_to(&mut stream).await;
}
