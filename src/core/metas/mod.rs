mod meta;
mod snapshot;
mod storage;
mod wal;

pub mod example;

use tokio::sync::OnceCell;
pub use meta::{
    ContainerManager, ContainerMeta, ContainerStatus, ContainerState,
    HealthStatus, NetworkConfig, ResourceConfig, MountPoint, MountType,
    ContainerFilter, ResourceSummary, MetadataEvent, MetadataEventHandler
};
pub use storage::{StorageConfig, StorageManager, StorageOperation};
pub use wal::{WalManager, IntegrityReport, WalError};

pub fn current_time() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

// Convenience functions for creating common filters
impl ContainerFilter {
    pub fn by_status(status: ContainerStatus) -> Self {
        Self {
            status: Some(status),
            ..Default::default()
        }
    }
    
    pub fn by_label(key: &str, value: &str) -> Self {
        Self {
            labels: [(key.to_string(), value.to_string())].into(),
            ..Default::default()
        }
    }
    
    pub fn recent(hours: u64) -> Self {
        Self {
            since: Some(current_time() - hours * 3600),
            ..Default::default()
        }
    }
}

pub static CONTAINER_METAS: OnceCell<ContainerManager> = OnceCell::const_new();
