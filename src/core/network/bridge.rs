use std::{
    net::Ipv4Addr,
    os::fd::{AsFd, AsRawFd},
};

use anyhow::Context;
use futures::TryStreamExt;
use netlink_packet_route::link::LinkMessage;

use super::{network::Network, Endpoint};

pub struct BridgeDriver {}

impl BridgeDriver {
    pub async fn create_network(
        &self,
        name: &str,
        cidr: &str,
        gateway: Ipv4Addr,
    ) -> anyhow::Result<Network> {
        self.create_bridge(name).await?;

        let prefix_len = cidr
            .split('/')
            .nth(1)
            .ok_or(anyhow::anyhow!("Invalid CIDR"))?
            .parse::<u8>()?;

        if let Err(e) = self.set_bridge_ip(name, gateway, prefix_len).await {
            let _ = self.delete_bridge(name).await;
            return Err(e);
        }

        if let Err(e) = self.set_link_up(name).await {
            let _ = self.delete_bridge(name).await;
            return Err(e);
        }

        if let Err(e) = self.set_basic_iptables(name, cidr).await {
            let _ = self.delete_bridge(name).await;
            return Err(e);
        }

        Ok(Network {
            name: name.to_string(),
            cidr: cidr.to_string(),
            gateway: gateway,
            driver: "bridge".to_string(),
        })
    }

    pub async fn delete_network(&self, network: &Network) -> anyhow::Result<()> {
        self.delete_bridge(&network.name).await
    }

    pub async fn connect(
        &self,
        network: &Network,
        endpoint: &Endpoint,
    ) -> anyhow::Result<Ipv4Addr> {
        // Create veth pair
        self.create_veth_pair(&endpoint.veth_host, &endpoint.veth_peer)
            .await
            .context("Failed to create veth pair")?;

        // Add host veth to bridge
        self.add_to_bridge(&endpoint.veth_host, &network.name)
            .await
            .context("Failed to add veth to bridge")?;

        // Set host veth up
        self.set_link_up(&endpoint.veth_host)
            .await
            .context("Failed to set host veth up")?;

        // Move peer veth to container netns
        // Note: container_id should be the PID of the container process
        let netns_path = format!("/proc/{}/ns/net", endpoint.container_id);
        self.move_to_netns(&endpoint.veth_peer, &netns_path)
            .await
            .context("Failed to move veth to container netns")?;

        Ok(endpoint.container_ip)
    }

    async fn create_bridge(&self, name: &str) -> anyhow::Result<()> {
        let (connection, handle, _) = rtnetlink::new_connection()?;
        tokio::spawn(connection);

        handle
            .link()
            .add()
            .bridge(name.to_string())
            .execute()
            .await?;
        Ok(())
    }

    async fn delete_bridge(&self, name: &str) -> anyhow::Result<()> {
        let (connection, handle, _) = rtnetlink::new_connection()?;
        tokio::spawn(connection);

        let link = self.get_link_by_name(name, &handle).await?;
        handle.link().del(link.header.index).execute().await?;

        Ok(())
    }

    async fn create_veth_pair(&self, host_veth: &str, peer_veth: &str) -> anyhow::Result<()> {
        let (connection, handle, _) = rtnetlink::new_connection()?;
        tokio::spawn(connection);

        handle
            .link()
            .add()
            .veth(host_veth.to_string(), peer_veth.to_string())
            .execute()
            .await?;

        Ok(())
    }

    async fn set_bridge_ip(
        &self,
        bridge: &str,
        ip: Ipv4Addr,
        prefix_len: u8,
    ) -> anyhow::Result<()> {
        let (connection, handle, _) = rtnetlink::new_connection()?;
        tokio::spawn(connection);

        let bridge_link = self.get_link_by_name(bridge, &handle).await?;
        handle
            .address()
            .add(bridge_link.header.index, ip.into(), prefix_len)
            .execute()
            .await?;

        Ok(())
    }

    async fn add_to_bridge(&self, iface: &str, bridge: &str) -> anyhow::Result<()> {
        let (connection, handle, _) = rtnetlink::new_connection()?;
        tokio::spawn(connection);

        let bridge_link = self.get_link_by_name(bridge, &handle).await?;
        let iface_link = self.get_link_by_name(iface, &handle).await?;

        handle
            .link()
            .set(iface_link.header.index)
            .controller(bridge_link.header.index)
            .execute()
            .await?;

        Ok(())
    }

    async fn move_to_netns(&self, iface: &str, netns: &str) -> anyhow::Result<()> {
        let (connection, handle, _) = rtnetlink::new_connection()?;
        tokio::spawn(connection);

        let iface_link = self.get_link_by_name(iface, &handle).await?;
        let netns_file = tokio::fs::File::open(netns).await?;

        handle
            .link()
            .set(iface_link.header.index)
            .setns_by_fd(netns_file.as_fd().as_raw_fd())
            .execute()
            .await?;

        Ok(())
    }

    async fn set_link_up(&self, name: &str) -> anyhow::Result<()> {
        let (connection, handle, _) = rtnetlink::new_connection()?;
        tokio::spawn(connection);

        let link = self.get_link_by_name(name, &handle).await?;
        handle.link().set(link.header.index).up().execute().await?;

        Ok(())
    }

    async fn set_basic_iptables(&self, name: &str, cidr: &str) -> anyhow::Result<()> {
        async fn exec_iptables(args: &[&str]) -> anyhow::Result<()> {
            tokio::process::Command::new("iptables")
                .args(args)
                .status()
                .await?
                .success()
                .then_some(())
                .context("`iptables` exited with non-zero status")
        }

        exec_iptables(&["-A", "FORWARD", "-i", &name, "-j", "ACCEPT"])
            .await
            .context("Failed to set FORWARD rule")?;

        exec_iptables(&[
            "-t",
            "nat",
            "-A",
            "POSTROUTING",
            "-s",
            &cidr.to_string(),
            "-j",
            "MASQUERADE",
        ])
        .await
        .context("Failed to set MASQUERADE rule")?;

        Ok(())
    }

    async fn get_link_by_name(
        &self,
        name: &str,
        handle: &rtnetlink::Handle,
    ) -> anyhow::Result<LinkMessage> {
        handle
            .link()
            .get()
            .match_name(name.to_string())
            .execute()
            .try_next()
            .await?
            .ok_or(anyhow::anyhow!("Link not found"))
    }
}
