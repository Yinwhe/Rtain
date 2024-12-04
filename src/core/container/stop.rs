use log::{error, info};
use nix::{
    sys::signal::{kill, SIGKILL},
    unistd::Pid,
};

use crate::core::RECORD_MANAGER;
use crate::core::{cmd::StopArgs, records::ContainerStatus};

/// Stop a running container.
pub fn stop_container(stop_args: StopArgs) {
    // Let's first get the container pid.
    let cr = match RECORD_MANAGER.get_record(&stop_args.name) {
        Some(cr) => cr,
        None => {
            error!(
                "Failed to stop container {}, record does not exist",
                &stop_args.name
            );
            return;
        }
    };

    // Ok kill it.
    let pid = Pid::from_raw(cr.pid.parse::<i32>().unwrap());
    if let Err(e) = kill(pid, SIGKILL) {
        error!("Failed to stop container {}, due to: {}", stop_args.name, e);
        return;
    }

    // Update records.
    match RECORD_MANAGER.set_status(&cr.id, ContainerStatus::Stopped) {
        Ok(_) => {
            info!("Container {} stopped", stop_args.name);
        }
        Err(e) => {
            error!(
                "Failed to stop container {}: cannot update status, {}",
                stop_args.name, e
            );
        }
    }
}
