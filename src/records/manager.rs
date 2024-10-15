use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
};

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ContainerRecord {
    pub id: String,
    pub name: String,
    pub pid: String,
    pub command: String,
    pub status: ContainerStatus,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum ContainerStatus {
    Running,
    Stopped,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ContainerManager {
    records: HashSet<String>,
    root_path: PathBuf,

    #[serde(skip)]
    loaded_datas: HashMap<String, ContainerRecord>,
}

impl ContainerManager {
    pub fn init() -> Result<Self, Box<dyn std::error::Error>> {
        let root_path = PathBuf::from("/tmp/rtain");
        if !root_path.exists() {
            std::fs::create_dir_all(&root_path)?;
        }

        let manager_path = root_path.join("manager.json");
        let mut manager = if manager_path.exists() {
            Self::load()
        } else {
            let manager = ContainerManager {
                records: HashSet::new(),
                root_path,

                loaded_datas: HashMap::new(),
            };

            manager.save()?;

            Ok(manager)
        };

        if let Ok(manager) = &mut manager {
            // For current simple impl, we only need to sync once at the start.
            manager.sync()?;
        }

        manager
    }

    pub fn register(&mut self, record: &ContainerRecord) -> Result<(), Box<dyn std::error::Error>> {
        self.records.insert(record.id.clone());
        self.loaded_datas.insert(record.id.clone(), record.clone());

        record.save(&self.root_path)?;

        self.save()
    }

    pub fn deregister(&mut self, id: &str) -> Result<(), Box<dyn std::error::Error>> {
        self.records.remove(id);
        self.loaded_datas.remove(id);

        let record_path = self.root_path.join(format!("{}.json", id));
        std::fs::remove_file(record_path)?;

        self.save()
    }

    pub fn update(
        &mut self,
        id: &str,
        pid: &str,
        status: ContainerStatus,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let cr = match self.loaded_datas.get_mut(id) {
            Some(cr) => cr,
            None => return Err(format!("No container found with id: {}", id).into()),
        };

        cr.pid = pid.to_string();
        cr.status = status;
        cr.save(&self.root_path)?;

        Ok(())
    }

    pub fn sync(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let not_loaded: Vec<String> = self
            .records
            .iter()
            .filter(|id| !self.loaded_datas.contains_key(*id))
            .cloned()
            .collect();

        for to_load in not_loaded {
            self.load_record(&to_load)?;
        }

        Ok(())
    }

    pub fn all_containers(&self) -> Result<Vec<&ContainerRecord>, Box<dyn std::error::Error>> {
        Ok(self.loaded_datas.values().collect())
    }

    pub fn container_with_name(
        &self,
        name: &str,
    ) -> Result<&ContainerRecord, Box<dyn std::error::Error>> {
        let container = self.loaded_datas.iter().find(|(_, c)| c.name == name);
        match container {
            Some((_, c)) => Ok(c),
            None => Err(format!("No container found with name: {}", name).into()),
        }
    }

    fn load_record(&mut self, id: &str) -> Result<(), Box<dyn std::error::Error>> {
        let path = self.root_path.join(format!("{}.json", id));
        let record = ContainerRecord::load(&path)?;

        self.loaded_datas.insert(id.to_string(), record);

        Ok(())
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
    pub fn new(name: &str, id: &str, pid: &str, command: &str, status: ContainerStatus) -> Self {
        ContainerRecord {
            id: id.to_string(),
            name: name.to_string(),
            pid: pid.to_string(),
            command: command.to_string(),
            status,
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
        record_path: &PathBuf,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let record = std::fs::read_to_string(record_path)?;
        let record: ContainerRecord = serde_json::from_str(&record)?;

        Ok(record)
    }
}

impl ContainerStatus {
    pub fn is_running(&self) -> bool {
        match self {
            ContainerStatus::Running => true,
            _ => false,
        }
    }
}
