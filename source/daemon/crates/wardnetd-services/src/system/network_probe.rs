use std::net::Ipv4Addr;

use async_trait::async_trait;

/// Outcome of a DHCP self-probe — every server that responded with
/// an OFFER, identified by the `ServerIdentifier` DHCP option (what
/// the responder claims its own address to be).
#[derive(Debug, Clone, Default)]
pub struct DhcpProbeOutcome {
    pub responders: Vec<Ipv4Addr>,
}

/// Active LAN probe — distinct from [`NetworkInspector`] which only
/// reads OS state. Hosts both the ARP gateway probe and the DHCP
/// self-probe behind a single seam so the service layer doesn't
/// need to know about `pnet` or raw sockets.
///
/// MAC addresses cross the trait boundary as colon-separated hex
/// strings (`AA:BB:CC:DD:EE:FF`).
#[async_trait]
pub trait NetworkProbe: Send + Sync {
    /// Send an ARP request for `target_ip` and return the responder's
    /// MAC, or `None` if no reply arrived inside the impl's timeout
    /// (~1s on the real impl). Errors are reserved for setup
    /// failures (interface not found, no source IP, raw-socket
    /// permission denied) — a missing reply is a normal outcome and
    /// surfaces as `Ok(None)`.
    async fn arp_probe(&self, target_ip: Ipv4Addr) -> anyhow::Result<Option<String>>;

    /// Broadcast a DHCPDISCOVER from a synthetic MAC and collect every
    /// `ServerIdentifier` from OFFER replies that arrive within the
    /// impl's window (~1.5s on the real impl). Caller distinguishes
    /// "Wardnet responded" from "foreign server responded" by
    /// comparing each entry against its known LAN IP.
    async fn dhcp_self_probe(&self) -> anyhow::Result<DhcpProbeOutcome>;
}
