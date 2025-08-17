use std::path::PathBuf;

use tokio::io::AsyncWriteExt;
// Note: Serde imports removed as they're not used in this file

use super::{current_time, storage::StorageOperation};

#[derive(Debug)]
/// Write-ahead loggings.
pub struct WalManager {
    pub current_path: PathBuf,
    pub archive_dir: PathBuf,
    pub max_archives: usize,
}

impl WalManager {
    pub async fn new(wal_dir: &PathBuf, max_wals: usize) -> anyhow::Result<Self> {
        tokio::fs::create_dir_all(&wal_dir).await?;

        Ok(Self {
            current_path: wal_dir.join("current.wal"),
            archive_dir: wal_dir.join("archive"),
            max_archives: max_wals,
        })
    }

    pub async fn write_operation(&self, op: &StorageOperation) -> anyhow::Result<()> {
        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.current_path)
            .await?;

        let serialized_op = bincode::serialize(op)?;

        let length = serialized_op.len() as u64;
        file.write_all(&length.to_le_bytes()).await?;
        file.write_all(&serialized_op).await?;

        Ok(())
    }

    pub async fn read_operations(&self) -> anyhow::Result<Vec<StorageOperation>> {
        let data = match tokio::fs::read(&self.current_path).await {
            Ok(data) => data,
            Err(err) => {
                if err.kind() == std::io::ErrorKind::NotFound {
                    return Ok(Vec::new());
                } else {
                    return Err(err.into());
                }
            }
        };

        let mut operations = Vec::new();
        let mut index = 0;

        while index < data.len() {
            let length = u64::from_le_bytes(data[index..index + 8].try_into().unwrap());
            index += 8;

            let end = index + length as usize;
            let op_data = &data[index..end];
            let op = bincode::deserialize(op_data)?;
            operations.push(op);

            index = end;
        }

        Ok(operations)
    }

    pub async fn rotate(&mut self) -> anyhow::Result<()> {
        let timestamp = current_time();

        let archive_path = self.archive_dir.join(format!("wal-{}.log", timestamp));
        tokio::fs::rename(&self.current_path, archive_path).await?;

        Ok(())
    }

    pub async fn purge_old_archives(&self) -> anyhow::Result<()> {
        let mut entries = std::fs::read_dir(&self.archive_dir)?
            .into_iter()
            .filter_map(|e| e.ok())
            .collect::<Vec<_>>();

        entries.sort_by_key(|e| e.path().metadata().unwrap().modified().unwrap());

        let to_delete = entries.len().saturating_sub(self.max_archives);
        if to_delete == 0 {
            return Ok(());
        }

        for entry in entries.into_iter().take(to_delete) {
            tokio::fs::remove_file(entry.path()).await?;
        }

        Ok(())
    }

    // Support compaction to reduce WAL file size
    pub async fn compact(&self, snapshot_index: u64) -> anyhow::Result<()> {
        let current_entries = self.read_all_operations().await?;
        let filtered_entries: Vec<_> = current_entries
            .into_iter()
            .skip_while(|(index, _)| *index <= snapshot_index)
            .collect();

        self.rewrite_wal(filtered_entries).await
    }

    // Support WAL replay verification
    pub async fn verify_integrity(&self) -> anyhow::Result<IntegrityReport> {
        let operations = self.read_all_operations().await?;
        let mut report = IntegrityReport::default();
        report.total_operations = operations.len();

        for (index, op) in operations {
            if let Err(e) = self.validate_operation(&op) {
                report.errors.push(WalError {
                    index,
                    operation: op,
                    error: e,
                });
            }
        }

        Ok(report)
    }

    // Read all operations with indices
    pub async fn read_all_operations(&self) -> anyhow::Result<Vec<(u64, StorageOperation)>> {
        let data = match tokio::fs::read(&self.current_path).await {
            Ok(data) => data,
            Err(err) => {
                if err.kind() == std::io::ErrorKind::NotFound {
                    return Ok(Vec::new());
                } else {
                    return Err(err.into());
                }
            }
        };

        let mut operations = Vec::new();
        let mut index = 0;
        let mut op_index = 0;

        while index < data.len() {
            let length = u64::from_le_bytes(data[index..index + 8].try_into().unwrap());
            index += 8;

            let end = index + length as usize;
            let op_data = &data[index..end];
            let op = bincode::deserialize(op_data)?;
            operations.push((op_index, op));
            op_index += 1;

            index = end;
        }

        Ok(operations)
    }

    // Rewrite WAL file
    async fn rewrite_wal(&self, operations: Vec<(u64, StorageOperation)>) -> anyhow::Result<()> {
        let temp_path = self.current_path.with_extension("wal.tmp");

        {
            let mut file = tokio::fs::OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(&temp_path)
                .await?;

            for (_, op) in operations {
                let serialized_op = bincode::serialize(&op)?;
                let length = serialized_op.len() as u64;
                file.write_all(&length.to_le_bytes()).await?;
                file.write_all(&serialized_op).await?;
            }
        }

        tokio::fs::rename(temp_path, &self.current_path).await?;
        Ok(())
    }

    // Validate operation correctness
    fn validate_operation(&self, op: &StorageOperation) -> anyhow::Result<()> {
        match op {
            StorageOperation::Create(meta) => {
                if meta.id.is_empty() || meta.name.is_empty() {
                    return Err(anyhow::anyhow!("Container ID or name cannot be empty"));
                }
            }
            StorageOperation::UpdateStatus { id, .. }
            | StorageOperation::UpdateState { id, .. }
            | StorageOperation::Delete(id) => {
                if id.is_empty() {
                    return Err(anyhow::anyhow!("Container ID cannot be empty"));
                }
            }
            StorageOperation::Batch(ops) => {
                for op in ops {
                    self.validate_operation(op)?;
                }
            }
            _ => {} // Other operations don't need special validation
        }
        Ok(())
    }
}

// Integrity report
#[derive(Debug, Default)]
pub struct IntegrityReport {
    pub errors: Vec<WalError>,
    pub total_operations: usize,
}

#[derive(Debug)]
pub struct WalError {
    pub index: u64,
    pub operation: StorageOperation,
    pub error: anyhow::Error,
}

impl IntegrityReport {
    pub fn is_valid(&self) -> bool {
        self.errors.is_empty()
    }

    pub fn error_count(&self) -> usize {
        self.errors.len()
    }

    pub fn success_rate(&self) -> f64 {
        if self.total_operations == 0 {
            return 1.0;
        }
        1.0 - (self.errors.len() as f64 / self.total_operations as f64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::metas::meta::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_wal_basic_operations() {
        let temp_dir = TempDir::new().unwrap();
        let wal_manager = WalManager::new(&temp_dir.path().to_path_buf(), 5)
            .await
            .unwrap();

        // Test write operation
        let meta = ContainerMeta::new(
            "test_id".to_string(),
            "test_container".to_string(),
            "nginx:latest".to_string(),
            vec!["nginx".to_string()],
            vec![],
        );
        let op = StorageOperation::Create(meta);

        let result = wal_manager.write_operation(&op).await;
        assert!(result.is_ok(), "Failed to write operation to WAL");

        // Test read operation
        let operations = wal_manager.read_operations().await.unwrap();
        assert_eq!(operations.len(), 1);

        if let StorageOperation::Create(read_meta) = &operations[0] {
            assert_eq!(read_meta.id, "test_id");
            assert_eq!(read_meta.name, "test_container");
        } else {
            panic!("Expected Create operation");
        }
    }

    #[tokio::test]
    async fn test_wal_multiple_operations() {
        let temp_dir = TempDir::new().unwrap();
        let wal_manager = WalManager::new(&temp_dir.path().to_path_buf(), 5)
            .await
            .unwrap();

        // Write multiple operations
        let ops = vec![
            StorageOperation::Create(ContainerMeta::new(
                "container1".to_string(),
                "test1".to_string(),
                "nginx:latest".to_string(),
                vec!["nginx".to_string()],
                vec![],
            )),
            StorageOperation::UpdateStatus {
                id: "container1".to_string(),
                status: ContainerStatus::Running,
            },
            StorageOperation::Delete("container1".to_string()),
        ];

        for op in &ops {
            wal_manager.write_operation(op).await.unwrap();
        }

        // Read and verify
        let read_ops = wal_manager.read_operations().await.unwrap();
        assert_eq!(read_ops.len(), 3);

        // Verify operation types
        assert!(matches!(read_ops[0], StorageOperation::Create(_)));
        assert!(matches!(read_ops[1], StorageOperation::UpdateStatus { .. }));
        assert!(matches!(read_ops[2], StorageOperation::Delete(_)));
    }

    #[tokio::test]
    async fn test_wal_integrity_verification() {
        let temp_dir = TempDir::new().unwrap();
        let wal_manager = WalManager::new(&temp_dir.path().to_path_buf(), 5)
            .await
            .unwrap();

        // Write valid operations
        let valid_ops = vec![
            StorageOperation::Create(ContainerMeta::new(
                "valid_container".to_string(),
                "valid_name".to_string(),
                "nginx:latest".to_string(),
                vec!["nginx".to_string()],
                vec![],
            )),
            StorageOperation::UpdateStatus {
                id: "valid_container".to_string(),
                status: ContainerStatus::Running,
            },
        ];

        for op in &valid_ops {
            wal_manager.write_operation(op).await.unwrap();
        }

        // Write invalid operation (empty ID)
        let invalid_op = StorageOperation::Delete("".to_string());
        wal_manager.write_operation(&invalid_op).await.unwrap();

        // Verify integrity
        let report = wal_manager.verify_integrity().await.unwrap();
        assert_eq!(report.total_operations, 3);
        assert_eq!(report.error_count(), 1);
        assert!(!report.is_valid());
        assert!((report.success_rate() - 2.0 / 3.0).abs() < 1e-10);

        // Check error details
        assert_eq!(report.errors.len(), 1);
        assert_eq!(report.errors[0].index, 2);
    }

    #[tokio::test]
    async fn test_wal_compaction() {
        let temp_dir = TempDir::new().unwrap();
        let wal_manager = WalManager::new(&temp_dir.path().to_path_buf(), 5)
            .await
            .unwrap();

        // Write multiple operations
        let ops = vec![
            StorageOperation::Create(ContainerMeta::new(
                "container1".to_string(),
                "test1".to_string(),
                "nginx:latest".to_string(),
                vec!["nginx".to_string()],
                vec![],
            )),
            StorageOperation::Create(ContainerMeta::new(
                "container2".to_string(),
                "test2".to_string(),
                "nginx:latest".to_string(),
                vec!["nginx".to_string()],
                vec![],
            )),
            StorageOperation::UpdateStatus {
                id: "container1".to_string(),
                status: ContainerStatus::Running,
            },
            StorageOperation::Delete("container1".to_string()),
        ];

        for op in &ops {
            wal_manager.write_operation(op).await.unwrap();
        }

        // Compact WAL (keep operations with index > 2)
        wal_manager.compact(2).await.unwrap();

        // Verify compaction results
        let remaining_ops = wal_manager.read_operations().await.unwrap();
        assert_eq!(remaining_ops.len(), 1); // Only the last operation remains

        if let StorageOperation::Delete(id) = &remaining_ops[0] {
            assert_eq!(id, "container1");
        } else {
            panic!("Expected Delete operation after compaction");
        }
    }

    #[tokio::test]
    async fn test_wal_read_all_operations_with_indices() {
        let temp_dir = TempDir::new().unwrap();
        let wal_manager = WalManager::new(&temp_dir.path().to_path_buf(), 5)
            .await
            .unwrap();

        // Write operations
        let ops = vec![
            StorageOperation::Create(ContainerMeta::new(
                "container1".to_string(),
                "test1".to_string(),
                "nginx:latest".to_string(),
                vec!["nginx".to_string()],
                vec![],
            )),
            StorageOperation::UpdateStatus {
                id: "container1".to_string(),
                status: ContainerStatus::Running,
            },
        ];

        for op in &ops {
            wal_manager.write_operation(op).await.unwrap();
        }

        // Read indexed operations
        let indexed_ops = wal_manager.read_all_operations().await.unwrap();
        assert_eq!(indexed_ops.len(), 2);

        // Verify indices
        assert_eq!(indexed_ops[0].0, 0);
        assert_eq!(indexed_ops[1].0, 1);

        // Verify operation types
        assert!(matches!(indexed_ops[0].1, StorageOperation::Create(_)));
        assert!(matches!(
            indexed_ops[1].1,
            StorageOperation::UpdateStatus { .. }
        ));
    }

    #[tokio::test]
    async fn test_wal_validation() {
        let temp_dir = TempDir::new().unwrap();
        let wal_manager = WalManager::new(&temp_dir.path().to_path_buf(), 5)
            .await
            .unwrap();

        // Test valid operations
        let valid_meta = ContainerMeta::new(
            "valid_id".to_string(),
            "valid_name".to_string(),
            "nginx:latest".to_string(),
            vec!["nginx".to_string()],
            vec![],
        );
        let valid_op = StorageOperation::Create(valid_meta);
        assert!(wal_manager.validate_operation(&valid_op).is_ok());

        // Test invalid operation (empty ID)
        let invalid_meta = ContainerMeta::new(
            "".to_string(), // Empty ID
            "name".to_string(),
            "nginx:latest".to_string(),
            vec!["nginx".to_string()],
            vec![],
        );
        let invalid_op = StorageOperation::Create(invalid_meta);
        assert!(wal_manager.validate_operation(&invalid_op).is_err());

        // Test empty name
        let invalid_name_meta = ContainerMeta::new(
            "id".to_string(),
            "".to_string(), // Empty name
            "nginx:latest".to_string(),
            vec!["nginx".to_string()],
            vec![],
        );
        let invalid_name_op = StorageOperation::Create(invalid_name_meta);
        assert!(wal_manager.validate_operation(&invalid_name_op).is_err());

        // Test batch operation validation
        let batch_op = StorageOperation::Batch(vec![valid_op, invalid_op]);
        assert!(wal_manager.validate_operation(&batch_op).is_err());
    }

    #[tokio::test]
    async fn test_wal_empty_file() {
        let temp_dir = TempDir::new().unwrap();
        let wal_manager = WalManager::new(&temp_dir.path().to_path_buf(), 5)
            .await
            .unwrap();

        // Test reading non-existent file
        let operations = wal_manager.read_operations().await.unwrap();
        assert_eq!(operations.len(), 0);

        // Test verifying empty file
        let report = wal_manager.verify_integrity().await.unwrap();
        assert_eq!(report.total_operations, 0);
        assert!(report.is_valid());
        assert_eq!(report.success_rate(), 1.0);
    }
}
