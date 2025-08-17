use std::{collections::HashMap, path::PathBuf};

use dashmap::DashMap;
use serde::{Deserialize, Serialize};

use crate::core::ROOT_PATH;

use super::{
    current_time,
    storage::{StorageConfig, StorageManager, StorageOperation},
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ContainerStatus {
    Creating,   // Being created
    Running,    // Currently running
    Paused,     // Paused
    Restarting, // Restarting
    Removing,   // Being removed
    Exited,     // Has exited
    Dead,       // Dead (cannot operate)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum HealthStatus {
    Unknown,
    Starting,
    Healthy,
    Unhealthy,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContainerState {
    pub status: ContainerStatus,
    pub pid: Option<i32>,
    pub started_at: Option<u64>,
    pub finished_at: Option<u64>,
    pub exit_code: Option<i32>,
    pub error: Option<String>,
    pub restart_count: u32,
    pub health_status: HealthStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NetworkConfig {
    pub ip_address: Option<String>,
    pub network_name: String,
    pub mac_address: Option<String>,
    pub ports: HashMap<u16, u16>, // host_port -> container_port
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResourceConfig {
    pub memory_limit: Option<u64>, // bytes
    pub cpu_limit: Option<f64>,    // cores
    pub pids_limit: Option<u64>,
    pub disk_limit: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MountPoint {
    pub source: String,      // host path
    pub destination: String, // container path
    pub mount_type: MountType,
    pub read_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MountType {
    Bind,
    Volume,
    Tmpfs,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContainerMeta {
    // Basic information
    pub id: String,
    pub name: String,
    pub created_at: u64,
    pub updated_at: u64,

    // Configuration information
    pub image: String,
    pub command: Vec<String>,
    pub args: Vec<String>,
    pub working_dir: Option<String>,
    pub user: Option<String>,

    // Environment and labels
    pub env: HashMap<String, String>,
    pub labels: HashMap<String, String>,

    // State information
    pub state: ContainerState,

    // Network information
    pub network: Option<NetworkConfig>,

    // Resource configuration
    pub resources: ResourceConfig,

    // Mount information
    pub mounts: Vec<MountPoint>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct InnerState {
    pub by_id: DashMap<String, ContainerMeta>,
    pub by_name: DashMap<String, String>,
}

/// Uplevel container manager.
#[derive(Debug)]
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
            wal_dir: PathBuf::from(format!("{ROOT_PATH}/containermetas/wal")),
            snapshots_dir: PathBuf::from(format!("{ROOT_PATH}/containermetas/snapshots")),

            max_wals: 10,
            max_snapshots: 10,

            snapshot_intervals_secs: 60,
            cleanup_interval_secs: 3 * 60,
        };

        Self::new(config).await
    }

    #[inline]
    pub async fn register(&self, meta: ContainerMeta) -> anyhow::Result<()> {
        self.storage.execute(StorageOperation::Create(meta)).await
    }

    #[inline]
    pub async fn updates(&self, id: String, status: ContainerStatus) -> anyhow::Result<()> {
        self.storage
            .execute(StorageOperation::UpdateStatus { id, status })
            .await
    }

    // Enhanced query functionality
    pub async fn list_containers(&self, filter: Option<ContainerFilter>) -> Vec<ContainerMeta> {
        let all_metas = self.storage.get_all_metas().await;

        let Some(filter) = filter else {
            return all_metas;
        };

        let mut filtered: Vec<_> = all_metas
            .into_iter()
            .filter(|meta| filter.matches(meta))
            .collect();

        if let Some(limit) = filter.limit {
            filtered.truncate(limit);
        }

        filtered
    }

    pub async fn get_containers_by_status(&self, status: ContainerStatus) -> Vec<ContainerMeta> {
        self.list_containers(Some(ContainerFilter {
            status: Some(status),
            ..Default::default()
        }))
        .await
    }

    pub async fn get_containers_by_label(&self, key: &str, value: &str) -> Vec<ContainerMeta> {
        self.list_containers(Some(ContainerFilter {
            labels: [(key.to_string(), value.to_string())].into(),
            ..Default::default()
        }))
        .await
    }

    // Statistics
    pub async fn get_resource_summary(&self) -> ResourceSummary {
        let containers = self.storage.get_all_metas().await;

        let mut summary = ResourceSummary::default();
        summary.total_count = containers.len();

        for container in &containers {
            // Resource statistics
            if let Some(memory) = container.resources.memory_limit {
                summary.total_memory += memory;
            }
            if let Some(cpu) = container.resources.cpu_limit {
                summary.total_cpu += cpu;
            }

            // Status statistics
            *summary
                .containers_by_status
                .entry(container.state.status.clone())
                .or_insert(0) += 1;

            if container.state.status.is_running() {
                summary.running_count += 1;
            }
        }

        summary
    }

    #[inline]
    pub async fn deregister(&self, id: String) -> anyhow::Result<()> {
        self.storage.execute(StorageOperation::Delete(id)).await
    }

    // Batch operations
    pub async fn batch_update(&self, operations: Vec<StorageOperation>) -> anyhow::Result<()> {
        self.storage
            .execute(StorageOperation::Batch(operations))
            .await
    }

    // Event system support temporarily removed for compilation
    // TODO: Implement event system properly

    // Advanced container management methods
    pub async fn update_container_resources(
        &self,
        id: String,
        resources: ResourceConfig,
    ) -> anyhow::Result<()> {
        self.storage
            .execute(StorageOperation::UpdateResources { id, resources })
            .await
    }

    pub async fn attach_network(&self, id: String, network: NetworkConfig) -> anyhow::Result<()> {
        self.storage
            .execute(StorageOperation::AttachNetwork { id, network })
            .await
    }

    pub async fn detach_network(&self, id: String) -> anyhow::Result<()> {
        self.storage
            .execute(StorageOperation::DetachNetwork { id })
            .await
    }

    pub async fn add_mount(&self, id: String, mount: MountPoint) -> anyhow::Result<()> {
        self.storage
            .execute(StorageOperation::AddMount { id, mount })
            .await
    }

    pub async fn remove_mount(&self, id: String, destination: String) -> anyhow::Result<()> {
        self.storage
            .execute(StorageOperation::RemoveMount { id, destination })
            .await
    }

    // WAL management
    pub async fn compact_storage(&self, snapshot_index: u64) -> anyhow::Result<()> {
        self.storage.compact_wal(snapshot_index).await
    }

    pub async fn verify_storage_integrity(&self) -> anyhow::Result<super::wal::IntegrityReport> {
        self.storage.verify_wal_integrity().await
    }

    #[inline]
    #[allow(unused)]
    pub async fn get_meta_by_id(&self, id: &str) -> Option<ContainerMeta> {
        self.storage.get_meta_by_id(id).await
    }

    #[inline]
    pub async fn get_meta_by_name(&self, name: &str) -> Option<ContainerMeta> {
        self.storage.get_meta_by_name(name).await
    }

    #[inline]
    pub async fn get_all_metas(&self) -> Vec<ContainerMeta> {
        self.storage.get_all_metas().await
    }
}

impl ContainerMeta {
    pub fn new(
        id: String,
        name: String,
        image: String,
        command: Vec<String>,
        args: Vec<String>,
    ) -> Self {
        let time = current_time();
        Self {
            id,
            name,
            created_at: time,
            updated_at: time,
            image,
            command,
            args,
            working_dir: None,
            user: None,
            env: HashMap::new(),
            labels: HashMap::new(),
            state: ContainerState {
                status: ContainerStatus::Creating,
                pid: None,
                started_at: None,
                finished_at: None,
                exit_code: None,
                error: None,
                restart_count: 0,
                health_status: HealthStatus::Unknown,
            },
            network: None,
            resources: ResourceConfig {
                memory_limit: None,
                cpu_limit: None,
                pids_limit: None,
                disk_limit: None,
            },
            mounts: Vec::new(),
        }
    }

    pub fn get_pid(&self) -> Option<i32> {
        self.state.pid
    }

    pub fn set_running(&mut self, pid: i32) {
        self.state.status = ContainerStatus::Running;
        self.state.pid = Some(pid);
        self.state.started_at = Some(current_time());
        self.updated_at = current_time();
    }

    pub fn set_stopped(&mut self, exit_code: Option<i32>, error: Option<String>) {
        self.state.status = ContainerStatus::Exited;
        self.state.pid = None;
        self.state.finished_at = Some(current_time());
        self.state.exit_code = exit_code;
        self.state.error = error;
        self.updated_at = current_time();
    }
}

impl ContainerStatus {
    pub fn is_running(&self) -> bool {
        matches!(self, Self::Running)
    }

    pub fn is_stopped(&self) -> bool {
        matches!(self, Self::Exited | Self::Dead)
    }

    pub fn can_start(&self) -> bool {
        matches!(self, Self::Exited | Self::Dead)
    }

    pub fn can_stop(&self) -> bool {
        matches!(self, Self::Running | Self::Paused)
    }
}

impl InnerState {
    pub fn apply_operation(&self, op: StorageOperation) -> anyhow::Result<()> {
        match op {
            StorageOperation::Create(meta) => {
                self.by_name.insert(meta.name.clone(), meta.id.clone());
                self.by_id.insert(meta.id.clone(), meta);
            }
            StorageOperation::Delete(id) => {
                if let Some((_, meta)) = self.by_id.remove(&id) {
                    self.by_name.remove(&meta.name);
                }
            }
            StorageOperation::UpdateStatus { id, status } => {
                if let Some(mut entry) = self.by_id.get_mut(&id) {
                    entry.state.status = status;
                    entry.updated_at = current_time();
                }
            }
            StorageOperation::UpdateState { id, state } => {
                if let Some(mut entry) = self.by_id.get_mut(&id) {
                    entry.state = state;
                    entry.updated_at = current_time();
                }
            }
            StorageOperation::UpdateEnvironment { id, env } => {
                if let Some(mut entry) = self.by_id.get_mut(&id) {
                    entry.env = env;
                    entry.updated_at = current_time();
                }
            }
            StorageOperation::UpdateLabels { id, labels } => {
                if let Some(mut entry) = self.by_id.get_mut(&id) {
                    entry.labels = labels;
                    entry.updated_at = current_time();
                }
            }
            StorageOperation::UpdateResources { id, resources } => {
                if let Some(mut entry) = self.by_id.get_mut(&id) {
                    entry.resources = resources;
                    entry.updated_at = current_time();
                }
            }
            StorageOperation::AttachNetwork { id, network } => {
                if let Some(mut entry) = self.by_id.get_mut(&id) {
                    entry.network = Some(network);
                    entry.updated_at = current_time();
                }
            }
            StorageOperation::DetachNetwork { id } => {
                if let Some(mut entry) = self.by_id.get_mut(&id) {
                    entry.network = None;
                    entry.updated_at = current_time();
                }
            }
            StorageOperation::AddMount { id, mount } => {
                if let Some(mut entry) = self.by_id.get_mut(&id) {
                    entry.mounts.push(mount);
                    entry.updated_at = current_time();
                }
            }
            StorageOperation::RemoveMount { id, destination } => {
                if let Some(mut entry) = self.by_id.get_mut(&id) {
                    entry.mounts.retain(|m| m.destination != destination);
                    entry.updated_at = current_time();
                }
            }
            StorageOperation::Batch(operations) => {
                for op in operations {
                    self.apply_operation(op)?;
                }
            }
        }
        Ok(())
    }
}

// Query filter
#[derive(Debug, Default)]
pub struct ContainerFilter {
    pub status: Option<ContainerStatus>,
    pub labels: HashMap<String, String>,
    pub name_pattern: Option<String>,
    pub since: Option<u64>,
    pub until: Option<u64>,
    pub limit: Option<usize>,
}

impl ContainerFilter {
    pub fn matches(&self, meta: &ContainerMeta) -> bool {
        // Status filtering
        if let Some(ref status) = self.status {
            if &meta.state.status != status {
                return false;
            }
        }

        // Label filtering
        for (key, value) in &self.labels {
            if meta.labels.get(key) != Some(value) {
                return false;
            }
        }

        // Name pattern matching
        if let Some(ref pattern) = self.name_pattern {
            if !meta.name.contains(pattern) {
                return false;
            }
        }

        // Time range filtering
        if let Some(since) = self.since {
            if meta.created_at < since {
                return false;
            }
        }

        if let Some(until) = self.until {
            if meta.created_at > until {
                return false;
            }
        }

        true
    }
}

// Resource summary
#[derive(Debug, Default)]
pub struct ResourceSummary {
    pub total_memory: u64,
    pub total_cpu: f64,
    pub running_count: usize,
    pub total_count: usize,
    pub containers_by_status: HashMap<ContainerStatus, usize>,
}

// Event definitions
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MetadataEvent {
    ContainerCreated {
        id: String,
        name: String,
    },
    ContainerDeleted {
        id: String,
        name: String,
    },
    StatusChanged {
        id: String,
        name: String,
        old_status: ContainerStatus,
        new_status: ContainerStatus,
    },
    ResourcesUpdated {
        id: String,
        resources: ResourceConfig,
    },
    NetworkAttached {
        id: String,
        network: NetworkConfig,
    },
    HealthChanged {
        id: String,
        old_health: HealthStatus,
        new_health: HealthStatus,
    },
}

// Event handler
pub trait MetadataEventHandler: Send + Sync {
    fn handle(
        &self,
        event: MetadataEvent,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + '_>>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use tempfile::TempDir;

    // Test event handler
    #[derive(Debug, Default)]
    struct TestEventHandler {
        events: Arc<Mutex<Vec<MetadataEvent>>>,
    }

    impl TestEventHandler {
        fn new() -> Self {
            Self {
                events: Arc::new(Mutex::new(Vec::new())),
            }
        }

        fn get_events(&self) -> Vec<MetadataEvent> {
            self.events.lock().unwrap().clone()
        }

        fn clear_events(&self) {
            self.events.lock().unwrap().clear();
        }
    }

    impl MetadataEventHandler for TestEventHandler {
        fn handle(
            &self,
            event: MetadataEvent,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + '_>> {
            Box::pin(async move {
                self.events.lock().unwrap().push(event);
            })
        }
    }

    #[test]
    fn test_container_meta_new() {
        let meta = ContainerMeta::new(
            "test_id".to_string(),
            "test_container".to_string(),
            "nginx:latest".to_string(),
            vec!["nginx".to_string()],
            vec!["-g".to_string(), "daemon off;".to_string()],
        );

        assert_eq!(meta.id, "test_id");
        assert_eq!(meta.name, "test_container");
        assert_eq!(meta.image, "nginx:latest");
        assert_eq!(meta.command, vec!["nginx"]);
        assert_eq!(meta.args, vec!["-g", "daemon off;"]);
        assert_eq!(meta.state.status, ContainerStatus::Creating);
        assert!(meta.env.is_empty());
        assert!(meta.labels.is_empty());
        assert!(meta.mounts.is_empty());
    }

    #[test]
    fn test_container_state_transitions() {
        let mut meta = ContainerMeta::new(
            "test_id".to_string(),
            "test_container".to_string(),
            "nginx:latest".to_string(),
            vec!["nginx".to_string()],
            vec![],
        );

        // Test setting running state
        meta.set_running(1234);
        assert_eq!(meta.state.status, ContainerStatus::Running);
        assert_eq!(meta.state.pid, Some(1234));
        assert!(meta.state.started_at.is_some());

        // Test setting stopped state
        meta.set_stopped(Some(0), None);
        assert_eq!(meta.state.status, ContainerStatus::Exited);
        assert_eq!(meta.state.pid, None);
        assert_eq!(meta.state.exit_code, Some(0));
        assert!(meta.state.finished_at.is_some());

        // Test stopped with error
        meta.set_stopped(Some(1), Some("Error occurred".to_string()));
        assert_eq!(meta.state.exit_code, Some(1));
        assert_eq!(meta.state.error, Some("Error occurred".to_string()));
    }

    #[test]
    fn test_container_status_methods() {
        assert!(ContainerStatus::Running.is_running());
        assert!(!ContainerStatus::Exited.is_running());
        assert!(!ContainerStatus::Creating.is_running());

        assert!(ContainerStatus::Exited.is_stopped());
        assert!(ContainerStatus::Dead.is_stopped());
        assert!(!ContainerStatus::Running.is_stopped());

        assert!(ContainerStatus::Exited.can_start());
        assert!(ContainerStatus::Dead.can_start());
        assert!(!ContainerStatus::Running.can_start());

        assert!(ContainerStatus::Running.can_stop());
        assert!(ContainerStatus::Paused.can_stop());
        assert!(!ContainerStatus::Exited.can_stop());
    }

    #[test]
    fn test_container_filter_matches() {
        let mut meta = ContainerMeta::new(
            "test_id".to_string(),
            "web-app".to_string(),
            "nginx:latest".to_string(),
            vec!["nginx".to_string()],
            vec![],
        );
        meta.state.status = ContainerStatus::Running;
        meta.labels.insert("app".to_string(), "web".to_string());
        meta.labels.insert("env".to_string(), "prod".to_string());

        // Test status filtering
        let filter = ContainerFilter {
            status: Some(ContainerStatus::Running),
            ..Default::default()
        };
        assert!(filter.matches(&meta));

        let filter = ContainerFilter {
            status: Some(ContainerStatus::Exited),
            ..Default::default()
        };
        assert!(!filter.matches(&meta));

        // Test label filtering
        let filter = ContainerFilter {
            labels: [("app".to_string(), "web".to_string())].into(),
            ..Default::default()
        };
        assert!(filter.matches(&meta));

        let filter = ContainerFilter {
            labels: [("app".to_string(), "db".to_string())].into(),
            ..Default::default()
        };
        assert!(!filter.matches(&meta));

        // Test name pattern matching
        let filter = ContainerFilter {
            name_pattern: Some("web".to_string()),
            ..Default::default()
        };
        assert!(filter.matches(&meta));

        let filter = ContainerFilter {
            name_pattern: Some("database".to_string()),
            ..Default::default()
        };
        assert!(!filter.matches(&meta));

        // Test time filtering
        let now = current_time();
        let filter = ContainerFilter {
            since: Some(now - 100),
            until: Some(now + 100),
            ..Default::default()
        };
        assert!(filter.matches(&meta));

        let filter = ContainerFilter {
            since: Some(now + 100),
            ..Default::default()
        };
        assert!(!filter.matches(&meta));
    }

    #[test]
    fn test_resource_config() {
        let resources = ResourceConfig {
            memory_limit: Some(512 * 1024 * 1024), // 512MB
            cpu_limit: Some(1.5),
            pids_limit: Some(1000),
            disk_limit: None,
        };

        assert_eq!(resources.memory_limit, Some(512 * 1024 * 1024));
        assert_eq!(resources.cpu_limit, Some(1.5));
        assert_eq!(resources.pids_limit, Some(1000));
        assert_eq!(resources.disk_limit, None);
    }

    #[test]
    fn test_network_config() {
        let network = NetworkConfig {
            ip_address: Some("172.17.0.2".to_string()),
            network_name: "bridge".to_string(),
            mac_address: Some("02:42:ac:11:00:02".to_string()),
            ports: [(80, 8080), (443, 8443)].into(),
        };

        assert_eq!(network.ip_address, Some("172.17.0.2".to_string()));
        assert_eq!(network.network_name, "bridge");
        assert_eq!(network.ports.get(&80), Some(&8080));
        assert_eq!(network.ports.get(&443), Some(&8443));
    }

    #[test]
    fn test_mount_point() {
        let mount = MountPoint {
            source: "/host/data".to_string(),
            destination: "/app/data".to_string(),
            mount_type: MountType::Bind,
            read_only: false,
        };

        assert_eq!(mount.source, "/host/data");
        assert_eq!(mount.destination, "/app/data");
        assert!(matches!(mount.mount_type, MountType::Bind));
        assert!(!mount.read_only);
    }

    #[tokio::test]
    async fn test_container_manager_basic_operations() {
        let temp_wal = TempDir::new().unwrap();
        let temp_snapshots = TempDir::new().unwrap();

        let config = StorageConfig {
            wal_dir: temp_wal.path().to_path_buf(),
            snapshots_dir: temp_snapshots.path().to_path_buf(),
            max_wals: 5,
            max_snapshots: 3,
            snapshot_intervals_secs: 60,
            cleanup_interval_secs: 180,
        };

        let manager = ContainerManager::new(config).await.unwrap();

        // Create test container
        let meta = ContainerMeta::new(
            "test_id".to_string(),
            "test_container".to_string(),
            "nginx:latest".to_string(),
            vec!["nginx".to_string()],
            vec![],
        );

        // Register container
        manager.register(meta.clone()).await.unwrap();

        // Verify container exists
        let retrieved = manager.get_meta_by_id(&meta.id).await;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().name, meta.name);

        let retrieved_by_name = manager.get_meta_by_name(&meta.name).await;
        assert!(retrieved_by_name.is_some());
        assert_eq!(retrieved_by_name.unwrap().id, meta.id);

        // Update status
        manager
            .updates(meta.id.clone(), ContainerStatus::Running)
            .await
            .unwrap();
        let updated = manager.get_meta_by_id(&meta.id).await.unwrap();
        assert_eq!(updated.state.status, ContainerStatus::Running);

        // Delete container
        manager.deregister(meta.id.clone()).await.unwrap();
        let deleted = manager.get_meta_by_id(&meta.id).await;
        assert!(deleted.is_none());
    }

    #[tokio::test]
    async fn test_advanced_queries() {
        let temp_wal = TempDir::new().unwrap();
        let temp_snapshots = TempDir::new().unwrap();

        let config = StorageConfig {
            wal_dir: temp_wal.path().to_path_buf(),
            snapshots_dir: temp_snapshots.path().to_path_buf(),
            max_wals: 5,
            max_snapshots: 3,
            snapshot_intervals_secs: 60,
            cleanup_interval_secs: 180,
        };

        let manager = ContainerManager::new(config).await.unwrap();

        // Create multiple test containers
        let mut web_container = ContainerMeta::new(
            "web_1".to_string(),
            "web-server".to_string(),
            "nginx:latest".to_string(),
            vec!["nginx".to_string()],
            vec![],
        );
        web_container.state.status = ContainerStatus::Running;
        web_container
            .labels
            .insert("app".to_string(), "web".to_string());
        web_container.resources.memory_limit = Some(512 * 1024 * 1024);
        web_container.resources.cpu_limit = Some(1.0);

        let mut db_container = ContainerMeta::new(
            "db_1".to_string(),
            "database".to_string(),
            "postgres:13".to_string(),
            vec!["postgres".to_string()],
            vec![],
        );
        db_container.state.status = ContainerStatus::Exited;
        db_container
            .labels
            .insert("app".to_string(), "db".to_string());
        db_container.resources.memory_limit = Some(1024 * 1024 * 1024);
        db_container.resources.cpu_limit = Some(2.0);

        // Register container
        manager.register(web_container).await.unwrap();
        manager.register(db_container).await.unwrap();

        // Test query by status
        let running_containers = manager
            .get_containers_by_status(ContainerStatus::Running)
            .await;
        assert_eq!(running_containers.len(), 1);
        assert_eq!(running_containers[0].name, "web-server");

        let stopped_containers = manager
            .get_containers_by_status(ContainerStatus::Exited)
            .await;
        assert_eq!(stopped_containers.len(), 1);
        assert_eq!(stopped_containers[0].name, "database");

        // Test query by label
        let web_containers = manager.get_containers_by_label("app", "web").await;
        assert_eq!(web_containers.len(), 1);
        assert_eq!(web_containers[0].name, "web-server");

        let db_containers = manager.get_containers_by_label("app", "db").await;
        assert_eq!(db_containers.len(), 1);
        assert_eq!(db_containers[0].name, "database");

        // Test resource summary
        let summary = manager.get_resource_summary().await;
        assert_eq!(summary.total_count, 2);
        assert_eq!(summary.running_count, 1);
        assert_eq!(summary.total_memory, 1536 * 1024 * 1024); // 512MB + 1024MB
        assert_eq!(summary.total_cpu, 3.0); // 1.0 + 2.0
        assert_eq!(
            summary.containers_by_status.get(&ContainerStatus::Running),
            Some(&1)
        );
        assert_eq!(
            summary.containers_by_status.get(&ContainerStatus::Exited),
            Some(&1)
        );

        // Test complex filtering
        let filter = ContainerFilter {
            status: Some(ContainerStatus::Running),
            labels: [("app".to_string(), "web".to_string())].into(),
            name_pattern: Some("web".to_string()),
            limit: Some(10),
            ..Default::default()
        };
        let filtered = manager.list_containers(Some(filter)).await;
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].name, "web-server");
    }
}
