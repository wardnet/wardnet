//! No-op [`InboundWgInterface`] implementation for the mock server (issue #809).

use async_trait::async_trait;
use wardnetd_services::inbound_wg::interface::{
    InboundWgInterface, InboundWgPeerConfig, InboundWgPeerStats, InboundWgServerConfig,
};

/// An inbound `WireGuard` server interface that performs no kernel operations.
///
/// Every method logs the call and returns `Ok` (or an empty stats list). Used
/// by the mock server to satisfy the trait without touching kernel `WireGuard`
/// state.
#[derive(Debug, Default, Clone)]
pub struct NoopInboundWgInterface;

#[async_trait]
impl InboundWgInterface for NoopInboundWgInterface {
    async fn ensure_server(&self, config: InboundWgServerConfig) -> anyhow::Result<()> {
        tracing::debug!(
            interface = %config.interface_name,
            listen_port = config.listen_port,
            "mock inbound-wg ensure_server: interface={iface}",
            iface = config.interface_name,
        );
        Ok(())
    }

    async fn tear_down_server(&self, interface_name: &str) -> anyhow::Result<()> {
        tracing::debug!(
            interface = interface_name,
            "mock inbound-wg tear_down_server: interface={interface_name}",
        );
        Ok(())
    }

    async fn add_peer(
        &self,
        interface_name: &str,
        _peer: InboundWgPeerConfig,
    ) -> anyhow::Result<()> {
        tracing::debug!(
            interface = interface_name,
            "mock inbound-wg add_peer: interface={interface_name}",
        );
        Ok(())
    }

    async fn remove_peer(&self, interface_name: &str, _public_key: [u8; 32]) -> anyhow::Result<()> {
        tracing::debug!(
            interface = interface_name,
            "mock inbound-wg remove_peer: interface={interface_name}",
        );
        Ok(())
    }

    async fn peer_stats(&self, interface_name: &str) -> anyhow::Result<Vec<InboundWgPeerStats>> {
        tracing::debug!(
            interface = interface_name,
            "mock inbound-wg peer_stats: interface={interface_name}",
        );
        Ok(Vec::new())
    }
}
