use std::{collections::HashSet, path::PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct ContainerRecord {
    pub id: String,
    pub name: String,
    pub pid: i32,
    pub command: String,
    pub status: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ContainerManager {
    records: HashSet<String>,
    root_path: PathBuf,
}

impl ContainerManager {
    pub fn init() -> Result<Self, Box<dyn std::error::Error>> {
        let root_path = PathBuf::from("/tmp/rtain");
        let manager_path = root_path.join("manager.json");

        if manager_path.exists() {
            Self::load()
        } else {
            let manager = ContainerManager {
                records: HashSet::new(),
                root_path,
            };

            manager.save()?;

            Ok(manager)
        }
    }

    pub fn register(&mut self, record: &ContainerRecord) -> Result<(), Box<dyn std::error::Error>> {
        self.records.insert(record.id.clone());

        record.save(&self.root_path)?;
        self.save()
    }

    pub fn deregister(&mut self, id: &str) -> Result<(), Box<dyn std::error::Error>> {
        self.records.remove(id);

        let record_path = self.root_path.join(format!("{}.json", id));
        std::fs::remove_file(record_path)?;

        self.save()
    }

    fn load() -> Result<Self, Box<dyn std::error::Error>> {
        let manager_path = PathBuf::from("/tmp/rtain/manager.json");

        let manager = std::fs::read_to_string(manager_path)?;
        let manager: ContainerManager = serde_json::from_str(&manager)?;

        Ok(manager)
    }

    fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
        let manager_path = PathBuf::from("/tmp/rtain/manager.json");

        let manager = serde_json::to_string(self)?;
        std::fs::write(manager_path, manager)?;

        Ok(())
    }
}

impl Drop for ContainerManager {
    fn drop(&mut self) {
        self.save().unwrap();
    }
}

impl ContainerRecord {
    pub fn new(name: &str, id: &str, pid: i32, command: &str) -> Self {
        ContainerRecord {
            id: id.to_string(),
            name: name.to_string(),
            pid,
            command: command.to_string(),
            status: "unimplemented".to_string(),
        }
    }

    pub(in crate::records) fn save(
        &self,
        root_path: &PathBuf,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let record_path = root_path.join(format!("{}.json", self.id));

        let record = serde_json::to_string(self)?;
        std::fs::write(record_path, record)?;

        Ok(())
    }

    pub(in crate::records) fn load(
        root_path: &PathBuf,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let record_path = root_path.join("record.json");

        let record = std::fs::read_to_string(record_path)?;
        let record: ContainerRecord = serde_json::from_str(&record)?;

        Ok(record)
    }
}
