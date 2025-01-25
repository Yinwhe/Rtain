mod meta;
mod snapshot;
mod storage;
mod wal;

use tokio::sync::OnceCell;
pub use meta::{ContainerManager, ContainerMeta, ContainerStatus};

pub fn current_time() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

pub static CONTAINER_METAS: OnceCell<ContainerManager> = OnceCell::const_new();
