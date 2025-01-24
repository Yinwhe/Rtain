use std::os::fd::AsRawFd;

use cgroups_rs::{Cgroup, CgroupPid};
use log::error;
use nix::{
    pty::{openpty, OpenptyResult},
    unistd::{read, write, Pid},
};
use tokio::net::{unix::pipe, UnixStream};

use super::init::{do_run, new_container_process};
use crate::core::{
    cmd::StartArgs,
    metas::{ContainerMeta, ContainerStatus, CONTAINER_METAS},
};
use crate::core::{Msg, ROOT_PATH};

pub async fn start_container(start_args: StartArgs, mut stream: UnixStream) {
    let meta = match CONTAINER_METAS.get_meta_by_name(&start_args.name).await {
        Some(meta) => meta,
        None => {
            error!(
                "Failed to start container {}, record does not exist",
                &start_args.name
            );
            let _ = Msg::Err(format!(
                "Failed to start container {}, record does not exist",
                &start_args.name
            ))
            .send_to(&mut stream)
            .await;

            return;
        }
    };

    if meta.status.is_running() {
        error!(
            "Failed to start container {}, it's already running",
            &start_args.name
        );
        let _ = Msg::Err(format!(
            "Failed to start container {}, it's already running",
            &start_args.name
        ))
        .send_to(&mut stream)
        .await;

        return;
    }

    let (pty, pipe, child) = match start_prepare(&meta).await {
        Ok(res) => res,
        Err(e) => {
            error!("Failed to start container: {:?}", e);
            // FIXME: Error return.

            return;
        }
    };

    do_run(
        meta.name,
        meta.id,
        child,
        pty,
        pipe,
        stream,
        start_args.detach,
    )
    .await;
}

async fn start_prepare(
    meta: &ContainerMeta,
) -> anyhow::Result<(OpenptyResult, (pipe::Sender, pipe::Receiver), Pid)> {
    let name_id = format!("{}-{}", &meta.name, &meta.id);
    let mnt_path = format!("{}/{}/mnt", ROOT_PATH, name_id);

    let pty = openpty(None, None)?;

    // Sync between daemon and new child process (container).
    let pipe_p = pipe::pipe()?;
    let pipe_c = pipe::pipe()?;
    let pipe = (pipe_p.0, pipe_c.1);
    let mut buf = [0u8; 4];

    // Create a new process with old namespaces.
    let child = match new_container_process(&mnt_path, (&pipe_c.0, &pipe_p.1), &pty, &meta.command)
    {
        Ok(child) => child,
        Err(e) => {
            return Err(e);
        }
    };

    // Wait for child ready.
    read(pipe.0.as_raw_fd(), &mut buf).unwrap();
    if buf == *b"EXIT" {
        return Err(anyhow::anyhow!(
            "Failed to start container: child unexpected exit"
        ));
    }

    // Get the old cgroups
    let hier = cgroups_rs::hierarchies::auto();
    let cg = Cgroup::load(hier, name_id);

    if let Err(e) = cg.add_task_by_tgid(CgroupPid::from(child.as_raw() as u64)) {
        write(&pipe.0, b"EXIT").unwrap();

        return Err(anyhow::anyhow!("Failed to add task to cgroup: {:?}", e));
    }

    // Updates records.
    if let Err(e) = CONTAINER_METAS
        .updates(meta.id.clone(), ContainerStatus::running(child.as_raw()))
        .await
    {
        error!("Failed to update container status: {:?}", e);
        write(&pipe.0, b"EXIT").unwrap();
        let _ = cg.kill();

        return Err(anyhow::anyhow!("Failed to update container: {:?}", e));
    }

    Ok((pty, pipe, child))
}
