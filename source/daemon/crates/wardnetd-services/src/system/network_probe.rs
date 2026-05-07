use std::net::Ipv4Addr;

use async_trait::async_trait;

/// Active LAN probe — distinct from [`NetworkInspector`] which only
/// reads OS state. The current shape is just an ARP probe used by
/// the wizard's router-MAC step; a DHCP self-probe lands on this
/// trait in a follow-up commit so both probe paths stay testable
/// behind a single seam.
///
/// MAC addresses cross the trait boundary as colon-separated hex
/// strings (`AA:BB:CC:DD:EE:FF`) so the service layer doesn't have
/// to take a transitive `pnet` dependency.
#[async_trait]
pub trait NetworkProbe: Send + Sync {
    /// Send an ARP request for `target_ip` and return the responder's
    /// MAC, or `None` if no reply arrived inside the impl's timeout
    /// (~1s on the real impl). Errors are reserved for setup
    /// failures (interface not found, no source IP, raw-socket
    /// permission denied) — a missing reply is a normal outcome and
    /// surfaces as `Ok(None)`.
    async fn arp_probe(&self, target_ip: Ipv4Addr) -> anyhow::Result<Option<String>>;
}
