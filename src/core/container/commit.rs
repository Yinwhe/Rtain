use std::{path::Path, process::Command};

use log::{debug, error};
use tokio::net::UnixStream;

use crate::core::cmd::CommitArgs;
use crate::core::metas::CONTAINER_METAS;
use crate::core::{Msg, ROOT_PATH};

pub async fn commit_container(cm_args: CommitArgs, mut stream: UnixStream) {
    let meta = match CONTAINER_METAS
        .get()
        .unwrap()
        .get_meta_by_name(&cm_args.name)
        .await
    {
        Some(meta) => meta,
        None => {
            error!(
                "Failed to commit container {}, record does not exist",
                &cm_args.name
            );

            let _ = Msg::Err(format!(
                "Failed to commit container {}, record does not exist",
                cm_args.name
            ))
            .send_to(&mut stream)
            .await;

            return;
        }
    };

    let name_id = format!("{}-{}", meta.name, meta.id);

    let mnt_path = Path::new(ROOT_PATH).join(name_id).join("mnt");
    let image_path = Path::new(&cm_args.image).join(format!("{}.tar", cm_args.image));

    debug!(
        "Commit container {}({}) to image {}({})",
        &cm_args.name,
        &mnt_path.to_string_lossy(),
        &cm_args.image,
        &image_path.to_string_lossy()
    );

    // Use tar command to create an image tarball
    let output = match Command::new("tar")
        .arg("-czf")
        .arg(image_path)
        .arg("-C")
        .arg(mnt_path)
        .arg(".")
        .output()
    {
        Ok(output) => output,
        Err(e) => {
            error!(
                "Failed to commit container {}, due to: {}",
                &cm_args.name, e
            );

            let _ = Msg::Err(format!(
                "Failed to commit container {}, cannot tar the image: {}",
                cm_args.name, e
            ))
            .send_to(&mut stream)
            .await;

            return;
        }
    };

    if !output.status.success() {
        error!(
            "Failed to commit container {}, tar command failed",
            &cm_args.name
        );
        let error = String::from_utf8_lossy(&output.stderr);
        let _ = Msg::Err(format!(
            "Failed to commit container {}, tar command failed: {}",
            cm_args.name, error
        ))
        .send_to(&mut stream)
        .await;

        return;
    }

    let _ = Msg::OkContent(format!(
        "Container {} commited to image {}",
        cm_args.name, cm_args.image
    ))
    .send_to(&mut stream)
    .await;
}
