use cgroups_rs::Cgroup;
use log::error;
use tokio::net::UnixStream;

use super::image::delete_workspace;
use crate::core::cmd::RMArgs;
use crate::core::metas::CONTAINER_METAS;
use crate::core::{Msg, ROOT_PATH};

pub async fn remove_container(rm_args: RMArgs, mut stream: UnixStream) {
    let meta = match CONTAINER_METAS
        .get()
        .unwrap()
        .get_meta_by_name(&rm_args.name)
        .await
    {
        Some(meta) => meta,
        None => {
            error!(
                "Failed to rm container {}, record does not exist",
                &rm_args.name
            );
            return;
        }
    };

    if meta.status.is_running() {
        error!(
            "Failed to rm container {}, it's still running",
            &rm_args.name
        );
        return;
    }

    // Do some clean up.
    let name_id = format!("{}-{}", meta.name, meta.id);
    let root_path = format!("{}/{}", ROOT_PATH, name_id);
    let mnt_path = format!("{}/{}/mnt", ROOT_PATH, name_id);

    let hier = cgroups_rs::hierarchies::auto();
    let cg = Cgroup::load(hier, name_id);

    if let Err(e) = cg.delete() {
        error!(
            "Failed to rm container {}, cannot clean up cgroup: {}",
            &rm_args.name, e
        );
    }

    // TODO: volume support needed.
    if let Err(e) = delete_workspace(&root_path, &mnt_path, &None).await {
        error!(
            "Failed to rm container {}, cannot clean up workspace: {}",
            &rm_args.name, e
        );
    }

    if let Err(e) = CONTAINER_METAS.get().unwrap().deregister(meta.id).await {
        error!(
            "Failed to rm container {}, cannot deregister container: {}",
            &rm_args.name, e
        );
    }

    let _ = Msg::OkContent(format!("Container {} removed", &rm_args.name))
        .send_to(&mut stream)
        .await;
}
