//! No-op [`FirewallManager`] and [`PolicyRouter`] implementations for the
//! mock server.

use async_trait::async_trait;
use wardnetd_services::routing::{FirewallManager, PolicyRouter};

/// A firewall manager that performs no nftables operations.
#[derive(Debug, Default, Clone)]
pub struct NoopFirewallManager;

#[async_trait]
impl FirewallManager for NoopFirewallManager {
    async fn init_wardnet_table(&self) -> anyhow::Result<()> {
        tracing::debug!("mock firewall init_wardnet_table");
        Ok(())
    }

    async fn flush_wardnet_table(&self) -> anyhow::Result<()> {
        tracing::debug!("mock firewall flush_wardnet_table");
        Ok(())
    }

    async fn add_masquerade(&self, interface: &str) -> anyhow::Result<()> {
        tracing::debug!(
            interface,
            "mock firewall add_masquerade: interface={interface}",
        );
        Ok(())
    }

    async fn remove_masquerade(&self, interface: &str) -> anyhow::Result<()> {
        tracing::debug!(
            interface,
            "mock firewall remove_masquerade: interface={interface}",
        );
        Ok(())
    }

    async fn add_dns_redirect(&self, device_ip: &str, dns_ip: &str) -> anyhow::Result<()> {
        tracing::debug!(
            device_ip,
            dns_ip,
            "mock firewall add_dns_redirect: device_ip={device_ip}, dns_ip={dns_ip}",
        );
        Ok(())
    }

    async fn remove_dns_redirect(&self, device_ip: &str) -> anyhow::Result<()> {
        tracing::debug!(
            device_ip,
            "mock firewall remove_dns_redirect: device_ip={device_ip}",
        );
        Ok(())
    }

    async fn add_tcp_reset_reject(&self, device_ip: &str) -> anyhow::Result<()> {
        tracing::debug!(
            device_ip,
            "mock firewall add_tcp_reset_reject: device_ip={device_ip}",
        );
        Ok(())
    }

    async fn remove_tcp_reset_reject(&self, device_ip: &str) -> anyhow::Result<()> {
        tracing::debug!(
            device_ip,
            "mock firewall remove_tcp_reset_reject: device_ip={device_ip}",
        );
        Ok(())
    }

    async fn check_tools_available(&self) -> anyhow::Result<()> {
        Ok(())
    }

    async fn destroy_wardnet_table(&self) -> anyhow::Result<()> {
        tracing::debug!("mock firewall destroy_wardnet_table");
        Ok(())
    }
}

/// A policy router that performs no `ip rule` / `ip route` operations.
#[derive(Debug, Default, Clone)]
pub struct NoopPolicyRouter;

#[async_trait]
impl PolicyRouter for NoopPolicyRouter {
    async fn enable_ip_forwarding(&self) -> anyhow::Result<()> {
        tracing::debug!("mock policy enable_ip_forwarding");
        Ok(())
    }

    async fn add_route_table(&self, interface: &str, table: u32) -> anyhow::Result<()> {
        tracing::debug!(
            interface,
            table,
            "mock policy add_route_table: interface={interface}, table={table}",
        );
        Ok(())
    }

    async fn remove_route_table(&self, table: u32) -> anyhow::Result<()> {
        tracing::debug!(table, "mock policy remove_route_table: table={table}");
        Ok(())
    }

    async fn has_route_table(&self, table: u32) -> anyhow::Result<bool> {
        tracing::debug!(table, "mock policy has_route_table: table={table}");
        Ok(true)
    }

    async fn add_ip_rule(&self, src_ip: &str, table: u32) -> anyhow::Result<()> {
        tracing::debug!(
            src_ip,
            table,
            "mock policy add_ip_rule: src_ip={src_ip}, table={table}",
        );
        Ok(())
    }

    async fn remove_ip_rule(&self, src_ip: &str, table: u32) -> anyhow::Result<()> {
        tracing::debug!(
            src_ip,
            table,
            "mock policy remove_ip_rule: src_ip={src_ip}, table={table}",
        );
        Ok(())
    }

    async fn list_wardnet_rules(&self) -> anyhow::Result<Vec<(String, u32)>> {
        Ok(Vec::new())
    }

    async fn flush_conntrack(&self, src_ip: &str) -> anyhow::Result<()> {
        tracing::debug!(src_ip, "mock policy flush_conntrack: src_ip={src_ip}",);
        Ok(())
    }

    async fn flush_route_cache(&self) -> anyhow::Result<()> {
        tracing::debug!("mock policy flush_route_cache");
        Ok(())
    }

    async fn check_tools_available(&self) -> anyhow::Result<()> {
        Ok(())
    }
}
