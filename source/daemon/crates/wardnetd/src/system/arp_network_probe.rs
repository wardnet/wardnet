//! ARP-based [`NetworkProbe`] implementation.
//!
//! Sends a single ARP request from the LAN interface and waits up to
//! [`ARP_TIMEOUT`] for a reply matching the target IP. The send/receive
//! pair runs inside a `spawn_blocking` task because pnet's datalink
//! channel is synchronous; the ARP exchange is fast enough that this
//! is fine for an interactive wizard step.
//!
//! Source MAC + IP are taken from the named interface at probe time so
//! the daemon doesn't have to track interface IP changes itself.

use std::net::Ipv4Addr;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use pnet::datalink::{self, Channel, Config};
use pnet::packet::Packet;
use pnet::packet::arp::{ArpOperations, ArpPacket};
use pnet::packet::ethernet::{EtherTypes, EthernetPacket};
use wardnetd_services::system::NetworkProbe;

use crate::packet_capture_pnet::{build_arp_request, find_interface};

/// Wait up to this long for a single ARP reply before giving up.
///
/// 750 ms is comfortably longer than typical LAN ARP latency (<10 ms)
/// and short enough to keep the wizard feeling responsive when the
/// gateway is unreachable.
const ARP_TIMEOUT: Duration = Duration::from_millis(750);

/// Per-poll read timeout on the datalink channel. Bounded so we revisit
/// the deadline check often without busy-looping.
const READ_POLL: Duration = Duration::from_millis(100);

/// Real ARP probe backed by `pnet`. Resolves source MAC + IP from the
/// named interface on each call so re-creating the channel after a
/// link bounce is automatic.
pub struct ArpNetworkProbe {
    interface: String,
}

impl ArpNetworkProbe {
    #[must_use]
    pub fn new(interface: String) -> Self {
        Self { interface }
    }
}

#[async_trait]
impl NetworkProbe for ArpNetworkProbe {
    async fn arp_probe(&self, target_ip: Ipv4Addr) -> anyhow::Result<Option<String>> {
        let interface_name = self.interface.clone();

        tokio::task::spawn_blocking(move || run_arp_probe(&interface_name, target_ip))
            .await
            .map_err(|e| anyhow::anyhow!("arp probe blocking task panicked: {e}"))?
    }
}

/// Synchronous ARP probe — extracted so the `spawn_blocking` call is
/// just a one-line forward.
fn run_arp_probe(interface_name: &str, target_ip: Ipv4Addr) -> anyhow::Result<Option<String>> {
    let iface = find_interface(interface_name)?;
    let src_mac = iface
        .mac
        .ok_or_else(|| anyhow::anyhow!("interface '{interface_name}' has no MAC address"))?;
    let src_ip = iface
        .ips
        .iter()
        .find_map(|ip| match ip {
            pnet::ipnetwork::IpNetwork::V4(net) => Some(net.ip()),
            pnet::ipnetwork::IpNetwork::V6(_) => None,
        })
        .ok_or_else(|| anyhow::anyhow!("interface '{interface_name}' has no IPv4 address"))?;

    let config = Config {
        read_timeout: Some(READ_POLL),
        ..Config::default()
    };

    let Channel::Ethernet(mut tx, mut rx) = datalink::channel(&iface, config)? else {
        anyhow::bail!("unsupported channel type for interface '{interface_name}'");
    };

    let request = build_arp_request(src_mac, src_ip, target_ip)
        .ok_or_else(|| anyhow::anyhow!("failed to build ARP request"))?;
    if let Some(send_result) = tx.send_to(&request, None) {
        send_result.map_err(|e| anyhow::anyhow!("failed to send ARP request: {e}"))?;
    } else {
        anyhow::bail!("datalink channel rejected the ARP request");
    }

    let deadline = Instant::now() + ARP_TIMEOUT;
    while Instant::now() < deadline {
        match rx.next() {
            Ok(frame) => {
                if let Some(mac) = parse_arp_reply(frame, target_ip) {
                    return Ok(Some(mac));
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {}
            Err(e) => return Err(anyhow::anyhow!("ARP receive failed: {e}")),
        }
    }

    Ok(None)
}

/// Pull the sender MAC out of an ARP reply for `target_ip`. Returns
/// `None` if the frame is anything else.
fn parse_arp_reply(frame: &[u8], target_ip: Ipv4Addr) -> Option<String> {
    let eth = EthernetPacket::new(frame)?;
    if eth.get_ethertype() != EtherTypes::Arp {
        return None;
    }
    let arp = ArpPacket::new(eth.payload())?;
    if arp.get_operation() != ArpOperations::Reply {
        return None;
    }
    if arp.get_sender_proto_addr() != target_ip {
        return None;
    }
    Some(arp.get_sender_hw_addr().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use pnet::packet::arp::{ArpHardwareTypes, MutableArpPacket};
    use pnet::packet::ethernet::MutableEthernetPacket;
    use pnet::util::MacAddr;

    fn craft_arp_reply(
        src_mac: MacAddr,
        src_ip: Ipv4Addr,
        dst_mac: MacAddr,
        dst_ip: Ipv4Addr,
    ) -> Vec<u8> {
        let mut buf = vec![0u8; 42];
        {
            let mut eth = MutableEthernetPacket::new(&mut buf).unwrap();
            eth.set_destination(dst_mac);
            eth.set_source(src_mac);
            eth.set_ethertype(EtherTypes::Arp);
        }
        {
            let mut arp = MutableArpPacket::new(&mut buf[14..]).unwrap();
            arp.set_hardware_type(ArpHardwareTypes::Ethernet);
            arp.set_protocol_type(EtherTypes::Ipv4);
            arp.set_hw_addr_len(6);
            arp.set_proto_addr_len(4);
            arp.set_operation(ArpOperations::Reply);
            arp.set_sender_hw_addr(src_mac);
            arp.set_sender_proto_addr(src_ip);
            arp.set_target_hw_addr(dst_mac);
            arp.set_target_proto_addr(dst_ip);
        }
        buf
    }

    #[test]
    fn parses_matching_reply() {
        let frame = craft_arp_reply(
            MacAddr(0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF),
            Ipv4Addr::new(192, 168, 1, 1),
            MacAddr(0x11, 0x22, 0x33, 0x44, 0x55, 0x66),
            Ipv4Addr::new(192, 168, 1, 100),
        );
        let mac = parse_arp_reply(&frame, Ipv4Addr::new(192, 168, 1, 1));
        assert_eq!(mac.as_deref(), Some("aa:bb:cc:dd:ee:ff"));
    }

    #[test]
    fn ignores_reply_from_wrong_ip() {
        let frame = craft_arp_reply(
            MacAddr(0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF),
            Ipv4Addr::new(192, 168, 1, 2),
            MacAddr(0x11, 0x22, 0x33, 0x44, 0x55, 0x66),
            Ipv4Addr::new(192, 168, 1, 100),
        );
        assert!(parse_arp_reply(&frame, Ipv4Addr::new(192, 168, 1, 1)).is_none());
    }

    #[test]
    fn ignores_arp_request_frames() {
        let req = build_arp_request(
            MacAddr(0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF),
            Ipv4Addr::new(192, 168, 1, 100),
            Ipv4Addr::new(192, 168, 1, 1),
        )
        .unwrap();
        assert!(parse_arp_reply(&req, Ipv4Addr::new(192, 168, 1, 1)).is_none());
    }

    #[test]
    fn ignores_non_arp_ethertype() {
        // Ethernet frame with EtherType IPv4 — should be ignored.
        let mut buf = vec![0u8; 42];
        {
            let mut eth = MutableEthernetPacket::new(&mut buf).unwrap();
            eth.set_destination(MacAddr(0, 0, 0, 0, 0, 0));
            eth.set_source(MacAddr(0, 0, 0, 0, 0, 0));
            eth.set_ethertype(EtherTypes::Ipv4);
        }
        assert!(parse_arp_reply(&buf, Ipv4Addr::new(192, 168, 1, 1)).is_none());
    }
}
