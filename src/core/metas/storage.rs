use std::{path::PathBuf, sync::Arc, time::Duration};

use serde::{Deserialize, Serialize};
use tokio::{
    sync::{mpsc, oneshot, Mutex},
    task::JoinHandle,
};

use super::{
    meta::{
        ContainerMeta, ContainerState, ContainerStatus, InnerState, MetadataEvent,
        MetadataEventHandler, MountPoint, MountType, NetworkConfig, ResourceConfig,
    },
    snapshot::Snapshotter,
    wal::WalManager,
};

#[derive(Clone, Debug)]
pub struct StorageConfig {
    pub wal_dir: PathBuf,
    pub snapshots_dir: PathBuf,

    pub max_wals: usize,
    pub max_snapshots: usize,

    pub snapshot_intervals_secs: u64,
    pub cleanup_interval_secs: u64,
}

pub struct StorageManager {
    op_sender: Arc<Mutex<mpsc::Sender<(StorageOperation, oneshot::Sender<anyhow::Result<()>>)>>>,
    inner: Arc<Mutex<StorageInner>>,
    #[allow(unused)]
    worker: JoinHandle<()>,
}

impl std::fmt::Debug for StorageManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StorageManager")
            .field("op_sender", &"Arc<Mutex<Sender>>")
            .field("inner", &self.inner)
            .field("worker", &"JoinHandle<()>")
            .finish()
    }
}

#[derive(Debug)]
struct StorageInner {
    config: StorageConfig,
    wal: WalManager,
    snapshotter: Snapshotter,
    state: InnerState,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StorageOperation {
    // Basic operations
    Create(ContainerMeta),
    Delete(String),

    // Fine-grained status updates
    UpdateStatus {
        id: String,
        status: ContainerStatus,
    },
    UpdateState {
        id: String,
        state: ContainerState,
    },

    // Configuration updates
    UpdateEnvironment {
        id: String,
        env: std::collections::HashMap<String, String>,
    },
    UpdateLabels {
        id: String,
        labels: std::collections::HashMap<String, String>,
    },
    UpdateResources {
        id: String,
        resources: ResourceConfig,
    },

    // Network operations
    AttachNetwork {
        id: String,
        network: NetworkConfig,
    },
    DetachNetwork {
        id: String,
    },

    // Mount operations
    AddMount {
        id: String,
        mount: MountPoint,
    },
    RemoveMount {
        id: String,
        destination: String,
    },

    // Batch operations
    Batch(Vec<StorageOperation>),
}

impl StorageManager {
    pub async fn new(config: StorageConfig) -> anyhow::Result<Self> {
        let wal = WalManager::new(&config.wal_dir, config.max_wals).await?;
        let snapshotter = Snapshotter::new(&config.snapshots_dir, config.max_snapshots).await?;

        let state = Self::recover_state(&snapshotter, &wal).await?;
        let inner = Arc::new(Mutex::new(StorageInner {
            config,
            wal,
            snapshotter,
            state,
        }));

        let (op_sender, op_recver) = mpsc::channel(128);
        let worker = Self::start_background_worker(inner.clone(), op_recver);

        Ok(Self {
            inner: inner,
            op_sender: Arc::new(Mutex::new(op_sender)),
            worker,
        })
    }

    pub async fn execute(&self, op: StorageOperation) -> anyhow::Result<()> {
        let (ack_tx, ack_rx) = oneshot::channel();

        self.op_sender.lock().await.send((op, ack_tx)).await?;

        ack_rx.await?
    }

    async fn recover_state(
        snapshotter: &Snapshotter,
        wal: &WalManager,
    ) -> anyhow::Result<InnerState> {
        let state = snapshotter.load_latest().await?;

        // Replay the wals.
        let wal_entries = wal.read_operations().await?;
        for op in wal_entries {
            state.apply_operation(op)?;
        }

        Ok(state)
    }

    fn start_background_worker(
        inner: Arc<Mutex<StorageInner>>,
        mut op_recver: mpsc::Receiver<(StorageOperation, oneshot::Sender<anyhow::Result<()>>)>,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            let snapshot_interval = tokio::time::interval(Duration::from_secs(
                inner.lock().await.config.snapshot_intervals_secs,
            ));
            let cleanup_interval = tokio::time::interval(Duration::from_secs(
                inner.lock().await.config.cleanup_interval_secs,
            ));

            let snapshot_inner = inner.clone();
            let snapshot_task = tokio::spawn(async move {
                let mut interval = snapshot_interval;
                interval.tick().await;
                loop {
                    interval.tick().await;
                    let locked_inner = snapshot_inner.lock().await;

                    if let Err(e) = locked_inner
                        .snapshotter
                        .take_snapshot(&locked_inner.state)
                        .await
                    {
                        log::error!("Failed to take snapshot: {}", e);
                    }
                }
            });

            let cleanup_inner = inner.clone();
            let cleanup_task = tokio::spawn(async move {
                let mut interval = cleanup_interval;
                interval.tick().await;
                loop {
                    interval.tick().await;
                    let mut locked_inner = cleanup_inner.lock().await;

                    let cleanup_result: anyhow::Result<()> = async {
                        locked_inner.snapshotter.purge_old_snapshots().await?;
                        locked_inner.wal.rotate().await?;
                        locked_inner.wal.purge_old_archives().await?;
                        Ok(())
                    }
                    .await;

                    if let Err(e) = cleanup_result {
                        log::error!("Failed to do cleanup: {e}");
                    }
                }
            });

            loop {
                let op = op_recver.recv().await;
                match op {
                    Some((op, ack_tx)) => {
                        let locked_inner = inner.lock().await;

                        // WAL first.
                        if let Err(e) = locked_inner.wal.write_operation(&op).await {
                            log::error!("Failed to write WAL: {e}");
                            ack_tx.send(Err(e)).unwrap();

                            continue;
                        }

                        // Updates data in memory.
                        if let Err(e) = locked_inner.state.apply_operation(op) {
                            log::error!("Failed to snapshot: {e}");
                            ack_tx.send(Err(e)).unwrap();

                            continue;
                        }

                        ack_tx.send(Ok(())).unwrap();
                    }
                    None => {
                        break;
                    }
                }
            }

            let _ = tokio::try_join!(snapshot_task, cleanup_task);
        })
    }
}

impl StorageManager {
    #[allow(unused)]
    pub async fn get_meta_by_id(&self, id: &str) -> Option<ContainerMeta> {
        self.inner
            .lock()
            .await
            .state
            .by_id
            .get(id)
            .map(|meta| meta.clone())
    }

    pub async fn get_meta_by_name(&self, name: &str) -> Option<ContainerMeta> {
        let locked_inner = self.inner.lock().await;
        locked_inner.state.by_name.get(name).map(|id| {
            locked_inner
                .state
                .by_id
                .get(id.as_str())
                .map(|meta| meta.clone())
                .unwrap()
        })
    }

    pub async fn get_all_metas(&self) -> Vec<ContainerMeta> {
        self.inner
            .lock()
            .await
            .state
            .by_id
            .iter()
            .map(|meta| meta.clone())
            .collect()
    }

    // Event system support
    // Event handler methods temporarily removed for compilation
    // TODO: Implement event system properly

    async fn operation_to_event(&self, op: &StorageOperation) -> Option<MetadataEvent> {
        match op {
            StorageOperation::Create(meta) => Some(MetadataEvent::ContainerCreated {
                id: meta.id.clone(),
                name: meta.name.clone(),
            }),
            StorageOperation::Delete(id) => {
                if let Some(meta) = self.get_meta_by_id(id).await {
                    Some(MetadataEvent::ContainerDeleted {
                        id: id.clone(),
                        name: meta.name,
                    })
                } else {
                    None
                }
            }
            StorageOperation::UpdateStatus { id, status } => {
                if let Some(meta) = self.get_meta_by_id(id).await {
                    Some(MetadataEvent::StatusChanged {
                        id: id.clone(),
                        name: meta.name,
                        old_status: meta.state.status,
                        new_status: status.clone(),
                    })
                } else {
                    None
                }
            }
            StorageOperation::UpdateResources { id, resources } => {
                Some(MetadataEvent::ResourcesUpdated {
                    id: id.clone(),
                    resources: resources.clone(),
                })
            }
            StorageOperation::AttachNetwork { id, network } => {
                Some(MetadataEvent::NetworkAttached {
                    id: id.clone(),
                    network: network.clone(),
                })
            }
            // Event conversion for other operations
            _ => None,
        }
    }

    // Enhanced WAL functionality
    pub async fn compact_wal(&self, snapshot_index: u64) -> anyhow::Result<()> {
        let inner = self.inner.lock().await;
        inner.wal.compact(snapshot_index).await
    }

    pub async fn verify_wal_integrity(&self) -> anyhow::Result<super::wal::IntegrityReport> {
        let inner = self.inner.lock().await;
        inner.wal.verify_integrity().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_storage_manager_new() {
        use tempfile::TempDir;

        // Create temporary directories for test isolation
        let temp_wal = TempDir::new().unwrap();
        let temp_snapshots = TempDir::new().unwrap();

        let config = StorageConfig {
            wal_dir: temp_wal.path().to_path_buf(),
            snapshots_dir: temp_snapshots.path().to_path_buf(),
            max_wals: 5,
            max_snapshots: 3,
            snapshot_intervals_secs: 60,
            cleanup_interval_secs: 60,
        };

        let storage_manager = StorageManager::new(config).await;

        assert!(storage_manager.is_ok(), "Failed to create StorageManager");

        let storage_manager = storage_manager.unwrap();
        let inner = storage_manager.inner.lock().await;

        assert_eq!(inner.config.max_wals, 5);
        assert_eq!(inner.config.max_snapshots, 3);
    }

    #[tokio::test]
    async fn test_execute_create_operation() {
        use tempfile::TempDir;

        // Create temporary directories for test isolation
        let temp_wal = TempDir::new().unwrap();
        let temp_snapshots = TempDir::new().unwrap();

        let config = StorageConfig {
            wal_dir: temp_wal.path().to_path_buf(),
            snapshots_dir: temp_snapshots.path().to_path_buf(),
            max_wals: 5,
            max_snapshots: 3,
            snapshot_intervals_secs: 60,
            cleanup_interval_secs: 60,
        };

        let storage_manager = StorageManager::new(config).await.unwrap();

        let meta = ContainerMeta::new(
            "container1".to_string(),
            "test_container".to_string(),
            "ubuntu:latest".to_string(),
            vec!["/bin/bash".to_string()],
            vec![],
        );

        let op = StorageOperation::Create(meta);
        let result = storage_manager.execute(op).await;

        assert!(result.is_ok(), "Failed to execute create operation");
    }

    #[tokio::test]
    async fn test_execute_update_status() {
        use tempfile::TempDir;

        // Create temporary directories for test isolation
        let temp_wal = TempDir::new().unwrap();
        let temp_snapshots = TempDir::new().unwrap();

        let config = StorageConfig {
            wal_dir: temp_wal.path().to_path_buf(),
            snapshots_dir: temp_snapshots.path().to_path_buf(),
            max_wals: 5,
            max_snapshots: 3,
            snapshot_intervals_secs: 60,
            cleanup_interval_secs: 60,
        };

        let storage_manager = StorageManager::new(config).await.unwrap();

        let op = StorageOperation::UpdateStatus {
            id: "container1".to_string(),
            status: ContainerStatus::Exited,
        };

        let result = storage_manager.execute(op).await;

        assert!(result.is_ok(), "Failed to execute update status operation");
    }

    #[tokio::test]
    async fn test_execute_delete_operation() {
        use tempfile::TempDir;

        // Create temporary directories for test isolation
        let temp_wal = TempDir::new().unwrap();
        let temp_snapshots = TempDir::new().unwrap();

        let config = StorageConfig {
            wal_dir: temp_wal.path().to_path_buf(),
            snapshots_dir: temp_snapshots.path().to_path_buf(),
            max_wals: 5,
            max_snapshots: 3,
            snapshot_intervals_secs: 60,
            cleanup_interval_secs: 60,
        };

        let storage_manager = StorageManager::new(config).await.unwrap();

        let op = StorageOperation::Delete("container1".to_string());

        let result = storage_manager.execute(op).await;

        assert!(result.is_ok(), "Failed to execute delete operation");
    }

    #[tokio::test]
    async fn test_inner_state_apply_create_operation() {
        let state = InnerState::default();

        let meta = ContainerMeta::new(
            "container1".to_string(),
            "test_container".to_string(),
            "ubuntu:latest".to_string(),
            vec!["/bin/bash".to_string()],
            vec![],
        );

        let op = StorageOperation::Create(meta.clone());

        let result = state.apply_operation(op);

        assert!(result.is_ok(), "Failed to apply create operation");

        assert!(
            state.by_id.contains_key(&meta.id),
            "Container not found in by_id map"
        );
        assert!(
            state.by_name.contains_key(&meta.name),
            "Container not found in by_name map"
        );
    }

    #[tokio::test]
    async fn test_inner_state_apply_update_status() {
        let state = InnerState::default();

        let meta = ContainerMeta::new(
            "container1".to_string(),
            "test_container".to_string(),
            "ubuntu:latest".to_string(),
            vec!["/bin/bash".to_string()],
            vec![],
        );

        let op = StorageOperation::Create(meta.clone());
        state.apply_operation(op).unwrap();

        let new_status = ContainerStatus::Exited;
        let update_op = StorageOperation::UpdateStatus {
            id: meta.id.clone(),
            status: new_status.clone(),
        };

        let result = state.apply_operation(update_op);

        assert!(result.is_ok(), "Failed to apply update status operation");

        if let Some(updated_meta) = state.by_id.get(&meta.id) {
            assert_eq!(
                updated_meta.state.status, new_status,
                "Status update failed"
            );
        } else {
            panic!("Container not found after status update");
        };
    }

    #[tokio::test]
    async fn test_inner_state_apply_delete_operation() {
        let state = InnerState::default();

        let meta = ContainerMeta::new(
            "container1".to_string(),
            "test_container".to_string(),
            "ubuntu:latest".to_string(),
            vec!["/bin/bash".to_string()],
            vec![],
        );

        let op = StorageOperation::Create(meta.clone());
        state.apply_operation(op).unwrap();

        let delete_op = StorageOperation::Delete(meta.id.clone());

        let result = state.apply_operation(delete_op);

        assert!(result.is_ok(), "Failed to apply delete operation");

        assert!(
            !state.by_id.contains_key(&meta.id),
            "Container was not removed from by_id map"
        );
        assert!(
            !state.by_name.contains_key(&meta.name),
            "Container was not removed from by_name map"
        );
    }

    #[tokio::test]
    async fn test_new_storage_operations_step_by_step() {
        println!("Step 1: Creating state");
        let state = InnerState::default();

        let mut meta = ContainerMeta::new(
            "container1".to_string(),
            "test_container".to_string(),
            "ubuntu:latest".to_string(),
            vec!["/bin/bash".to_string()],
            vec![],
        );
        meta.env.insert("TEST_VAR".to_string(), "value".to_string());
        meta.labels.insert("app".to_string(), "test".to_string());

        // Create container
        state
            .apply_operation(StorageOperation::Create(meta.clone()))
            .unwrap();

        // Test environment variable updates
        let new_env = [("NEW_VAR".to_string(), "new_value".to_string())].into();
        let update_env_op = StorageOperation::UpdateEnvironment {
            id: meta.id.clone(),
            env: new_env,
        };
        state.apply_operation(update_env_op).unwrap();

        // Verify environment update - use scoped block to release reference
        {
            let updated = state.by_id.get(&meta.id).unwrap();
            assert_eq!(updated.env.get("NEW_VAR"), Some(&"new_value".to_string()));
        } // Reference is dropped here

        // Test label updates
        let new_labels = [("version".to_string(), "1.0".to_string())].into();
        let update_labels_op = StorageOperation::UpdateLabels {
            id: meta.id.clone(),
            labels: new_labels,
        };
        state.apply_operation(update_labels_op).unwrap();

        // Verify labels update - use scoped block to release reference
        {
            let updated = state.by_id.get(&meta.id).unwrap();
            assert_eq!(updated.labels.get("version"), Some(&"1.0".to_string()));
        } // Reference is dropped here

        // Test resource configuration updates
        let resources = ResourceConfig {
            memory_limit: Some(512 * 1024 * 1024),
            cpu_limit: Some(1.5),
            pids_limit: Some(1000),
            disk_limit: None,
        };
        let update_resources_op = StorageOperation::UpdateResources {
            id: meta.id.clone(),
            resources: resources.clone(),
        };
        state.apply_operation(update_resources_op).unwrap();

        // Verify resources update - use scoped block to release reference
        {
            let updated = state.by_id.get(&meta.id).unwrap();
            assert_eq!(updated.resources.memory_limit, Some(512 * 1024 * 1024));
            assert_eq!(updated.resources.cpu_limit, Some(1.5));
        } // Reference is dropped here

        // Test network operations
        let network = NetworkConfig {
            ip_address: Some("172.17.0.2".to_string()),
            network_name: "bridge".to_string(),
            mac_address: Some("02:42:ac:11:00:02".to_string()),
            ports: [(80, 8080)].into(),
        };
        let attach_network_op = StorageOperation::AttachNetwork {
            id: meta.id.clone(),
            network: network.clone(),
        };
        state.apply_operation(attach_network_op).unwrap();

        // Verify network attach - use scoped block to release reference
        {
            let updated = state.by_id.get(&meta.id).unwrap();
            assert!(updated.network.is_some());
            assert_eq!(
                updated.network.as_ref().unwrap().ip_address,
                Some("172.17.0.2".to_string())
            );
        } // Reference is dropped here

        // Test network detachment
        let detach_network_op = StorageOperation::DetachNetwork {
            id: meta.id.clone(),
        };
        state.apply_operation(detach_network_op).unwrap();

        // Verify network detach - use scoped block to release reference
        {
            let updated = state.by_id.get(&meta.id).unwrap();
            assert!(updated.network.is_none());
        } // Reference is dropped here

        // Test adding mount point
        let mount = MountPoint {
            source: "/host/data".to_string(),
            destination: "/app/data".to_string(),
            mount_type: MountType::Bind,
            read_only: false,
        };
        let add_mount_op = StorageOperation::AddMount {
            id: meta.id.clone(),
            mount: mount.clone(),
        };
        state.apply_operation(add_mount_op).unwrap();

        // Verify mount add - use scoped block to release reference
        {
            let updated = state.by_id.get(&meta.id).unwrap();
            assert_eq!(updated.mounts.len(), 1);
            assert_eq!(updated.mounts[0].destination, "/app/data");
        } // Reference is dropped here

        // Test removing mount point
        let remove_mount_op = StorageOperation::RemoveMount {
            id: meta.id.clone(),
            destination: "/app/data".to_string(),
        };
        state.apply_operation(remove_mount_op).unwrap();

        // Verify mount removal - use scoped block to release reference
        {
            let updated = state.by_id.get(&meta.id).unwrap();
            assert_eq!(updated.mounts.len(), 0);
        } // Reference is dropped here
    }

    #[tokio::test]
    async fn test_labels_operation_isolated() {
        println!("Testing isolated labels operation");
        let state = InnerState::default();

        // Create container first
        let meta = ContainerMeta::new(
            "container1".to_string(),
            "test_container".to_string(),
            "ubuntu:latest".to_string(),
            vec!["/bin/bash".to_string()],
            vec![],
        );

        println!("Creating container...");
        state
            .apply_operation(StorageOperation::Create(meta.clone()))
            .unwrap();

        println!("Container created, testing labels update...");

        // Test ONLY the labels update that seems to be problematic
        let new_labels = [("version".to_string(), "1.0".to_string())].into();
        let update_labels_op = StorageOperation::UpdateLabels {
            id: meta.id.clone(),
            labels: new_labels,
        };

        println!("About to call apply_operation for UpdateLabels...");
        state.apply_operation(update_labels_op).unwrap();
        println!("UpdateLabels completed successfully!");

        let updated = state.by_id.get(&meta.id).unwrap();
        assert_eq!(updated.labels.get("version"), Some(&"1.0".to_string()));
        println!("Test completed successfully!");
    }

    #[tokio::test]
    async fn test_batch_operations() {
        let state = InnerState::default();

        let meta1 = ContainerMeta::new(
            "container1".to_string(),
            "test_container1".to_string(),
            "ubuntu:latest".to_string(),
            vec!["/bin/bash".to_string()],
            vec![],
        );

        let meta2 = ContainerMeta::new(
            "container2".to_string(),
            "test_container2".to_string(),
            "ubuntu:latest".to_string(),
            vec!["/bin/bash".to_string()],
            vec![],
        );

        // Batch create and update
        let batch_ops = vec![
            StorageOperation::Create(meta1.clone()),
            StorageOperation::Create(meta2.clone()),
            StorageOperation::UpdateStatus {
                id: meta1.id.clone(),
                status: ContainerStatus::Running,
            },
            StorageOperation::UpdateStatus {
                id: meta2.id.clone(),
                status: ContainerStatus::Running,
            },
        ];

        let batch_op = StorageOperation::Batch(batch_ops);
        let result = state.apply_operation(batch_op);

        assert!(result.is_ok(), "Failed to apply batch operation");

        // Verify operation results
        assert!(state.by_id.contains_key(&meta1.id));
        assert!(state.by_id.contains_key(&meta2.id));

        let stored_meta1 = state.by_id.get(&meta1.id).unwrap();
        let stored_meta2 = state.by_id.get(&meta2.id).unwrap();

        assert_eq!(stored_meta1.state.status, ContainerStatus::Running);
        assert_eq!(stored_meta2.state.status, ContainerStatus::Running);
    }
}
