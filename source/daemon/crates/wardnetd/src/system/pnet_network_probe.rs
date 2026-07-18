//! Real [`NetworkProbe`] implementation backed by `pnet`.
//!
//! Two probes share this file because they share the same I/O shape
//! (open a pnet datalink channel, send a frame, read replies for a
//! bounded window) and the same setup costs (interface lookup, MAC +
//! IP discovery):
//!
//! - **ARP probe** (`arp_probe`): one ARP request, waits for the
//!   reply matching `target_ip`, returns its sender MAC.
//! - **DHCP self-probe** (`dhcp_self_probe`): one DHCPDISCOVER from a
//!   synthetic MAC, returns the `ServerIdentifier` of every DHCP
//!   server that replies inside the window.
//!
//! Both run inside `spawn_blocking` because pnet's datalink channel
//! is synchronous; both interactions complete in well under their
//! window so this is fine for an interactive wizard step.

use std::net::Ipv4Addr;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use dhcproto::v4::{DhcpOption, Flags, Message, MessageType, Opcode};
use dhcproto::{Decodable, Decoder, Encodable, Encoder};
use pnet::datalink::{self, Channel, Config};
use pnet::packet::Packet;
use pnet::packet::arp::{ArpOperations, ArpPacket};
use pnet::packet::ethernet::{EtherTypes, EthernetPacket, MutableEthernetPacket};
use pnet::packet::ip::IpNextHeaderProtocols;
use pnet::packet::ipv4::{Ipv4Packet, MutableIpv4Packet};
use pnet::packet::udp::{MutableUdpPacket, UdpPacket, ipv4_checksum as udp_ipv4_checksum};
use pnet::util::MacAddr;
use wardnetd_services::system::{DhcpProbeOutcome, NetworkProbe};

use crate::packet_capture_pnet::{build_arp_request, find_interface};

/// Wait up to this long for a single ARP reply before giving up.
///
/// 750 ms is comfortably longer than typical LAN ARP latency (<10 ms)
/// and short enough to keep the wizard feeling responsive when the
/// gateway is unreachable.
const ARP_TIMEOUT: Duration = Duration::from_millis(750);

/// Wait this long for DHCP OFFERs to come back. Most servers reply in
/// <100 ms; 1.5 s gives a comfortable margin without making the
/// wizard feel slow.
const DHCP_TIMEOUT: Duration = Duration::from_millis(1500);

/// Per-poll read timeout on the datalink channel. Bounded so we revisit
/// the deadline check often without busy-looping.
const READ_POLL: Duration = Duration::from_millis(100);

/// Synthetic MAC used as the DISCOVER's chaddr. Locally administered
/// (second hex of first byte = 2) and clearly not a real client so
/// Wardnet's own DHCP server can ignore the lease tracking — the
/// probe just wants to know whether *anything* answered.
pub(crate) const PROBE_CHADDR: MacAddr = MacAddr(0x02, 0x57, 0x41, 0x52, 0x44, 0x01);

/// Stable transaction ID. Servers echo it in their OFFER so we can
/// filter for replies to *our* probe and ignore unrelated DHCP
/// chatter on the LAN.
pub(crate) const PROBE_XID: u32 = 0xDEAD_BEEF;

/// Real probe backed by `pnet`. Resolves source MAC + IP from the
/// named interface on each call so re-creating the channel after a
/// link bounce is automatic.
pub struct PnetNetworkProbe {
    interface: String,
}

impl PnetNetworkProbe {
    #[must_use]
    pub fn new(interface: String) -> Self {
        Self { interface }
    }
}

#[async_trait]
impl NetworkProbe for PnetNetworkProbe {
    async fn arp_probe(&self, target_ip: Ipv4Addr) -> anyhow::Result<Option<String>> {
        let interface_name = self.interface.clone();

        tokio::task::spawn_blocking(move || run_arp_probe(&interface_name, target_ip))
            .await
            .map_err(|e| anyhow::anyhow!("arp probe blocking task panicked: {e}"))?
    }

    async fn dhcp_self_probe(&self) -> anyhow::Result<DhcpProbeOutcome> {
        let interface_name = self.interface.clone();

        tokio::task::spawn_blocking(move || run_dhcp_probe(&interface_name))
            .await
            .map_err(|e| anyhow::anyhow!("dhcp probe blocking task panicked: {e}"))?
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
pub(crate) fn parse_arp_reply(frame: &[u8], target_ip: Ipv4Addr) -> Option<String> {
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

// ---------------------------------------------------------------------------
// DHCP self-probe
// ---------------------------------------------------------------------------

/// Synchronous DHCPDISCOVER probe.
///
/// Builds the DHCP message with `dhcproto`, wraps it in UDP/IP/Ethernet
/// using `pnet`, and sends it as a broadcast frame. We deliberately
/// bypass the kernel UDP stack: port 68 is typically held by the host's
/// `dhcpcd` (and 67 by Wardnet itself), so binding either via a normal
/// `UdpSocket` would conflict.
fn run_dhcp_probe(interface_name: &str) -> anyhow::Result<DhcpProbeOutcome> {
    let iface = find_interface(interface_name)?;
    let src_mac = iface
        .mac
        .ok_or_else(|| anyhow::anyhow!("interface '{interface_name}' has no MAC address"))?;

    let config = Config {
        read_timeout: Some(READ_POLL),
        ..Config::default()
    };
    let Channel::Ethernet(mut tx, mut rx) = datalink::channel(&iface, config)? else {
        anyhow::bail!("unsupported channel type for interface '{interface_name}'");
    };

    let frame = build_dhcp_discover_frame(src_mac)?;
    if let Some(send_result) = tx.send_to(&frame, None) {
        send_result.map_err(|e| anyhow::anyhow!("failed to send DHCPDISCOVER: {e}"))?;
    } else {
        anyhow::bail!("datalink channel rejected the DHCPDISCOVER frame");
    }

    let mut responders: Vec<Ipv4Addr> = Vec::new();
    let deadline = Instant::now() + DHCP_TIMEOUT;
    while Instant::now() < deadline {
        match rx.next() {
            Ok(frame) => {
                if let Some(server_ip) = parse_dhcp_offer(frame, PROBE_XID)
                    && !responders.contains(&server_ip)
                {
                    responders.push(server_ip);
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {}
            Err(e) => return Err(anyhow::anyhow!("DHCP receive failed: {e}")),
        }
    }

    Ok(DhcpProbeOutcome { responders })
}

/// Build a complete Ethernet frame carrying a DHCPDISCOVER:
/// `Ethernet(broadcast) → IPv4(0.0.0.0 → 255.255.255.255) → UDP(68 → 67) → DHCP`.
pub(crate) fn build_dhcp_discover_frame(src_mac: MacAddr) -> anyhow::Result<Vec<u8>> {
    let dhcp_payload = encode_dhcp_discover()?;

    // UDP: 8-byte header + payload.
    let udp_total_len = 8 + dhcp_payload.len();
    // IPv4: 20-byte header + UDP.
    let ip_total_len = 20 + udp_total_len;
    // Ethernet: 14-byte header + IPv4.
    let frame_len = 14 + ip_total_len;
    let mut buf = vec![0u8; frame_len];

    // -- Ethernet ----------------------------------------------------------
    {
        let mut eth = MutableEthernetPacket::new(&mut buf)
            .ok_or_else(|| anyhow::anyhow!("frame buffer too small for Ethernet header"))?;
        eth.set_destination(MacAddr::broadcast());
        eth.set_source(src_mac);
        eth.set_ethertype(EtherTypes::Ipv4);
    }

    // -- IPv4 --------------------------------------------------------------
    {
        let mut ip = MutableIpv4Packet::new(&mut buf[14..])
            .ok_or_else(|| anyhow::anyhow!("frame buffer too small for IPv4 header"))?;
        ip.set_version(4);
        ip.set_header_length(5);
        ip.set_dscp(0);
        ip.set_ecn(0);
        ip.set_total_length(u16::try_from(ip_total_len).unwrap_or(u16::MAX));
        ip.set_identification(0);
        ip.set_flags(0);
        ip.set_fragment_offset(0);
        ip.set_ttl(64);
        ip.set_next_level_protocol(IpNextHeaderProtocols::Udp);
        ip.set_source(Ipv4Addr::UNSPECIFIED);
        ip.set_destination(Ipv4Addr::BROADCAST);
        // pnet's checksum helper takes an Ipv4Packet view over the header
        // we just populated; assign last so it covers the final values.
        let checksum = pnet::packet::ipv4::checksum(&ip.to_immutable());
        ip.set_checksum(checksum);
    }

    // -- UDP ---------------------------------------------------------------
    {
        let mut udp = MutableUdpPacket::new(&mut buf[14 + 20..])
            .ok_or_else(|| anyhow::anyhow!("frame buffer too small for UDP header"))?;
        udp.set_source(68);
        udp.set_destination(67);
        udp.set_length(u16::try_from(udp_total_len).unwrap_or(u16::MAX));
        udp.set_payload(&dhcp_payload);
        // UDP checksum covers a pseudo-header with the same source +
        // dest + protocol that the receiver expects.
        let checksum = udp_ipv4_checksum(
            &udp.to_immutable(),
            &Ipv4Addr::UNSPECIFIED,
            &Ipv4Addr::BROADCAST,
        );
        udp.set_checksum(checksum);
    }

    Ok(buf)
}

/// Encode a DHCPDISCOVER carrying our synthetic chaddr + `PROBE_XID`
/// and a parameter-request list that's standard enough to draw an
/// OFFER from any conforming server.
fn encode_dhcp_discover() -> anyhow::Result<Vec<u8>> {
    let mut msg = Message::default();
    msg.set_opcode(Opcode::BootRequest)
        .set_xid(PROBE_XID)
        .set_flags(Flags::default().set_broadcast())
        .set_chaddr(&[
            PROBE_CHADDR.0,
            PROBE_CHADDR.1,
            PROBE_CHADDR.2,
            PROBE_CHADDR.3,
            PROBE_CHADDR.4,
            PROBE_CHADDR.5,
        ]);

    let opts = msg.opts_mut();
    opts.insert(DhcpOption::MessageType(MessageType::Discover));
    opts.insert(DhcpOption::ParameterRequestList(vec![
        dhcproto::v4::OptionCode::SubnetMask,
        dhcproto::v4::OptionCode::Router,
        dhcproto::v4::OptionCode::DomainNameServer,
    ]));

    let mut buf = Vec::with_capacity(300);
    msg.encode(&mut Encoder::new(&mut buf))
        .map_err(|e| anyhow::anyhow!("failed to encode DHCPDISCOVER: {e}"))?;
    Ok(buf)
}

/// Parse an Ethernet frame and, if it carries a DHCPOFFER for our
/// transaction, extract the `ServerIdentifier`. Anything else returns
/// `None`.
pub(crate) fn parse_dhcp_offer(frame: &[u8], xid: u32) -> Option<Ipv4Addr> {
    let eth = EthernetPacket::new(frame)?;
    if eth.get_ethertype() != EtherTypes::Ipv4 {
        return None;
    }
    let ip = Ipv4Packet::new(eth.payload())?;
    if ip.get_next_level_protocol() != IpNextHeaderProtocols::Udp {
        return None;
    }
    let udp = UdpPacket::new(ip.payload())?;
    // DHCP servers send from 67 to a client on 68. Reject anything else.
    if udp.get_source() != 67 || udp.get_destination() != 68 {
        return None;
    }
    let msg = Message::decode(&mut Decoder::new(udp.payload())).ok()?;
    if msg.xid() != xid {
        return None;
    }
    // Only OFFERs count; an ACK to someone else's REQUEST is not a
    // self-probe response.
    let is_offer = msg
        .opts()
        .iter()
        .any(|(_, opt)| matches!(opt, DhcpOption::MessageType(MessageType::Offer)));
    if !is_offer {
        return None;
    }
    // ServerIdentifier is the canonical "this is my IP" advertised by
    // a DHCP server. Some servers also set siaddr; ServerIdentifier
    // wins per RFC 2131.
    msg.opts().iter().find_map(|(_, opt)| match opt {
        DhcpOption::ServerIdentifier(ip) => Some(*ip),
        _ => None,
    })
}
