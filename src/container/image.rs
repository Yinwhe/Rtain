use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use log::debug;

pub fn new_workspace(root_path: &str, mnt_path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let root_path = Path::new(root_path);
    let mnt_path = Path::new(mnt_path);

    create_ro_layer(&root_path)?;
    create_rw_layer(&root_path)?;
    create_mount_point(&root_path, &mnt_path)?;
    Ok(())
}

// Create a read-only layer, which is the busybox image.
fn create_ro_layer(root_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let busybox_dir = root_path.join("busybox");
    let busybox_tar = PathBuf::from("/home/ubuntu/Workspaces/Rtain/busybox.tar");

    if !busybox_dir.exists() {
        fs::create_dir_all(&busybox_dir)?;

        let status = Command::new("tar")
            .arg("-xvf")
            .arg(&busybox_tar)
            .arg("-C")
            .arg(&busybox_dir)
            .stdout(Stdio::null())
            .status()?;

        if status.success() {
            debug!("Unpacked busybox image to {:?}", busybox_dir);
        } else {
            return Err("Failed to unpack busybox image".into());
        }
    }

    debug!("Read-only layer at {:?}", busybox_dir);

    Ok(())
}

// Create a read-write layer, which is the container's write layer.
fn create_rw_layer(root_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let write_dir = root_path.join("writeLayer");
    if !write_dir.exists() {
        debug!("Create write layer dir: {:?}", write_dir);
        fs::create_dir_all(&write_dir)?;
    }

    debug!("Read-write layer at {:?}", write_dir);

    Ok(())
}

fn create_mount_point(root_path: &Path, mnt_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let upperdir = root_path.join("writeLayer");
    let lowerdir = root_path.join("busybox");
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

    debug!("Mounting overlay filesystem to {:?}", mnt_path);

    Command::new("mount")
        .arg("-t")
        .arg("overlay")
        .arg("overlay")
        .arg("-o")
        .arg(mount_option)
        .arg(mnt_path)
        .status()?;

    Ok(())
}

pub fn delete_workspace(root_url: &str, mnt_url: &str) -> Result<(), Box<dyn std::error::Error>> {
    delete_mount_point(Path::new(mnt_url))?;
    delete_write_layer(Path::new(root_url))?;
    Ok(())
}

fn delete_mount_point(mnt_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    debug!("Unmounted {:?}", mnt_path);
    Command::new("umount").arg(mnt_path).status()?;

    debug!("Deleted mount point at {:?}", mnt_path);
    fs::remove_dir_all(mnt_path)?;

    Ok(())
}

fn delete_write_layer(root_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let write_dir = root_path.join("writeLayer");
    let work_dir = root_path.join("work");

    debug!("Deleted write layer at {:?}", write_dir);
    fs::remove_dir_all(&write_dir)?;

    debug!("Deleted work dir at {:?}", work_dir);
    fs::remove_dir_all(&work_dir)?;

    Ok(())
}
