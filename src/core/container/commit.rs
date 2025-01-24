use std::{path::Path, process::Command};

use log::{debug, error};

use crate::core::cmd::CommitArgs;
use crate::core::metas::CONTAINER_METAS;
use crate::core::ROOT_PATH;

pub async fn commit_container(cm_args: CommitArgs) {
    let meta = match CONTAINER_METAS.get_meta_by_name(&cm_args.name).await {
        Some(meta) => meta,
        None => {
            error!(
                "Failed to commit container {}, record does not exist",
                &cm_args.name
            );
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

    if !mnt_path.exists() {
        error!(
            "Failed to commit container {}, mount path not existed",
            &cm_args.name
        );
        return;
    }

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
            return;
        }
    };

    if !output.status.success() {
        error!(
            "Failed to commit container {}, tar command failed",
            &cm_args.name
        );
        return;
    }
}
