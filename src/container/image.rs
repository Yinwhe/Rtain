use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};

use log::{debug, info};
use nix::mount::{mount, umount2, MntFlags, MsFlags};

pub fn new_workspace(
    image_path: &str,
    root_path: &str,
    mnt_path: &str,
    volume: &Option<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    let image_path = Path::new(image_path);
    let root_path = Path::new(root_path);
    let mnt_path = Path::new(mnt_path);

    create_ro_layer(&image_path, &root_path)?;
    create_rw_layer(&root_path)?;
    create_mount_point(&root_path, &mnt_path)?;

    if let Some(vol) = volume {
        let sv = vol.split(":").collect::<Vec<&str>>();
        if sv.len() == 2 && !sv[0].is_empty() && !sv[1].is_empty() {
            mount_volume(&mnt_path, sv)?;
        } else {
            return Err(format!("Invalid volume: {}", vol).into());
        }
    }

    Ok(())
}

// Create a read-only layer, on the given image.
fn create_ro_layer(image_path: &Path, root_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let image_dir = root_path.join("image");

    if !image_dir.exists() {
        fs::create_dir_all(&image_dir)?;

        let status = Command::new("tar")
            .arg("-xvf")
            .arg(&image_path)
            .arg("-C")
            .arg(&image_dir)
            .stdout(Stdio::null())
            .status()?;

        if status.success() {
            debug!("Unpacked image to {:?}", image_dir);
        } else {
            return Err("Failed to unpack image".into());
        }
    }

    debug!("Read-only layer at {:?}", image_dir);

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

    debug!("Mounting overlay filesystem to {:?}", mnt_path);

    let status = Command::new("mount")
        .arg("-t")
        .arg("overlay")
        .arg("overlay")
        .arg("-o")
        .arg(mount_option)
        .arg(mnt_path)
        .status()?;

    if !status.success() {
        return Err("Failed to mount overlay filesystem".into());
    }

    Ok(())
}

pub fn delete_workspace(
    root_path: &str,
    mnt_path: &str,
    volume: &Option<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    let root_path = Path::new(root_path);
    let mnt_path = Path::new(mnt_path);

    if let Some(vol) = volume {
        let sv = vol.split(":").collect::<Vec<&str>>();

        assert!(sv.len() == 2 && !sv[0].is_empty() && !sv[1].is_empty());
        umount_volume(mnt_path, sv)?;
    }

    delete_mount_point(mnt_path)?;
    delete_write_layer(root_path)?;

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

fn mount_volume(mnt_path: &Path, volume_path: Vec<&str>) -> Result<(), Box<dyn std::error::Error>> {
    info!("Mounting volume: {:?}", volume_path);

    let hostv = Path::new(volume_path[0]);
    let contv = mnt_path.join(volume_path[1].strip_prefix("/").unwrap());

    debug!("Host volume: {:?}", hostv);
    debug!("Container volume: {:?}", contv);

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

fn umount_volume(
    mnt_path: &Path,
    volume_path: Vec<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    info!("Unmounting volume: {:?}", volume_path);

    let contv = mnt_path.join(volume_path[1].strip_prefix("/").unwrap());
    debug!("Container volume: {:?}", contv);

    umount2(&contv, MntFlags::MNT_DETACH)?;

    Ok(())
}
