use std::{
    collections::HashMap,
    fs::{File, OpenOptions},
    io::Read,
    path::PathBuf,
    sync::RwLock,
};

use serde::{Deserialize, Serialize};

use crate::core::error::SimpleError;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ContainerRecord {
    pub id: String,
    pub name: String,
    pub pid: i32,
    pub command: Vec<String>,
    pub status: ContainerStatus,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum ContainerStatus {
    Running,
    Stopped,
}

#[derive(Debug)]
pub struct ContainerManager {
    /// Records <id, record>
    records: RwLock<HashMap<String, ContainerRecord>>,
    /// Records <name, id>
    name_to_id: RwLock<HashMap<String, String>>,
    /// Root path to store records.
    root_path: PathBuf,
}

impl ContainerManager {
    pub fn init(root_path: &str) -> Result<Self, SimpleError> {
        let manager = ContainerManager {
            records: RwLock::new(HashMap::new()),
            name_to_id: RwLock::new(HashMap::new()),
            root_path: PathBuf::from(root_path),
        };

        // Load records from disk if exist
        let record_path = manager.root_path.join("records.json");
        if record_path.exists() {
            manager.sync_from_disk()?;
        } else {
            manager.sync_to_disk()?;
        }
        Ok(manager)
    }

    pub fn register(&self, record: ContainerRecord) {
        let mut records = self.records.write().unwrap();
        let mut name_to_id = self.name_to_id.write().unwrap();

        records.insert(record.id.clone(), record.clone());
        name_to_id.insert(record.name.clone(), record.id.clone());
    }

    pub fn deregister(&self, id: &str) {
        let mut records = self.records.write().unwrap();
        let mut name_to_id = self.name_to_id.write().unwrap();

        if let Some(record) = records.remove(id) {
            name_to_id.remove(&record.name);
        }
    }

    pub fn get_record(&self, id: &str) -> Option<ContainerRecord> {
        let records = self.records.read().unwrap();
        records.get(id).cloned()
    }

    pub fn get_record_with_name(&self, name: &str) -> Option<ContainerRecord> {
        let name_to_id = self.name_to_id.read().unwrap();
        let records = self.records.read().unwrap();

        name_to_id.get(name).and_then(|id| records.get(id)).cloned()
    }

    pub fn get_all_records(&self) -> Vec<ContainerRecord> {
        // FIXME: Improve, how to return ref rather than clone
        let records = self.records.read().unwrap();
        records.values().cloned().collect()
    }

    pub fn set_status(&self, id: &str, status: ContainerStatus) {
        let mut records = self.records.write().unwrap();

        records.get_mut(id).unwrap().status = status;
    }

    pub fn set_pid(&self, id: &str, pid: i32) {
        let mut records = self.records.write().unwrap();

        records.get_mut(id).unwrap().pid = pid;
    }

    fn sync_from_disk(&self) -> Result<(), SimpleError> {
        let mut file = File::open(self.root_path.join("records.json"))?;
        let mut contents = String::new();

        file.read_to_string(&mut contents)?;

        let mut records_lock = self.records.write().unwrap();
        *records_lock = serde_json::from_str::<HashMap<String, ContainerRecord>>(&contents)?;

        Ok(())
    }

    fn sync_to_disk(&self) -> Result<(), SimpleError> {
        let records = self.records.read().unwrap();

        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(self.root_path.join("records.json"))?;

        serde_json::to_writer(&mut file, &*records)?;

        Ok(())
    }
}

impl Drop for ContainerManager {
    fn drop(&mut self) {
        self.sync_to_disk()
            .expect("Fatal, failed to sync records to disk");
    }
}

impl ContainerRecord {
    pub fn new(
        name: &str,
        id: &str,
        pid: i32,
        command: &Vec<String>,
        status: ContainerStatus,
    ) -> Self {
        ContainerRecord {
            id: id.to_string(),
            name: name.to_string(),
            pid: pid,
            command: command.clone(),
            status,
        }
    }
}

impl ContainerStatus {
    pub fn is_running(&self) -> bool {
        match self {
            ContainerStatus::Running => true,
            ContainerStatus::Stopped => false,
        }
    }
}
