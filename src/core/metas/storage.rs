use std::{path::PathBuf, sync::Arc, time::Duration};

use serde::{Deserialize, Serialize};
use tokio::{
    sync::{mpsc, oneshot, Mutex},
    task::JoinHandle,
};

use super::{
    meta::{ContainerMeta, ContainerStatus, InnerState},
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

#[derive(Debug)]
pub struct StorageManager {
    op_sender: Arc<Mutex<mpsc::Sender<(StorageOperation, oneshot::Sender<anyhow::Result<()>>)>>>,
    inner: Arc<Mutex<StorageInner>>,
    #[allow(unused)]
    worker: JoinHandle<()>,
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
    Create(ContainerMeta),
    UpdateStatus { id: String, status: ContainerStatus },
    Delete(String),
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

        let meta = ContainerMeta {
            id: "container1".to_string(),
            name: "test_container".to_string(),
            command: vec!["/bin/bash".to_string()],
            status: ContainerStatus::Running {
                pid: 1234,
                started_at: 1625238290,
            },
            created_at: 1625238290,
        };

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
            status: ContainerStatus::Stopped {
                stopped_at: 1625238390,
            },
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

        let meta = ContainerMeta {
            id: "container1".to_string(),
            name: "test_container".to_string(),
            command: vec!["/bin/bash".to_string()],
            status: ContainerStatus::Running {
                pid: 1234,
                started_at: 1625238290,
            },
            created_at: 1625238290,
        };

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

        let meta = ContainerMeta {
            id: "container1".to_string(),
            name: "test_container".to_string(),
            command: vec!["/bin/bash".to_string()],
            status: ContainerStatus::Running {
                pid: 1234,
                started_at: 1625238290,
            },
            created_at: 1625238290,
        };

        let op = StorageOperation::Create(meta.clone());
        state.apply_operation(op).unwrap();

        let new_status = ContainerStatus::Stopped {
            stopped_at: 1625238390,
        };
        let update_op = StorageOperation::UpdateStatus {
            id: meta.id.clone(),
            status: new_status.clone(),
        };

        let result = state.apply_operation(update_op);

        assert!(result.is_ok(), "Failed to apply update status operation");

        if let Some(updated_meta) = state.by_id.get(&meta.id) {
            assert_eq!(updated_meta.status, new_status, "Status update failed");
        } else {
            panic!("Container not found after status update");
        };
    }

    #[tokio::test]
    async fn test_inner_state_apply_delete_operation() {
        let state = InnerState::default();

        let meta = ContainerMeta {
            id: "container1".to_string(),
            name: "test_container".to_string(),
            command: vec!["/bin/bash".to_string()],
            status: ContainerStatus::Running {
                pid: 1234,
                started_at: 1625238290,
            },
            created_at: 1625238290,
        };

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
}
