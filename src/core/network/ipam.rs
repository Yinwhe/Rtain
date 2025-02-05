use std::{collections::HashMap, net::Ipv4Addr};

use bitvec::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct IPAM {
    #[serde(serialize_with = "serialize_subnets")]
    #[serde(deserialize_with = "deserialize_subnets")]
    subnets: HashMap<String, BitVec<u8>>,
}

impl IPAM {
    pub fn empty() -> Self {
        Self {
            subnets: HashMap::new(),
        }
    }

    pub fn add_subnet(&mut self, cidr: &str) -> anyhow::Result<()> {
        if self.subnets.contains_key(cidr) {
            return Err(anyhow::anyhow!("Subnet already exists"));
        }

        let (_, prefix_len) = Self::parse_cidr(cidr)?;
        let total_ips = 2u32.pow(32 - prefix_len) - 2;

        let mut bitmap = BitVec::new();
        bitmap.resize(total_ips as usize, false);
        self.subnets.insert(cidr.to_string(), bitmap);

        Ok(())
    }

    pub fn allocate_ip(&mut self, cidr: &str) -> anyhow::Result<Ipv4Addr> {
        let bitmap = self
            .subnets
            .get_mut(cidr)
            .ok_or(anyhow::anyhow!("Subnet not found"))?;

        if let Some(pos) = bitmap.first_zero() {
            bitmap.set(pos, true);
            Self::calculate_ip(cidr, pos as u32 + 1)
        } else {
            Err(anyhow::anyhow!("No available IP"))
        }
    }

    pub fn release_ip(&mut self, cidr: &str, ip: Ipv4Addr) -> anyhow::Result<()> {
        let (subnet_ip, prefix_len) = Self::parse_cidr(cidr)?;
        let pos = Self::ip_to_index(subnet_ip, ip, prefix_len)?;

        let bitmap = self
            .subnets
            .get_mut(cidr)
            .ok_or(anyhow::anyhow!("Subnet not found"))?;

        if pos >= bitmap.len() {
            return Err(anyhow::anyhow!("IP out of range"));
        }
        if !bitmap[pos] {
            return Err(anyhow::anyhow!("IP not allocated"));
        }
        bitmap.set(pos, false);

        Ok(())
    }

    pub fn allocate_gateway(&mut self, cidr: &str) -> anyhow::Result<Ipv4Addr> {
        self.allocate_specific_ip(cidr, 1)
    }

    fn allocate_specific_ip(&mut self, cidr: &str, index: u32) -> anyhow::Result<Ipv4Addr> {
        let bitmap = self
            .subnets
            .get_mut(cidr)
            .ok_or(anyhow::anyhow!("Subnet not found"))?;

        if index >= bitmap.len() as u32 {
            return Err(anyhow::anyhow!("IP out of range"));
        }

        if bitmap[index as usize] {
            return Err(anyhow::anyhow!("IP already allocated"));
        }

        bitmap.set(index as usize, true);
        Self::calculate_ip(cidr, index + 1)
    }

    fn parse_cidr(cidr: &str) -> anyhow::Result<(Ipv4Addr, u32)> {
        let (ip_str, len_str) = cidr
            .split_once('/')
            .ok_or(anyhow::anyhow!("Invalid CIDR"))?;

        let ip = ip_str.parse::<Ipv4Addr>()?;
        let len = len_str.parse::<u32>()?;

        if len > 32 {
            return Err(anyhow::anyhow!("Invalid prefix length"));
        }

        Ok((ip, len))
    }

    fn calculate_ip(cidr: &str, index: u32) -> anyhow::Result<Ipv4Addr> {
        let (subnet_ip, prefix_len) = Self::parse_cidr(cidr)?;

        let subnet = u32::from(subnet_ip);
        let mask = !((1 << (32 - prefix_len)) - 1);
        let host_part = (subnet & mask) + index;

        Ok(Ipv4Addr::from(host_part))
    }

    fn ip_to_index(
        subnet_ip: Ipv4Addr,
        target_ip: Ipv4Addr,
        prefix_len: u32,
    ) -> anyhow::Result<usize> {
        let subnet = u32::from(subnet_ip);
        let ip = u32::from(target_ip);
        let mask = !((1 << (32 - prefix_len)) - 1);

        if (subnet & mask) != (ip & mask) {
            return Err(anyhow::anyhow!("IP not in subnet"));
        }

        Ok((ip - subnet - 1) as usize)
    }
}

fn serialize_subnets<S>(
    subnets: &HashMap<String, BitVec<u8>>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    let mut map = HashMap::new();
    for (cidr, bitmap) in subnets {
        let bytes = bitmap.as_raw_slice().to_vec();
        map.insert(cidr, bytes);
    }
    map.serialize(serializer)
}

fn deserialize_subnets<'de, D>(deserializer: D) -> Result<HashMap<String, BitVec<u8>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let map: HashMap<String, Vec<u8>> = HashMap::deserialize(deserializer)?;
    let mut subnets = HashMap::new();
    for (cidr, bytes) in map {
        let bitvec = BitVec::from_vec(bytes);
        subnets.insert(cidr, bitvec);
    }
    Ok(subnets)
}
