//! No-op implementations of all network traits for development and testing.
//!
//! Every method logs via `tracing::info!` and returns a harmless default.
//! Used when running with `--mock-network`.

use async_trait::async_trait;

use crate::firewall::FirewallManager;
use crate::policy_router::PolicyRouter;
use crate::tunnel_interface::{CreateTunnelParams, TunnelInterface, TunnelStats};

// ---------------------------------------------------------------------------
// TunnelInterface
// ---------------------------------------------------------------------------

/// No-op tunnel interface for development and testing.
///
/// Logs all operations via `tracing::info!` without touching the kernel.
#[derive(Debug)]
pub struct NoopTunnelInterface;

#[async_trait]
impl TunnelInterface for NoopTunnelInterface {
    async fn create(&self, params: CreateTunnelParams) -> anyhow::Result<()> {
        tracing::info!(interface = %params.interface_name, "noop: create tunnel");
        Ok(())
    }

    async fn bring_up(&self, interface_name: &str) -> anyhow::Result<()> {
        tracing::info!(interface = %interface_name, "noop: bring up");
        Ok(())
    }

    async fn tear_down(&self, interface_name: &str) -> anyhow::Result<()> {
        tracing::info!(interface = %interface_name, "noop: tear down");
        Ok(())
    }

    async fn remove(&self, interface_name: &str) -> anyhow::Result<()> {
        tracing::info!(interface = %interface_name, "noop: remove tunnel");
        Ok(())
    }

    async fn get_stats(&self, interface_name: &str) -> anyhow::Result<Option<TunnelStats>> {
        tracing::info!(interface = %interface_name, "noop: get stats");
        Ok(None)
    }

    async fn list(&self) -> anyhow::Result<Vec<String>> {
        tracing::info!("noop: list tunnels");
        Ok(vec![])
    }
}

// ---------------------------------------------------------------------------
// PolicyRouter
// ---------------------------------------------------------------------------

/// No-op policy router for development and testing.
///
/// Logs all operations via `tracing::info!` without touching the kernel.
#[derive(Debug)]
pub struct NoopPolicyRouter;

#[async_trait]
impl PolicyRouter for NoopPolicyRouter {
    async fn enable_ip_forwarding(&self) -> anyhow::Result<()> {
        tracing::info!("noop: enable ip forwarding");
        Ok(())
    }

    async fn add_route_table(&self, interface: &str, table: u32) -> anyhow::Result<()> {
        tracing::info!(interface, table, "noop: add route table");
        Ok(())
    }

    async fn remove_route_table(&self, table: u32) -> anyhow::Result<()> {
        tracing::info!(table, "noop: remove route table");
        Ok(())
    }

    async fn has_route_table(&self, table: u32) -> anyhow::Result<bool> {
        tracing::info!(table, "noop: has route table");
        Ok(false)
    }

    async fn add_ip_rule(&self, src_ip: &str, table: u32) -> anyhow::Result<()> {
        tracing::info!(src_ip, table, "noop: add ip rule");
        Ok(())
    }

    async fn remove_ip_rule(&self, src_ip: &str, table: u32) -> anyhow::Result<()> {
        tracing::info!(src_ip, table, "noop: remove ip rule");
        Ok(())
    }

    async fn list_wardnet_rules(&self) -> anyhow::Result<Vec<(String, u32)>> {
        tracing::info!("noop: list wardnet rules");
        Ok(vec![])
    }

    async fn flush_conntrack(&self, src_ip: &str) -> anyhow::Result<()> {
        tracing::info!(src_ip, "noop: flush conntrack");
        Ok(())
    }

    async fn flush_route_cache(&self) -> anyhow::Result<()> {
        tracing::info!("noop: flush route cache");
        Ok(())
    }

    async fn check_tools_available(&self) -> anyhow::Result<()> {
        tracing::info!("noop: check tools available");
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// FirewallManager
// ---------------------------------------------------------------------------

/// No-op firewall manager for development and testing.
///
/// Logs all operations via `tracing::info!` without touching the kernel.
#[derive(Debug)]
pub struct NoopFirewallManager;

#[async_trait]
impl FirewallManager for NoopFirewallManager {
    async fn init_wardnet_table(&self) -> anyhow::Result<()> {
        tracing::info!("noop: init wardnet table");
        Ok(())
    }

    async fn flush_wardnet_table(&self) -> anyhow::Result<()> {
        tracing::info!("noop: flush wardnet table");
        Ok(())
    }

    async fn add_masquerade(&self, interface: &str) -> anyhow::Result<()> {
        tracing::info!(interface, "noop: add masquerade");
        Ok(())
    }

    async fn remove_masquerade(&self, interface: &str) -> anyhow::Result<()> {
        tracing::info!(interface, "noop: remove masquerade");
        Ok(())
    }

    async fn add_dns_redirect(&self, device_ip: &str, dns_ip: &str) -> anyhow::Result<()> {
        tracing::info!(device_ip, dns_ip, "noop: add dns redirect");
        Ok(())
    }

    async fn remove_dns_redirect(&self, device_ip: &str) -> anyhow::Result<()> {
        tracing::info!(device_ip, "noop: remove dns redirect");
        Ok(())
    }

    async fn check_tools_available(&self) -> anyhow::Result<()> {
        tracing::info!("noop: check tools available");
        Ok(())
    }

    async fn destroy_wardnet_table(&self) -> anyhow::Result<()> {
        tracing::info!("noop: destroy wardnet table");
        Ok(())
    }
}
