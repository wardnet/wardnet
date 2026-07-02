//! No-op [`FirewallManager`] and [`PolicyRouter`] implementations for the
//! mock server.

use async_trait::async_trait;
use wardnetd_services::routing::firewall::ZoneRules;
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

    async fn cleanup_legacy_dns_redirects(&self) -> anyhow::Result<()> {
        tracing::debug!("mock firewall cleanup_legacy_dns_redirects");
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

    async fn apply_zone_rules(
        &self,
        device_ip: &str,
        rules: ZoneRules,
        lan_interface: &str,
    ) -> anyhow::Result<()> {
        tracing::debug!(
            device_ip,
            allow_direct = rules.allow_direct,
            allow_tunnel = rules.allow_tunnel,
            admin_ui_reachable = rules.admin_ui_reachable,
            lan_interface,
            "mock firewall apply_zone_rules: device_ip={device_ip}",
        );
        Ok(())
    }

    async fn remove_zone_rules(&self, device_ip: &str) -> anyhow::Result<()> {
        tracing::debug!(
            device_ip,
            "mock firewall remove_zone_rules: device_ip={device_ip}",
        );
        Ok(())
    }

    async fn list_zone_rule_ips(&self) -> anyhow::Result<Vec<String>> {
        Ok(Vec::new())
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

    async fn add_interface_alias(
        &self,
        interface: &str,
        ip: &str,
        prefix: u8,
    ) -> anyhow::Result<()> {
        tracing::debug!(
            interface,
            ip,
            prefix,
            "mock policy add_interface_alias: {ip}/{prefix} dev {interface}",
        );
        Ok(())
    }

    async fn remove_interface_alias(
        &self,
        interface: &str,
        ip: &str,
        prefix: u8,
    ) -> anyhow::Result<()> {
        tracing::debug!(
            interface,
            ip,
            prefix,
            "mock policy remove_interface_alias: {ip}/{prefix} dev {interface}",
        );
        Ok(())
    }

    async fn list_interface_aliases(&self, interface: &str) -> anyhow::Result<Vec<(String, u8)>> {
        tracing::debug!(interface, "mock policy list_interface_aliases");
        Ok(Vec::new())
    }

    async fn set_proxy_arp(&self, interface: &str, enabled: bool) -> anyhow::Result<()> {
        tracing::debug!(
            interface,
            enabled,
            "mock policy set_proxy_arp: dev {interface} = {enabled}",
        );
        Ok(())
    }

    async fn add_host_route(&self, ip: &str, interface: &str) -> anyhow::Result<()> {
        tracing::debug!(
            ip,
            interface,
            "mock policy add_host_route: {ip}/32 dev {interface}",
        );
        Ok(())
    }

    async fn remove_host_route(&self, ip: &str, interface: &str) -> anyhow::Result<()> {
        tracing::debug!(
            ip,
            interface,
            "mock policy remove_host_route: {ip}/32 dev {interface}",
        );
        Ok(())
    }
}
