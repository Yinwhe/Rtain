use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};

use log::debug;
use nix::mount::{mount, umount2, MntFlags, MsFlags};

use crate::core::error::SimpleError;

pub fn new_workspace(
    image_path: &str,
    root_path: &str,
    mnt_path: &str,
    volume: &Option<String>,
) -> Result<(), SimpleError> {
    let image_path = Path::new(image_path);
    let root_path = Path::new(root_path);
    let mnt_path = Path::new(mnt_path);

    create_ro_layer(&image_path, &root_path)?;
    create_rw_layer(&root_path).map_err(|e| {
        // Clean up the ro layer.
        let _ = fs::remove_dir_all(root_path);
        e
    })?;
    create_mount_point(&root_path, &mnt_path).map_err(|e| {
        let _ = fs::remove_dir_all(root_path);
        e
    })?;

    if let Some(vol) = volume {
        let sv = vol.split(":").collect::<Vec<&str>>();
        if sv.len() == 2 && !sv[0].is_empty() && !sv[1].is_empty() {
            mount_volume(&mnt_path, sv).map_err(|e| {
                let _ = Command::new("umount").arg(mnt_path).status();
                let _ = fs::remove_dir_all(root_path);

                e
            })?;
        } else {
            let _ = Command::new("umount").arg(mnt_path).status();
            let _ = fs::remove_dir_all(root_path);

            return Err(format!("Invalid volume: {}", vol).into());
        }
    }

    debug!("[Daemon] Workspace created under {:?}", root_path);

    Ok(())
}

// Create a read-only layer, on the given image.
fn create_ro_layer(image_path: &Path, root_path: &Path) -> Result<(), SimpleError> {
    let image_dir = root_path.join("image");

    if !image_dir.exists() {
        fs::create_dir_all(&image_dir)?;

        let output = Command::new("tar")
            .arg("-xvf")
            .arg(&image_path)
            .arg("-C")
            .arg(&image_dir)
            .stdout(Stdio::null())
            .output()?;

        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).into());
        }
    }

    Ok(())
}

// Create a read-write layer, which is the container's write layer.
fn create_rw_layer(root_path: &Path) -> Result<(), SimpleError> {
    let write_dir = root_path.join("writeLayer");
    if !write_dir.exists() {
        fs::create_dir_all(&write_dir)?;
    }

    Ok(())
}

fn create_mount_point(root_path: &Path, mnt_path: &Path) -> Result<(), SimpleError> {
    let upperdir = root_path.join("writeLayer");
    let lowerdir = root_path.join("image");
    let workdir = root_path.join("work");

    if !workdir.exists() {
        fs::create_dir_all(&workdir)?;
    }

    if !mnt_path.exists() {
        fs::create_dir_all(mnt_path)?;
    }

    let mount_option = format!(
        "lowerdir={},upperdir={},workdir={}",
        lowerdir.display(),
        upperdir.display(),
        workdir.display()
    );

    let output = Command::new("mount")
        .arg("-t")
        .arg("overlay")
        .arg("overlay")
        .arg("-o")
        .arg(mount_option)
        .arg(mnt_path)
        .output()?;

    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into());
    }

    Ok(())
}

pub fn delete_workspace(
    root_path: &str,
    mnt_path: &str,
    volume: &Option<String>,
) -> Result<(), SimpleError> {
    let root_path = Path::new(root_path);
    let mnt_path = Path::new(mnt_path);

    if let Some(vol) = volume {
        let sv = vol.split(":").collect::<Vec<&str>>();

        assert!(sv.len() == 2 && !sv[0].is_empty() && !sv[1].is_empty());
        umount_volume(mnt_path, sv)?;
    }

    // Unmount the overlay filesystem.
    Command::new("umount").arg(mnt_path).status()?;
    // And simply delete the whole directory.
    fs::remove_dir_all(root_path)?;

    debug!("[Daemon] Workspace deleted under {:?}", root_path);

    Ok(())
}

fn mount_volume(mnt_path: &Path, volume_path: Vec<&str>) -> Result<(), SimpleError> {
    debug!("[Daemon] Mounting volume: {:?}", volume_path);

    let hostv = Path::new(volume_path[0]);
    let contv = mnt_path.join(volume_path[1].strip_prefix("/").unwrap());

    if !hostv.exists() {
        fs::create_dir_all(hostv)?;
    }

    if !contv.exists() {
        fs::create_dir_all(&contv)?;
    }

    mount(
        Some(hostv),
        &contv,
        None::<&str>,
        MsFlags::MS_BIND,
        None::<&str>,
    )?;

    Ok(())
}

fn umount_volume(mnt_path: &Path, volume_path: Vec<&str>) -> Result<(), SimpleError> {
    debug!("[Daemon] Unmounting volume: {:?}", volume_path);

    let contv = mnt_path.join(volume_path[1].strip_prefix("/").unwrap());

    umount2(&contv, MntFlags::MNT_DETACH)?;

    Ok(())
}
