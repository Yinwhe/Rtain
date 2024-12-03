use log::{error, info};
use nix::{
    sys::signal::{kill, SIGKILL},
    unistd::Pid,
};

use crate::{records::ContainerStatus, StopArgs, RECORD_MANAGER};

/// Stop a running container.
pub fn stop_container(stop_args: StopArgs) {
    // Let's first get the container pid.
    let (pid, id) = {
        let bindings = RECORD_MANAGER.lock().unwrap();
        let cr = match bindings.container_with_name(&stop_args.name) {
            Ok(cr) => cr,
            Err(e) => {
                error!("Failed to stop container {}, due to: {}", stop_args.name, e);
                return;
            }
        };

        if !cr.status.is_running() {
            error!(
                "Failed to stop container {}, it's already stopped",
                stop_args.name
            );
            return;
        }

        (cr.pid.parse::<i32>().unwrap(), cr.id.clone())
    };

    // Ok kill it.
    let pid = Pid::from_raw(pid);
    if let Err(e) = kill(pid, SIGKILL) {
        error!("Failed to stop container {}, due to: {}", stop_args.name, e);
        return;
    }

    // Update records.
    match RECORD_MANAGER
        .lock()
        .unwrap()
        .set_status(&id, ContainerStatus::Stopped)
    {
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
