use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

use crate::packet_capture::{ObservedDevice, PacketCapture};

/// No-op packet capture for development and testing.
///
/// `capture_loop` waits until cancelled, `arp_scan` returns immediately.
/// Used when running with `--mock-network`.
#[derive(Debug)]
pub struct NoopPacketCapture;

#[async_trait]
impl PacketCapture for NoopPacketCapture {
    async fn capture_loop(
        &self,
        interface: &str,
        _sender: tokio::sync::mpsc::Sender<ObservedDevice>,
        cancel: CancellationToken,
    ) -> anyhow::Result<()> {
        tracing::info!(
            interface,
            "noop: capture loop started, waiting for cancellation"
        );
        cancel.cancelled().await;
        tracing::info!(interface, "noop: capture loop cancelled");
        Ok(())
    }

    async fn arp_scan(&self, interface: &str) -> anyhow::Result<()> {
        tracing::info!(interface, "noop: arp scan");
        Ok(())
    }
}
