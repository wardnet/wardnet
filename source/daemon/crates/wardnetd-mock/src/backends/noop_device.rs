//! No-op [`PacketCapture`], [`HostnameResolver`] and [`DeviceProber`]
//! implementations for the mock server.

use std::net::IpAddr;

use async_trait::async_trait;
use tokio::sync::mpsc::Sender;
use tokio_util::sync::CancellationToken;
use wardnetd_services::device::{DeviceProber, HostnameResolver, ObservedDevice, PacketCapture};

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

/// A prober that contacts nothing and answers from the address alone.
///
/// Deterministic so `make run-dev` exercises *both* outcomes without any real
/// traffic: a device whose last address octet is odd reports one answering
/// port (picked from the catalog list by that octet, so different seeded
/// devices resolve to different vendors), and an even one reports none — which
/// is the "No known ports answered" path the UI has to say out loud.
#[derive(Debug, Default, Clone)]
pub struct NoopDeviceProber;

#[async_trait]
impl DeviceProber for NoopDeviceProber {
    async fn probe(&self, ip: IpAddr, ports: &[u16]) -> Vec<u16> {
        let octet = match ip {
            IpAddr::V4(v4) => v4.octets()[3],
            IpAddr::V6(v6) => v6.octets()[15],
        };
        let answering = if octet % 2 == 1 && !ports.is_empty() {
            vec![ports[usize::from(octet) % ports.len()]]
        } else {
            Vec::new()
        };
        tracing::debug!(
            %ip,
            probed = ports.len(),
            ?answering,
            "mock device probe (no traffic sent): ip={ip}"
        );
        answering
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
