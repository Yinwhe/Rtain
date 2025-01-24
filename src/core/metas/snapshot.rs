use std::path::PathBuf;

use super::{meta::InnerState, storage::current_time};

#[derive(Debug)]
pub struct Snapshotter {
    snapshot_dir: PathBuf,
    max_snapshots: usize,
}

impl Snapshotter {
    pub async fn new(snapshot_dir: &PathBuf, max_snapshots: usize) -> anyhow::Result<Self> {
        tokio::fs::create_dir_all(&snapshot_dir).await?;
        Ok(Self {
            snapshot_dir: snapshot_dir.to_owned(),
            max_snapshots: max_snapshots,
        })
    }

    pub async fn take_snapshot(&self, state: &InnerState) -> anyhow::Result<()> {
        let tmp_path = self.snapshot_dir.join("tmp.snapshot");
        let final_path = self
            .snapshot_dir
            .join(format!("snapshot-{}.bin", current_time()));

        let data = bincode::serialize(state)?;
        tokio::fs::write(&tmp_path, &data).await?;
        tokio::fs::rename(tmp_path, final_path).await?;

        Ok(())
    }

    pub async fn load_latest(&self) -> anyhow::Result<InnerState> {
        let mut entries = std::fs::read_dir(&self.snapshot_dir)?
            .into_iter()
            .filter_map(|e| e.ok())
            .collect::<Vec<_>>();

        entries.sort_by_key(|e| e.path().metadata().unwrap().modified().unwrap());

        if let Some(entry) = entries.last() {
            let data = tokio::fs::read(entry.path()).await?;
            Ok(bincode::deserialize(&data)?)
        } else {
            Ok(InnerState::default())
        }
    }

    pub async fn purge_old_snapshots(&self) -> anyhow::Result<()> {
        let mut entries = std::fs::read_dir(&self.snapshot_dir)?
            .into_iter()
            .filter_map(|e| e.ok())
            .collect::<Vec<_>>();

        entries.sort_by_key(|e| e.path().metadata().unwrap().modified().unwrap());

        let to_delete = entries.len().saturating_sub(self.max_snapshots);
        if to_delete == 0 {
            return Ok(());
        }

        for entry in entries.into_iter().take(to_delete) {
            tokio::fs::remove_file(entry.path()).await?;
        }

        Ok(())
    }
}
