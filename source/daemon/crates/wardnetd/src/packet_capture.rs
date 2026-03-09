use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

/// A device observation from a captured network packet.
#[derive(Debug, Clone)]
pub struct ObservedDevice {
    /// Normalized MAC address in "AA:BB:CC:DD:EE:FF" format.
    pub mac: String,
    /// IPv4 address as a string.
    pub ip: String,
    /// How this observation was captured.
    pub source: PacketSource,
}

/// The type of packet that produced a device observation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PacketSource {
    /// ARP request or reply.
    Arp,
    /// IPv4 packet.
    Ip,
}

/// Abstraction over network packet capture for device detection.
///
/// Covers both passive capture (listening for traffic) and active ARP scanning
/// (broadcasting ARP requests to discover idle devices).
#[async_trait]
pub trait PacketCapture: Send + Sync {
    /// Passively capture packets on the given network interface.
    ///
    /// Sends device observations on the provided channel until cancelled.
    async fn capture_loop(
        &self,
        interface: &str,
        sender: tokio::sync::mpsc::Sender<ObservedDevice>,
        cancel: CancellationToken,
    ) -> anyhow::Result<()>;

    /// Send ARP requests for all IPs in the LAN subnet to discover connected devices.
    ///
    /// Responses arrive via the passive `capture_loop` -- this just sends the probes.
    async fn arp_scan(&self, interface: &str) -> anyhow::Result<()>;
}
