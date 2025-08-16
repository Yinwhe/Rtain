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
    let container_metas = match CONTAINER_METAS.get() {
        Some(metas) => metas,
        None => {
            error!("Container metas not initialized");
            let _ = Msg::Err("Container metas not initialized".to_string())
                .send_to(&mut stream)
                .await;
            return;
        }
    };
    
    let meta = match container_metas
        .get_meta_by_name(&stop_args.name)
        .await
    {
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

    do_stop(meta.name, meta.id).await;

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
    if let Some(container_metas) = CONTAINER_METAS.get() {
        let _ = container_metas.updates(id, ContainerStatus::stop()).await;
    } else {
        error!("Container metas not initialized during stop");
    }

    info!("[Daemon] Container {} stopped", name);
}
