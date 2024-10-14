use std::fs::read_to_string;
use std::io::Write;

use log::{debug, error};
use tabwriter::TabWriter;

use crate::{LogsArgs, PSArgs, RECORD_MANAGER};

pub fn list_containers(_ps_args: PSArgs) {
    let mut bindings = RECORD_MANAGER.lock().unwrap();
    let records = match bindings.all_container() {
        Ok(records) => records,
        Err(e) => {
            error!("Failed to list containers: {}", e);
            return;
        }
    };

    debug!("List of containers: {:?}", records);

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

    drop(bindings);

    let output = String::from_utf8(tw.into_inner().unwrap()).unwrap();
    println!("{}", output);
}

pub fn show_logs(log_args: LogsArgs) {
    let cr = match RECORD_MANAGER
        .lock()
        .unwrap()
        .container_with_name(&log_args.name)
    {
        Ok(cr) => cr.clone(),
        Err(e) => {
            error!("Failed to get container record: {}", e);
            return;
        }
    };

    let name_id = format!("{}-{}", cr.name, cr.id);

    let path = format!("/tmp/rtain/{}/stdout.log", name_id);
    let logs = match read_to_string(path) {
        Ok(logs) => logs,
        Err(e) => {
            error!("Failed to read logs: {}", e);
            return;
        }
    };

    println!("{}", logs);
}
