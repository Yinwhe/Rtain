use std::path::PathBuf;

use tokio::io::AsyncWriteExt;

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
}
