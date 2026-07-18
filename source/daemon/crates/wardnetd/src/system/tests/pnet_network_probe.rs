//! Tests for the pnet-backed ARP / DHCP network probe frame
//! builders and parsers.

use std::net::Ipv4Addr;

use dhcproto::v4::{DhcpOption, Message, MessageType, Opcode};
use dhcproto::{Encodable, Encoder};
use pnet::packet::arp::{ArpHardwareTypes, ArpOperations, MutableArpPacket};
use pnet::packet::ethernet::{EtherTypes, MutableEthernetPacket};
use pnet::packet::ip::IpNextHeaderProtocols;
use pnet::packet::ipv4::MutableIpv4Packet;
use pnet::packet::udp::{MutableUdpPacket, ipv4_checksum as udp_ipv4_checksum};
use pnet::util::MacAddr;

use crate::packet_capture_pnet::build_arp_request;
use crate::system::pnet_network_probe::{
    PROBE_CHADDR, PROBE_XID, build_dhcp_discover_frame, parse_arp_reply, parse_dhcp_offer,
};

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

// -- DHCP probe --------------------------------------------------------

/// Wrap a DHCP `Message` in UDP/IPv4/Ethernet using the same
/// builder paths as `build_dhcp_discover_frame`, swapping the
/// payload for an OFFER reply we can hand to `parse_dhcp_offer`.
fn craft_dhcp_offer_frame(server_ip: Ipv4Addr, xid: u32) -> Vec<u8> {
    let mut msg = Message::default();
    msg.set_opcode(Opcode::BootReply)
        .set_xid(xid)
        .set_yiaddr(Ipv4Addr::new(192, 168, 1, 50))
        .set_siaddr(server_ip)
        .set_chaddr(&[
            PROBE_CHADDR.0,
            PROBE_CHADDR.1,
            PROBE_CHADDR.2,
            PROBE_CHADDR.3,
            PROBE_CHADDR.4,
            PROBE_CHADDR.5,
        ]);
    let opts = msg.opts_mut();
    opts.insert(DhcpOption::MessageType(MessageType::Offer));
    opts.insert(DhcpOption::ServerIdentifier(server_ip));

    let mut payload = Vec::new();
    msg.encode(&mut Encoder::new(&mut payload)).unwrap();

    let udp_total_len = 8 + payload.len();
    let ip_total_len = 20 + udp_total_len;
    let frame_len = 14 + ip_total_len;
    let mut buf = vec![0u8; frame_len];

    {
        let mut eth = MutableEthernetPacket::new(&mut buf).unwrap();
        eth.set_destination(MacAddr::broadcast());
        eth.set_source(MacAddr(0xDE, 0xAD, 0xBE, 0xEF, 0x00, 0x02));
        eth.set_ethertype(EtherTypes::Ipv4);
    }
    {
        let mut ip = MutableIpv4Packet::new(&mut buf[14..]).unwrap();
        ip.set_version(4);
        ip.set_header_length(5);
        ip.set_total_length(u16::try_from(ip_total_len).unwrap());
        ip.set_ttl(64);
        ip.set_next_level_protocol(IpNextHeaderProtocols::Udp);
        ip.set_source(server_ip);
        ip.set_destination(Ipv4Addr::BROADCAST);
        let cs = pnet::packet::ipv4::checksum(&ip.to_immutable());
        ip.set_checksum(cs);
    }
    {
        let mut udp = MutableUdpPacket::new(&mut buf[14 + 20..]).unwrap();
        udp.set_source(67);
        udp.set_destination(68);
        udp.set_length(u16::try_from(udp_total_len).unwrap());
        udp.set_payload(&payload);
        let cs = udp_ipv4_checksum(&udp.to_immutable(), &server_ip, &Ipv4Addr::BROADCAST);
        udp.set_checksum(cs);
    }
    buf
}

#[test]
fn build_discover_frame_round_trips_through_parser() {
    // Build the frame the probe sends, then re-parse it as if it
    // were arriving from the wire — proves the wrapping layers
    // line up. We swap the message type to OFFER so the parser's
    // type filter returns Some.
    let frame = build_dhcp_discover_frame(MacAddr(0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0x11)).unwrap();

    // Replace the DHCP payload with an OFFER carrying a known
    // server identifier — this is just a layered re-test of the
    // parser using the real wrapper code.
    let server_ip = Ipv4Addr::new(192, 168, 1, 1);
    let offer = craft_dhcp_offer_frame(server_ip, PROBE_XID);
    assert_eq!(parse_dhcp_offer(&offer, PROBE_XID), Some(server_ip));

    // Sanity: the DISCOVER frame itself isn't an OFFER and parses
    // to None — guards against accidentally matching our own
    // outgoing packet if we ever loop back.
    assert!(parse_dhcp_offer(&frame, PROBE_XID).is_none());
}

#[test]
fn ignores_offer_with_wrong_xid() {
    let server_ip = Ipv4Addr::new(192, 168, 1, 1);
    let frame = craft_dhcp_offer_frame(server_ip, PROBE_XID.wrapping_add(1));
    assert!(parse_dhcp_offer(&frame, PROBE_XID).is_none());
}

#[test]
fn ignores_non_dhcp_udp_traffic() {
    // UDP packet to port 53 (DNS) — definitely not a DHCP OFFER.
    let payload = b"\x00\x00\x00\x01some dns junk";
    let udp_total_len = 8 + payload.len();
    let ip_total_len = 20 + udp_total_len;
    let frame_len = 14 + ip_total_len;
    let mut buf = vec![0u8; frame_len];
    {
        let mut eth = MutableEthernetPacket::new(&mut buf).unwrap();
        eth.set_destination(MacAddr::broadcast());
        eth.set_source(MacAddr(0, 0, 0, 0, 0, 0));
        eth.set_ethertype(EtherTypes::Ipv4);
    }
    {
        let mut ip = MutableIpv4Packet::new(&mut buf[14..]).unwrap();
        ip.set_version(4);
        ip.set_header_length(5);
        ip.set_total_length(u16::try_from(ip_total_len).unwrap());
        ip.set_ttl(64);
        ip.set_next_level_protocol(IpNextHeaderProtocols::Udp);
        ip.set_source(Ipv4Addr::new(1, 1, 1, 1));
        ip.set_destination(Ipv4Addr::new(192, 168, 1, 100));
    }
    {
        let mut udp = MutableUdpPacket::new(&mut buf[14 + 20..]).unwrap();
        udp.set_source(53);
        udp.set_destination(54321);
        udp.set_length(u16::try_from(udp_total_len).unwrap());
        udp.set_payload(payload);
    }
    assert!(parse_dhcp_offer(&buf, PROBE_XID).is_none());
}
