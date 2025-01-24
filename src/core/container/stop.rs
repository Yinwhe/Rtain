use cgroups_rs::Cgroup;
use log::{error, info};
use tokio::net::UnixStream;

use crate::core::{cmd::StopArgs, records::ContainerStatus};
use crate::core::{Msg, RECORD_MANAGER};

/// Stop a running container.
pub async fn stop_container(stop_args: StopArgs, mut stream: UnixStream) {
    // Let's first get the container pid.
    let cr = match RECORD_MANAGER.get_record(&stop_args.name) {
        Some(cr) => cr,
        None => {
            error!(
                "Failed to stop container {}, record does not exist",
                &stop_args.name
            );

            let _ = Msg::Err(format!(
                "Failed to stop container {}, record does not exist",
                &stop_args.name
            ))
            .send_to(&mut stream)
            .await;

            return;
        }
    };

    do_stop(&cr.name, &cr.id);

    let _ = Msg::OkContent(format!("Container {} stoped", &stop_args.name))
        .send_to(&mut stream)
        .await;
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
    RECORD_MANAGER.update_record(&id, |r| r.status = ContainerStatus::Stopped);

    info!("[Daemon] Container {} stopped", name);
}
