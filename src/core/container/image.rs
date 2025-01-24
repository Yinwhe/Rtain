use std::path::Path;
use std::process::{Command, Stdio};

use log::debug;
use nix::mount::{mount, umount2, MntFlags, MsFlags};

pub async fn new_workspace(
    image_path: &str,
    root_path: &str,
    mnt_path: &str,
    volume: &Option<String>,
) -> anyhow::Result<()> {
    let image_path = Path::new(image_path);
    let root_path = Path::new(root_path);
    let mnt_path = Path::new(mnt_path);

    create_ro_layer(&image_path, &root_path).await?;
    if let Err(e) = create_rw_layer(&root_path).await {
        // Clean up the ro layer.
        let _ = tokio::fs::remove_dir_all(root_path).await;
        return Err(e);
    }
    if let Err(e) = create_mount_point(&root_path, &mnt_path).await {
        // Clean up the ro and rw layers.
        let _ = tokio::fs::remove_dir_all(root_path).await;
        return Err(e);
    }

    if let Some(vol) = volume {
        let sv = vol.split(":").collect::<Vec<&str>>();
        if sv.len() == 2 && !sv[0].is_empty() && !sv[1].is_empty() {
            if let Err(e) = mount_volume(&mnt_path, sv).await {
                // Clean up the ro and rw layers.
                let _ = Command::new("umount").arg(mnt_path).status();
                let _ = tokio::fs::remove_dir_all(root_path).await;

                return Err(e);
            }
        } else {
            let _ = Command::new("umount").arg(mnt_path).status();
            let _ = tokio::fs::remove_dir_all(root_path).await;

            return Err(anyhow::anyhow!("Invalid volume: {}", vol));
        }
    }

    debug!("[Daemon] Workspace created under {:?}", root_path);

    Ok(())
}

// Create a read-only layer, on the given image.
async fn create_ro_layer(image_path: &Path, root_path: &Path) -> anyhow::Result<()> {
    let image_dir = root_path.join("image");

    if !image_dir.exists() {
        tokio::fs::create_dir_all(&image_dir).await?;

        let output = Command::new("tar")
            .arg("-xvf")
            .arg(&image_path)
            .arg("-C")
            .arg(&image_dir)
            .stdout(Stdio::null())
            .output()?;

        if !output.status.success() {
            return Err(anyhow::anyhow!(
                "Failed to extract image: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }
    }

    Ok(())
}

// Create a read-write layer, which is the container's write layer.
async fn create_rw_layer(root_path: &Path) -> anyhow::Result<()> {
    let write_dir = root_path.join("writeLayer");
    if !write_dir.exists() {
        tokio::fs::create_dir_all(&write_dir).await?;
    }

    Ok(())
}

async fn create_mount_point(root_path: &Path, mnt_path: &Path) -> anyhow::Result<()> {
    let upperdir = root_path.join("writeLayer");
    let lowerdir = root_path.join("image");
    let workdir = root_path.join("work");

    if !workdir.exists() {
        tokio::fs::create_dir_all(&workdir).await?;
    }

    if !mnt_path.exists() {
        tokio::fs::create_dir_all(mnt_path).await?;
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
        return Err(anyhow::anyhow!(
            "Failed to mount overlay filesystem: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    Ok(())
}

pub async fn delete_workspace(
    root_path: &str,
    mnt_path: &str,
    volume: &Option<String>,
) -> anyhow::Result<()> {
    let root_path = Path::new(root_path);
    let mnt_path = Path::new(mnt_path);

    if let Some(vol) = volume {
        let sv = vol.split(":").collect::<Vec<&str>>();

        assert!(sv.len() == 2 && !sv[0].is_empty() && !sv[1].is_empty());
        umount_volume(mnt_path, sv).await?;
    }

    // Unmount the overlay filesystem.
    Command::new("umount").arg(mnt_path).status()?;
    // And simply delete the whole directory.
    tokio::fs::remove_dir_all(root_path).await?;

    debug!("[Daemon] Workspace deleted under {:?}", root_path);

    Ok(())
}

async fn mount_volume(mnt_path: &Path, volume_path: Vec<&str>) -> anyhow::Result<()> {
    debug!("[Daemon] Mounting volume: {:?}", volume_path);

    let hostv = Path::new(volume_path[0]);
    let contv = mnt_path.join(volume_path[1].strip_prefix("/").unwrap());

    if !hostv.exists() {
        tokio::fs::create_dir_all(hostv).await?;
    }

    if !contv.exists() {
        tokio::fs::create_dir_all(&contv).await?;
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

async fn umount_volume(mnt_path: &Path, volume_path: Vec<&str>) -> anyhow::Result<()> {
    debug!("[Daemon] Unmounting volume: {:?}", volume_path);

    let contv = mnt_path.join(volume_path[1].strip_prefix("/").unwrap());

    umount2(&contv, MntFlags::MNT_DETACH)?;

    Ok(())
}
