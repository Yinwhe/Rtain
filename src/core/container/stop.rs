use cgroups_rs::Cgroup;
use log::{error, info};
use tokio::net::UnixStream;

use crate::core::{
    cmd::StopArgs,
    metas::{ContainerStatus, CONTAINER_METAS},
    Msg,
};

/// Stop a running container.
pub async fn stop_container(stop_args: StopArgs, mut stream: UnixStream) {
    // Let's first get the container pid.
    let meta = match CONTAINER_METAS.get_meta_by_name(&stop_args.name).await {
        Some(meta) => meta,
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

    do_stop(meta.name, meta.id);

    let _ = Msg::OkContent(format!("Container {} stoped", &stop_args.name))
        .send_to(&mut stream)
        .await;
}

pub async fn do_stop(name: String, id: String) {
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
    CONTAINER_METAS.updates(id, ContainerStatus::stop()).await;

    info!("[Daemon] Container {} stopped", name);
}
