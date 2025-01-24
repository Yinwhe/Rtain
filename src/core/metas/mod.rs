mod meta;
mod snapshot;
mod storage;
mod wal;

pub fn current_time() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

pub use meta::{ContainerManager, ContainerMeta, ContainerStatus};
use once_cell::sync::Lazy;

pub static CONTAINER_METAS: Lazy<ContainerManager> = Lazy::new(|| {
    tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(ContainerManager::default())
        .expect("Failed to create container manager")
});
