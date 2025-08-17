// Example usage of the enhanced metadata system
#![allow(dead_code)]

use std::collections::HashMap;
use super::{
    meta::*,
    storage::{StorageConfig, StorageOperation},
};
use crate::core::metas::current_time;

// Example logging event handler
pub struct LoggingEventHandler;

impl MetadataEventHandler for LoggingEventHandler {
    fn handle(&self, event: MetadataEvent) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + '_>> {
        Box::pin(async move {
        match event {
            MetadataEvent::ContainerCreated { id, name } => {
                println!("📦 Container created: {} ({})", name, id);
            }
            MetadataEvent::ContainerDeleted { id, name } => {
                println!("🗑️  Container deleted: {} ({})", name, id);
            }
            MetadataEvent::StatusChanged { id, name, old_status, new_status } => {
                println!("🔄 Container {} ({}) status changed: {:?} -> {:?}", 
                    name, id, old_status, new_status);
            }
            MetadataEvent::ResourcesUpdated { id, resources } => {
                println!("📊 Container {} resources updated: {:?}", id, resources);
            }
            MetadataEvent::NetworkAttached { id, network } => {
                println!("🌐 Network attached to container {}: {:?}", id, network);
            }
            MetadataEvent::HealthChanged { id, old_health, new_health } => {
                println!("❤️  Container {} health changed: {:?} -> {:?}", 
                    id, old_health, new_health);
            }
        }
        })
    }
}

// Usage example
pub async fn example_usage() -> anyhow::Result<()> {
    // 1. Create container manager
    let mut manager = ContainerManager::default().await?;
    
    // 2. Add event handler (commented out due to API changes)
    // TODO: Implement event system
    // manager.add_event_handler(Box::new(LoggingEventHandler)).await;
    
    // 3. Create a complete container metadata
    let container_meta = ContainerMeta::new(
        "container_web_001".to_string(),
        "my-web-app".to_string(),
        "nginx:latest".to_string(),
        vec!["nginx".to_string()],
        vec!["-g".to_string(), "daemon off;".to_string()],
    );
    
    // 4. Register container
    manager.register(container_meta.clone()).await?;
    
    // 5. Update container configuration
    let mut updated_meta = container_meta.clone();
    updated_meta.env = [
        ("PORT".to_string(), "80".to_string()),
        ("ENV".to_string(), "production".to_string()),
    ].into();
    updated_meta.labels = [
        ("app".to_string(), "web".to_string()),
        ("version".to_string(), "1.0".to_string()),
    ].into();
    
    // 6. Update environment variables and labels (commented out due to API changes)
    // TODO: Implement environment and label update methods in ContainerManager
    // manager.update_environment(container_meta.id.clone(), updated_meta.env.clone()).await?;
    // manager.update_labels(container_meta.id.clone(), updated_meta.labels.clone()).await?;
    
    // 7. Set resource limits
    let resources = ResourceConfig {
        memory_limit: Some(512 * 1024 * 1024), // 512MB
        cpu_limit: Some(1.0),                   // 1 core
        pids_limit: Some(1000),
        disk_limit: None,
    };
    manager.update_container_resources(container_meta.id.clone(), resources).await?;
    
    // 8. Configure network
    let network = NetworkConfig {
        ip_address: Some("172.17.0.2".to_string()),
        network_name: "bridge".to_string(),
        mac_address: Some("02:42:ac:11:00:02".to_string()),
        ports: [(80, 8080)].into(),
    };
    manager.attach_network(container_meta.id.clone(), network).await?;
    
    // 9. Add mount point
    let mount = MountPoint {
        source: "/host/data".to_string(),
        destination: "/var/www/data".to_string(),
        mount_type: MountType::Bind,
        read_only: false,
    };
    manager.add_mount(container_meta.id.clone(), mount).await?;
    
    // 10. Status management example
    manager.updates(
        container_meta.id.clone(), 
        ContainerStatus::Running
    ).await?;
    
    // 11. Advanced query examples
    demo_advanced_queries(&manager).await?;
    
    // 12. Batch operation examples
    demo_batch_operations(&manager).await?;
    
    // 13. WAL management examples
    demo_wal_management(&manager).await?;
    
    Ok(())
}

async fn demo_advanced_queries(manager: &ContainerManager) -> anyhow::Result<()> {
    println!("\n=== Advanced Query Examples ===");
    
    // Query by status
    let running_containers = manager.get_containers_by_status(ContainerStatus::Running).await;
    println!("Running containers: {}", running_containers.len());
    
    // Query by label
    let web_containers = manager.get_containers_by_label("app", "web").await;
    println!("Web app containers: {}", web_containers.len());
    
    // Filtered query
    let filter = ContainerFilter {
        status: Some(ContainerStatus::Running),
        labels: [("app".to_string(), "web".to_string())].into(),
        name_pattern: Some("web".to_string()),
        since: Some(current_time() - 3600), // Created within 1 hour
        limit: Some(10),
        ..Default::default()
    };
    let filtered_containers = manager.list_containers(Some(filter)).await;
    println!("Filtered containers: {}", filtered_containers.len());
    
    // Resource statistics
    let summary = manager.get_resource_summary().await;
    println!("Resource Summary:");
    println!("  Total Memory: {} MB", summary.total_memory / 1024 / 1024);
    println!("  Total CPU: {} cores", summary.total_cpu);
    println!("  Running: {}/{}", summary.running_count, summary.total_count);
    for (status, count) in &summary.containers_by_status {
        println!("  {:?}: {}", status, count);
    }
    
    Ok(())
}

async fn demo_batch_operations(manager: &ContainerManager) -> anyhow::Result<()> {
    println!("\n=== Batch Operations Example ===");
    
    // Batch status update
    let operations = vec![
        StorageOperation::UpdateStatus { 
            id: "container_1".to_string(), 
            status: ContainerStatus::Paused 
        },
        StorageOperation::UpdateStatus { 
            id: "container_2".to_string(), 
            status: ContainerStatus::Paused 
        },
    ];
    
    match manager.batch_update(operations).await {
        Ok(()) => println!("Batch operation successful"),
        Err(e) => println!("Batch operation failed: {}", e),
    }
    
    Ok(())
}

async fn demo_wal_management(manager: &ContainerManager) -> anyhow::Result<()> {
    println!("\n=== WAL Management Example ===");
    
    // Verify WAL integrity
    let report = manager.verify_storage_integrity().await?;
    println!("WAL Integrity Report:");
    println!("  Total Operations: {}", report.total_operations);
    println!("  Errors: {}", report.error_count());
    println!("  Success Rate: {:.2}%", report.success_rate() * 100.0);
    
    if !report.is_valid() {
        println!("Errors found:");
        for error in &report.errors {
            println!("  Index {}: {:?}", error.index, error.error);
        }
    }
    
    // Compact WAL (usually after snapshot)
    match manager.compact_storage(100).await {
        Ok(()) => println!("WAL compaction successful"),
        Err(e) => println!("WAL compaction failed: {}", e),
    }
    
    Ok(())
}

// Custom event handler example
pub struct MetricsEventHandler {
    pub container_count: std::sync::atomic::AtomicUsize,
    pub status_changes: std::sync::atomic::AtomicUsize,
}

impl MetadataEventHandler for MetricsEventHandler {
    fn handle(&self, event: MetadataEvent) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + '_>> {
        Box::pin(async move {
        use std::sync::atomic::Ordering;
        
        match event {
            MetadataEvent::ContainerCreated { .. } => {
                self.container_count.fetch_add(1, Ordering::Relaxed);
            }
            MetadataEvent::ContainerDeleted { .. } => {
                self.container_count.fetch_sub(1, Ordering::Relaxed);
            }
            MetadataEvent::StatusChanged { .. } => {
                self.status_changes.fetch_add(1, Ordering::Relaxed);
            }
            _ => {}
        }
        })
    }
}