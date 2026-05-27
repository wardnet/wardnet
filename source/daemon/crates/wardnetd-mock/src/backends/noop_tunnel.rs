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
        // Derive a per-interface offset from the interface name so each
        // tunnel shows distinct totals.
        let hash: u64 = interface_name.bytes().fold(2_166_136_261_u64, |acc, b| {
            acc.wrapping_mul(16_777_619).wrapping_add(u64::from(b))
        });
        let now_secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        // Simulate ~100 KB/s tx and ~300 KB/s rx (matches seed generator
        // values). Returned as cumulative counters so record_byte_deltas
        // sees a non-zero delta on every poll.
        let bytes_tx = now_secs * 100_000 + (hash & 0x00FF_FFFF);
        let bytes_rx = now_secs * 300_000 + ((hash >> 8) & 0x00FF_FFFF);
        Ok(Some(TunnelStats {
            bytes_tx,
            bytes_rx,
            // Return a current handshake so the tunnel-test flow's
            // readiness gate clears immediately under the mock daemon.
            last_handshake: Some(chrono::Utc::now()),
        }))
    }

    async fn list(&self) -> anyhow::Result<Vec<String>> {
        Ok(Vec::new())
    }
}
