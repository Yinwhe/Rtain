use std::net::Ipv4Addr;

use tokio::sync::{Mutex, OnceCell};

mod bridge;
mod ipam;
mod network;

struct Endpoint {
    pub container_id: String,
    pub veth_host: String,
    pub veth_peer: String,
    pub container_ip: Ipv4Addr,
}

pub static NETWORKS: OnceCell<Mutex<Networks>> = OnceCell::const_new();
pub use network::*;
