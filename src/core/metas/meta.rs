use std::sync::Arc;

use dashmap::DashMap;
use serde::{Deserialize, Serialize};

use super::storage::{StorageConfig, StorageManager, StorageOperation};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ContainerStatus {
    Running { pid: i32, started_at: u64 },
    Stopped { exit_code: i32, exited_at: u64 },
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

pub struct ContainerManager {
    storage: Arc<StorageManager>,
}

impl ContainerManager {
    pub async fn new(config: StorageConfig) -> anyhow::Result<Self> {
        let storage = Arc::new(StorageManager::new(config).await?);

        Ok(Self { storage })
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
