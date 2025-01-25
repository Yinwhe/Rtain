use std::{
    io::{Read, Write},
    os::unix::net::UnixStream as StdUnixStream,
};

use cgroups_rs::{Cgroup, CgroupPid};
use log::error;
use nix::{
    pty::{openpty, OpenptyResult},
    unistd::Pid,
};
use tokio::net::UnixStream;

use super::init::{do_run, new_container_process};
use crate::core::{
    cmd::StartArgs,
    metas::{ContainerMeta, ContainerStatus, CONTAINER_METAS},
};
use crate::core::{Msg, ROOT_PATH};

pub async fn start_container(start_args: StartArgs, mut stream: UnixStream) {
    let meta = match CONTAINER_METAS
        .get()
        .unwrap()
        .get_meta_by_name(&start_args.name)
        .await
    {
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

    let (pty, sock, child) = match start_prepare(&meta).await {
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
        sock,
        stream,
        start_args.detach,
    )
    .await;
}

async fn start_prepare(
    meta: &ContainerMeta,
) -> anyhow::Result<(OpenptyResult, StdUnixStream, Pid)> {
    let name_id = format!("{}-{}", &meta.name, &meta.id);
    let mnt_path = format!("{}/{}/mnt", ROOT_PATH, name_id);

    let pty = openpty(None, None)?;

    // Sync between daemon and new child process (container).
    let (mut p_sock, c_sock) = StdUnixStream::pair()?;
    let mut buf = [0u8; 4];

    // Create a new process with old namespaces.
    let child = match new_container_process(&mnt_path, c_sock, &pty, &meta.command) {
        Ok(child) => child,
        Err(e) => {
            return Err(e);
        }
    };

    // Wait for child ready.
    p_sock.read_exact(&mut buf).unwrap();
    match &buf {
        b"EXIT" => {
            return Err(anyhow::anyhow!(
                "Failed to initialize container: child unexpected exit"
            ));
        }
        b"WAIT" => {}
        _ => unreachable!(),
    }

    // Get the old cgroups
    let hier = cgroups_rs::hierarchies::auto();
    let cg = Cgroup::load(hier, name_id);

    if let Err(e) = cg.add_task_by_tgid(CgroupPid::from(child.as_raw() as u64)) {
        p_sock.write(b"EXIT").unwrap();

        return Err(anyhow::anyhow!("Failed to add task to cgroup: {:?}", e));
    }

    // Updates records.
    if let Err(e) = CONTAINER_METAS
        .get()
        .unwrap()
        .updates(meta.id.clone(), ContainerStatus::running(child.as_raw()))
        .await
    {
        error!("Failed to update container status: {:?}", e);
        p_sock.write(b"EXIT").unwrap();
        let _ = cg.kill();

        return Err(anyhow::anyhow!("Failed to update container: {:?}", e));
    }

    Ok((pty, p_sock, child))
}
