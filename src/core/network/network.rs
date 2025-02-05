use std::{
    collections::HashMap,
    io::Read,
    net::Ipv4Addr,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use tokio::net::UnixStream;

use crate::core::{Msg, NetCreateArgs};

use super::{bridge::BridgeDriver, ipam::IPAM, NETWORKS};

#[derive(Serialize, Deserialize, Debug)]
pub struct Network {
    pub name: String,
    pub cidr: String,
    #[serde(serialize_with = "serialize_ipv4")]
    #[serde(deserialize_with = "deserialize_ipv4")]
    pub gateway: Ipv4Addr,
    pub driver: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Networks {
    pub ipam: IPAM,
    pub networks: HashMap<String, Network>,

    path: PathBuf,
}

impl Networks {
    // TODO: Improve info storage.
    pub fn load(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let path = path.as_ref().to_path_buf();

        if path.exists() {
            let mut file = std::fs::File::open(&path)?;
            let mut contents = Vec::new();
            file.read_to_end(&mut contents)?;

            let mut networks: Networks = bincode::deserialize(&contents)?;
            networks.path = path;

            Ok(networks)
        } else {
            if let Some(parent_dir) = path.parent() {
                std::fs::create_dir_all(parent_dir)?;
            }

            Ok(Networks {
                ipam: IPAM::empty(),
                networks: HashMap::new(),
                path,
            })
        }
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let contents = bincode::serialize(self)?;
        std::fs::write(&self.path, contents)?;

        Ok(())
    }
}

fn serialize_ipv4<S>(ip: &Ipv4Addr, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    ip.to_bits().serialize(serializer)
}

fn deserialize_ipv4<'de, D>(deserializer: D) -> Result<Ipv4Addr, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let bits = u32::deserialize(deserializer)?;
    Ok(Ipv4Addr::from_bits(bits))
}

const BRIDGEDRIVER: BridgeDriver = BridgeDriver {};

pub async fn create_network(create_args: NetCreateArgs, mut stream: UnixStream) {
    let mut networks_locked = NETWORKS.get().unwrap().lock().await;

    if networks_locked.networks.contains_key(&create_args.name) {
        log::error!(
            "Failed to create network, network already exists: {}",
            create_args.name
        );
        let _ = Msg::Err(format!(
            "Failed to create network, network already exists: {}",
            create_args.name
        ))
        .send_to(&mut stream)
        .await;

        return;
    }

    // Currently only Bridge supported.
    if create_args.driver != "bridge" {
        log::error!(
            "Failed to create network, invalid driver: {}",
            create_args.driver
        );
        let _ = Msg::Err(format!(
            "Failed to create network, invalid driver: {}",
            create_args.driver
        ))
        .send_to(&mut stream)
        .await;

        return;
    }

    if let Err(e) = networks_locked.ipam.add_subnet(&create_args.subnet) {
        log::error!("Failed to create network, add subnet fail: {e}");
        let _ = Msg::Err(format!("Failed to create network, add subnet fail: {e}"))
            .send_to(&mut stream)
            .await;

        return;
    }

    let gateway = match networks_locked.ipam.allocate_gateway(&create_args.subnet) {
        Ok(ip) => ip,
        Err(e) => {
            log::error!("Failed to create network, allocate gateway fail: {e}");
            let _ = Msg::Err(format!(
                "Failed to create network, allocate gateway fail: {e}"
            ))
            .send_to(&mut stream)
            .await;

            return;
        }
    };

    let network = match BRIDGEDRIVER
        .create_network(&create_args.name, &create_args.subnet, gateway)
        .await
    {
        Ok(net) => net,
        Err(e) => {
            log::error!("Failed to create network, driver error: {e}");
            let _ = Msg::Err(format!("Failed to create network, driver error: {e}"))
                .send_to(&mut stream)
                .await;

            let _ = networks_locked
                .ipam
                .release_ip(&create_args.subnet, gateway);

            return;
        }
    };

    let _ = Msg::OkContent(format!("Network {} created", create_args.name))
        .send_to(&mut stream)
        .await;

    networks_locked.networks.insert(create_args.name, network);
    let _ = networks_locked.save();
}
