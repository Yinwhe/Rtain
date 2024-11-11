use std::{path::Path, process::Command};

use log::{debug, error};

use crate::{CommitArgs, RECORD_MANAGER};

pub fn commit_container(cm_args: CommitArgs) {
    let bindings = RECORD_MANAGER.lock().unwrap();
    let cr = match bindings.container_with_name(&cm_args.name) {
        Ok(cr) => cr,
        Err(e) => {
            error!(
                "Failed to commit container {}, due to: {}",
                &cm_args.name, e
            );
            return;
        }
    };

    let name_id = format!("{}-{}", cr.name, cr.id);
    drop(bindings);

    let mnt_path = Path::new("/tmp/rtain").join(name_id).join("mnt");
    let image_path = Path::new(&cm_args.image).join(format!("{}.tar", cm_args.image));

    debug!("Commit container {} to image {}", &cm_args.name, &cm_args.image);

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
