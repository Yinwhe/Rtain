use std::path::PathBuf;

use dashmap::DashMap;
use serde::{Deserialize, Serialize};

use crate::core::ROOT_PATH;

use super::{
    current_time,
    storage::{StorageConfig, StorageManager, StorageOperation},
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ContainerStatus {
    Running { pid: i32, started_at: u64 },
    Stopped { stopped_at: u64 },
    Paused,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerMeta {
    pub id: String,
    pub name: String,
    pub command: Vec<String>,
    pub status: ContainerStatus,
    pub created_at: u64,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct InnerState {
    pub by_id: DashMap<String, ContainerMeta>,
    pub by_name: DashMap<String, String>,
}

/// Uplevel container manager.
pub struct ContainerManager {
    storage: StorageManager,
}

impl ContainerManager {
    pub async fn new(config: StorageConfig) -> anyhow::Result<Self> {
        let storage = StorageManager::new(config).await?;

        Ok(Self { storage })
    }

    pub async fn default() -> anyhow::Result<Self> {
        let config = StorageConfig {
            wal_dir: PathBuf::from(format!("{ROOT_PATH}/wal")),
            snapshots_dir: PathBuf::from(format!("{ROOT_PATH}/snapshots")),

            max_wals: 10,
            max_snapshots: 10,

            snapshot_intervals_secs: 60,
            cleanup_interval_secs: 3 * 60,
        };

        let storage = StorageManager::new(config).await?;

        Ok(Self { storage })
    }

    pub async fn register(&self, meta: ContainerMeta) -> anyhow::Result<()> {
        self.storage.execute(StorageOperation::Create(meta)).await
    }

    pub async fn updates(&self, id: String, status: ContainerStatus) -> anyhow::Result<()> {
        self.storage
            .execute(StorageOperation::UpdateStatus { id, status })
            .await
    }

    pub async fn deregister(&self, id: String) -> anyhow::Result<()> {
        self.storage.execute(StorageOperation::Delete(id)).await
    }

    pub async fn get_meta_by_id(&self, id: &str) -> Option<ContainerMeta> {
        self.storage.get_meta_by_id(id).await
    }

    pub async fn get_meta_by_name(&self, name: &str) -> Option<ContainerMeta> {
        self.storage.get_meta_by_name(name).await
    }

    pub async fn get_all_metas(&self) -> Vec<ContainerMeta> {
        self.storage.get_all_metas().await
    }
}

impl ContainerMeta {
    pub fn new(id: String, name: String, pid: i32, command: Vec<String>) -> Self {
        let time = current_time();
        Self {
            id,
            name,
            command,
            status: ContainerStatus::Running {
                pid,
                started_at: time,
            },
            created_at: time,
        }
    }

    pub fn get_pid(&self) -> Option<i32> {
        match self.status {
            ContainerStatus::Running { pid, .. } => Some(pid),
            _ => None,
        }
    }
}

impl ContainerStatus {
    pub fn is_running(&self) -> bool {
        matches!(self, Self::Running { .. })
    }

    pub fn is_stopped(&self) -> bool {
        matches!(self, Self::Stopped { .. })
    }

    pub fn running(pid: i32) -> Self {
        Self::Running {
            pid,
            started_at: current_time(),
        }
    }

    pub fn stop() ->  Self {
        Self::Stopped {
            stopped_at: current_time(),
        }
    }
}

impl InnerState {
    pub fn apply_operation(&self, op: StorageOperation) -> anyhow::Result<()> {
        match op {
            StorageOperation::Create(meta) => {
                self.by_name.insert(meta.name.clone(), meta.id.clone());
                self.by_id.insert(meta.id.clone(), meta);
            }
            StorageOperation::UpdateStatus { id, status } => {
                if let Some(mut entry) = self.by_id.get_mut(&id) {
                    entry.status = status;
                }
            }
            StorageOperation::Delete(id) => {
                if let Some((_, meta)) = self.by_id.remove(&id) {
                    self.by_name.remove(&meta.name);
                }
            }
        }
        Ok(())
    }
}
