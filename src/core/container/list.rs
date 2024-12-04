use std::fs::read_to_string;
use std::io::Write;

use log::error;
use tabwriter::TabWriter;

use crate::core::cmd::{LogsArgs, PSArgs};
use crate::core::RECORD_MANAGER;
use crate::ROOT_PATH;

pub fn list_containers(_ps_args: PSArgs) {
    let records = RECORD_MANAGER.get_all_records();

    let mut tw = TabWriter::new(vec![]);
    let _ = tw.write_all(b"ID\tNAME\tPID\tCOMMAND\tSTATUS\n");

    for record in records {
        let _ = writeln!(
            tw,
            "{}\t{}\t{}\t{}\t{:?}",
            record.id, record.name, record.pid, record.command, record.status
        );
    }

    let _ = tw.flush();

    let output = String::from_utf8(tw.into_inner().unwrap()).unwrap();
    println!("{}", output);
}

pub fn show_logs(log_args: LogsArgs) {
    let cr = match RECORD_MANAGER.get_record(&log_args.name) {
        Some(cr) => cr,
        None => {
            error!(
                "Failed to get container {} record, record does not exist",
                &log_args.name
            );
            return;
        }
    };

    let name_id = format!("{}-{}", cr.name, cr.id);

    let path = format!("{}/{}/stdout.log", ROOT_PATH, name_id);
    let logs = match read_to_string(path) {
        Ok(logs) => logs,
        Err(e) => {
            error!("Failed to read logs: {}", e);
            return;
        }
    };

    println!("{}", logs);
}
