//! No-op [`TunnelInterface`] implementation for the mock server.

use async_trait::async_trait;
use wardnetd_services::tunnel::{CreateTunnelParams, TunnelInterface, TunnelStats};

/// A tunnel interface that performs no kernel operations.
///
/// Every method logs the call and returns `Ok(())` (or an empty list / default
/// stats). Used by the mock server to satisfy the trait without touching
/// kernel `WireGuard` state.
#[derive(Debug, Default, Clone)]
pub struct NoopTunnelInterface;

#[async_trait]
impl TunnelInterface for NoopTunnelInterface {
    async fn create(&self, params: CreateTunnelParams) -> anyhow::Result<()> {
        tracing::debug!(
            interface = %params.interface_name,
            "mock tunnel create: interface={iface}",
            iface = params.interface_name,
        );
        Ok(())
    }

    async fn bring_up(&self, interface_name: &str) -> anyhow::Result<()> {
        tracing::debug!(
            interface = interface_name,
            "mock tunnel bring_up: interface={interface_name}",
        );
        Ok(())
    }

    async fn tear_down(&self, interface_name: &str) -> anyhow::Result<()> {
        tracing::debug!(
            interface = interface_name,
            "mock tunnel tear_down: interface={interface_name}",
        );
        Ok(())
    }

    async fn remove(&self, interface_name: &str) -> anyhow::Result<()> {
        tracing::debug!(
            interface = interface_name,
            "mock tunnel remove: interface={interface_name}",
        );
        Ok(())
    }

    async fn get_stats(&self, interface_name: &str) -> anyhow::Result<Option<TunnelStats>> {
        tracing::debug!(
            interface = interface_name,
            "mock tunnel get_stats: interface={interface_name}",
        );
        Ok(Some(TunnelStats {
            bytes_tx: 0,
            bytes_rx: 0,
            last_handshake: None,
        }))
    }

    async fn list(&self) -> anyhow::Result<Vec<String>> {
        Ok(Vec::new())
    }
}
