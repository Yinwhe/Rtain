use cgroups_rs::Cgroup;
use log::{error, info};

use crate::core::RECORD_MANAGER;
use crate::core::{cmd::StopArgs, records::ContainerStatus};

/// Stop a running container.
pub async fn stop_container(stop_args: StopArgs) {
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

    do_stop(&cr.name, &cr.id);
}

pub fn do_stop(name: &str, id: &str) {
    let name_id = format!("{name}-{id}");

    // Get current cgroups
    let hier = cgroups_rs::hierarchies::auto();
    let cg = Cgroup::load(hier, name_id);

    // Cgroup kills
    if let Err(e) = cg.kill() {
        error!("Failed to stop container {}: {}", name, e);
        return;
    }

    // Update records.
    RECORD_MANAGER.set_status(&id, ContainerStatus::Stopped);

    info!("[Daemon] Container {} stopped", name);
}
