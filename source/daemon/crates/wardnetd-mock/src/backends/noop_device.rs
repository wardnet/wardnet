//! No-op [`PacketCapture`] and [`HostnameResolver`] implementations for the
//! mock server.

use async_trait::async_trait;
use tokio::sync::mpsc::Sender;
use tokio_util::sync::CancellationToken;
use wardnetd_services::device::{HostnameResolver, ObservedDevice, PacketCapture};

/// A packet capture backend that captures nothing.
///
/// `capture_loop` waits for the cancellation token and returns — it never
/// sends an observation. `arp_scan` is a no-op.
#[derive(Debug, Default, Clone)]
pub struct NoopPacketCapture;

#[async_trait]
impl PacketCapture for NoopPacketCapture {
    async fn capture_loop(
        &self,
        interface: &str,
        _sender: Sender<ObservedDevice>,
        cancel: CancellationToken,
    ) -> anyhow::Result<()> {
        tracing::debug!(
            interface,
            "mock packet capture_loop starting: interface={interface}",
        );
        cancel.cancelled().await;
        tracing::debug!(
            interface,
            "mock packet capture_loop cancelled: interface={interface}",
        );
        Ok(())
    }

    async fn arp_scan(&self, interface: &str) -> anyhow::Result<()> {
        tracing::debug!(interface, "mock arp_scan: interface={interface}");
        Ok(())
    }
}

/// A hostname resolver that never resolves.
#[derive(Debug, Default, Clone)]
pub struct NoopHostnameResolver;

#[async_trait]
impl HostnameResolver for NoopHostnameResolver {
    async fn resolve(&self, ip: &str) -> Option<String> {
        tracing::debug!(ip, "mock hostname resolve (returning None): ip={ip}");
        None
    }
}
