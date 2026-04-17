use std::net::Ipv4Addr;

use pnet::packet::Packet;
use pnet::packet::arp::{ArpHardwareTypes, ArpOperations, ArpPacket, MutableArpPacket};
use pnet::packet::ethernet::{EtherTypes, EthernetPacket, MutableEthernetPacket};
use pnet::packet::ipv4::MutableIpv4Packet;
use pnet::util::MacAddr;

use crate::packet_capture_pnet::{
    build_arp_request, find_interface, format_mac, parse_frame, should_filter_mac, subnet_hosts,
};
use wardnetd_services::device::packet_capture::PacketSource;

// ---------------------------------------------------------------------------
// format_mac
// ---------------------------------------------------------------------------

#[test]
fn format_mac_all_zeros() {
    assert_eq!(format_mac(MacAddr::zero()), "00:00:00:00:00:00");
}

#[test]
fn format_mac_all_ff() {
    assert_eq!(
        format_mac(MacAddr::new(0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF)),
        "FF:FF:FF:FF:FF:FF"
    );
}

#[test]
fn format_mac_typical() {
    let mac = MacAddr::new(0xAA, 0xBB, 0xCC, 0x01, 0x02, 0x03);
    assert_eq!(format_mac(mac), "AA:BB:CC:01:02:03");
}

#[test]
fn format_mac_lowercase_hex_digits_uppercased() {
    // Ensure digits a-f come out uppercase.
    let mac = MacAddr::new(0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f);
    assert_eq!(format_mac(mac), "0A:0B:0C:0D:0E:0F");
}

// ---------------------------------------------------------------------------
// should_filter_mac
// ---------------------------------------------------------------------------

#[test]
fn filter_own_mac() {
    let own = MacAddr::new(0xDE, 0xAD, 0xBE, 0xEF, 0x00, 0x01);
    assert!(should_filter_mac(own, own));
}

#[test]
fn filter_broadcast_mac() {
    let own = MacAddr::new(0xDE, 0xAD, 0xBE, 0xEF, 0x00, 0x01);
    assert!(should_filter_mac(MacAddr::broadcast(), own));
}

#[test]
fn filter_multicast_mac() {
    let own = MacAddr::new(0xDE, 0xAD, 0xBE, 0xEF, 0x00, 0x01);
    // Multicast: bit 0 of first octet is set (0x01, 0x33, etc.)
    let multicast = MacAddr::new(0x01, 0x00, 0x5E, 0x00, 0x00, 0x01);
    assert!(should_filter_mac(multicast, own));
}

#[test]
fn allow_normal_unicast_mac() {
    let own = MacAddr::new(0xDE, 0xAD, 0xBE, 0xEF, 0x00, 0x01);
    let other = MacAddr::new(0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0x00);
    assert!(!should_filter_mac(other, own));
}

#[test]
fn filter_ipv6_multicast_mac() {
    let own = MacAddr::new(0x02, 0x00, 0x00, 0x00, 0x00, 0x01);
    // IPv6 multicast prefix 33:33:xx:xx:xx:xx -- first octet 0x33 has bit 0 set
    let ipv6_mcast = MacAddr::new(0x33, 0x33, 0x00, 0x00, 0x00, 0x01);
    assert!(should_filter_mac(ipv6_mcast, own));
}

// ---------------------------------------------------------------------------
// subnet_hosts
// ---------------------------------------------------------------------------

#[test]
fn subnet_hosts_slash_24() {
    let hosts = subnet_hosts(
        Ipv4Addr::new(192, 168, 1, 100),
        Ipv4Addr::new(255, 255, 255, 0),
    );
    // /24 has 256 addresses, minus network (.0) and broadcast (.255) = 254 hosts
    assert_eq!(hosts.len(), 254);
    assert_eq!(hosts[0], Ipv4Addr::new(192, 168, 1, 1));
    assert_eq!(hosts[253], Ipv4Addr::new(192, 168, 1, 254));
}

#[test]
fn subnet_hosts_slash_30() {
    // /30 = 4 addresses, minus 2 = 2 hosts
    let hosts = subnet_hosts(
        Ipv4Addr::new(10, 0, 0, 1),
        Ipv4Addr::new(255, 255, 255, 252),
    );
    assert_eq!(hosts.len(), 2);
    assert_eq!(hosts[0], Ipv4Addr::new(10, 0, 0, 1));
    assert_eq!(hosts[1], Ipv4Addr::new(10, 0, 0, 2));
}

#[test]
fn subnet_hosts_slash_31() {
    // /31 = 2 addresses total, network + broadcast overlap, so no usable hosts
    // per the implementation: start = network+1, end = broadcast-1, start > end => empty
    let hosts = subnet_hosts(
        Ipv4Addr::new(10, 0, 0, 0),
        Ipv4Addr::new(255, 255, 255, 254),
    );
    assert!(hosts.is_empty());
}

#[test]
fn subnet_hosts_slash_32() {
    // /32 = single address, no hosts
    let hosts = subnet_hosts(Ipv4Addr::new(10, 0, 0, 1), Ipv4Addr::BROADCAST);
    assert!(hosts.is_empty());
}

#[test]
fn subnet_hosts_slash_16() {
    let hosts = subnet_hosts(Ipv4Addr::new(172, 16, 0, 50), Ipv4Addr::new(255, 255, 0, 0));
    // /16 = 65536 addresses, minus 2 = 65534 hosts
    assert_eq!(hosts.len(), 65534);
    assert_eq!(hosts[0], Ipv4Addr::new(172, 16, 0, 1));
    assert_eq!(hosts[65533], Ipv4Addr::new(172, 16, 255, 254));
}

// ---------------------------------------------------------------------------
// build_arp_request
// ---------------------------------------------------------------------------

#[test]
fn build_arp_request_produces_valid_frame() {
    let src_mac = MacAddr::new(0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF);
    let src_ip = Ipv4Addr::new(192, 168, 1, 10);
    let target_ip = Ipv4Addr::new(192, 168, 1, 20);

    let frame = build_arp_request(src_mac, src_ip, target_ip).expect("should build frame");
    assert_eq!(frame.len(), 42);

    // Verify Ethernet header
    let eth = EthernetPacket::new(&frame).unwrap();
    assert_eq!(eth.get_destination(), MacAddr::broadcast());
    assert_eq!(eth.get_source(), src_mac);
    assert_eq!(eth.get_ethertype(), EtherTypes::Arp);

    // Verify ARP payload
    let arp = ArpPacket::new(eth.payload()).unwrap();
    assert_eq!(arp.get_hardware_type(), ArpHardwareTypes::Ethernet);
    assert_eq!(arp.get_protocol_type(), EtherTypes::Ipv4);
    assert_eq!(arp.get_hw_addr_len(), 6);
    assert_eq!(arp.get_proto_addr_len(), 4);
    assert_eq!(arp.get_operation(), ArpOperations::Request);
    assert_eq!(arp.get_sender_hw_addr(), src_mac);
    assert_eq!(arp.get_sender_proto_addr(), src_ip);
    assert_eq!(arp.get_target_hw_addr(), MacAddr::zero());
    assert_eq!(arp.get_target_proto_addr(), target_ip);
}

// ---------------------------------------------------------------------------
// parse_frame -- ARP
// ---------------------------------------------------------------------------

/// Helper: build a raw ARP reply frame for testing `parse_frame`.
fn make_arp_frame(sender_mac: MacAddr, sender_ip: Ipv4Addr) -> Vec<u8> {
    let mut buf = vec![0u8; 42];
    {
        let mut eth = MutableEthernetPacket::new(&mut buf).unwrap();
        eth.set_source(sender_mac);
        eth.set_destination(MacAddr::broadcast());
        eth.set_ethertype(EtherTypes::Arp);
    }
    {
        let mut arp = MutableArpPacket::new(&mut buf[14..]).unwrap();
        arp.set_hardware_type(ArpHardwareTypes::Ethernet);
        arp.set_protocol_type(EtherTypes::Ipv4);
        arp.set_hw_addr_len(6);
        arp.set_proto_addr_len(4);
        arp.set_operation(ArpOperations::Reply);
        arp.set_sender_hw_addr(sender_mac);
        arp.set_sender_proto_addr(sender_ip);
        arp.set_target_hw_addr(MacAddr::zero());
        arp.set_target_proto_addr(Ipv4Addr::UNSPECIFIED);
    }
    buf
}

/// Helper: build a raw IPv4 Ethernet frame for testing `parse_frame`.
fn make_ipv4_frame(src_mac: MacAddr, src_ip: Ipv4Addr) -> Vec<u8> {
    // Ethernet header (14) + IPv4 header (20 minimum) = 34 bytes
    let mut buf = vec![0u8; 34];
    {
        let mut eth = MutableEthernetPacket::new(&mut buf).unwrap();
        eth.set_source(src_mac);
        eth.set_destination(MacAddr::new(0xDE, 0xAD, 0x00, 0x00, 0x00, 0x01));
        eth.set_ethertype(EtherTypes::Ipv4);
    }
    {
        let mut ipv4 = MutableIpv4Packet::new(&mut buf[14..]).unwrap();
        ipv4.set_version(4);
        ipv4.set_header_length(5);
        ipv4.set_total_length(20);
        ipv4.set_source(src_ip);
        ipv4.set_destination(Ipv4Addr::new(192, 168, 1, 1));
    }
    buf
}

#[test]
fn parse_frame_arp_valid() {
    let own_mac = MacAddr::new(0xDE, 0xAD, 0xBE, 0xEF, 0x00, 0x01);
    let sender_mac = MacAddr::new(0xAA, 0xBB, 0xCC, 0x11, 0x22, 0x33);
    let sender_ip = Ipv4Addr::new(192, 168, 1, 50);

    let frame = make_arp_frame(sender_mac, sender_ip);
    let obs = parse_frame(&frame, own_mac).expect("should parse ARP frame");

    assert_eq!(obs.mac, "AA:BB:CC:11:22:33");
    assert_eq!(obs.ip, "192.168.1.50");
    assert_eq!(obs.source, PacketSource::Arp);
}

#[test]
fn parse_frame_arp_filtered_own_mac() {
    let own_mac = MacAddr::new(0xAA, 0xBB, 0xCC, 0x11, 0x22, 0x33);
    let frame = make_arp_frame(own_mac, Ipv4Addr::new(192, 168, 1, 1));
    assert!(parse_frame(&frame, own_mac).is_none());
}

#[test]
fn parse_frame_arp_filtered_unspecified_ip() {
    let own_mac = MacAddr::new(0xDE, 0xAD, 0xBE, 0xEF, 0x00, 0x01);
    let sender_mac = MacAddr::new(0xAA, 0xBB, 0xCC, 0x11, 0x22, 0x33);
    let frame = make_arp_frame(sender_mac, Ipv4Addr::UNSPECIFIED);
    assert!(parse_frame(&frame, own_mac).is_none());
}

#[test]
fn parse_frame_arp_filtered_broadcast_sender() {
    let own_mac = MacAddr::new(0xDE, 0xAD, 0xBE, 0xEF, 0x00, 0x01);
    // ARP with broadcast as sender MAC should be filtered
    let frame = make_arp_frame(MacAddr::broadcast(), Ipv4Addr::new(10, 0, 0, 1));
    assert!(parse_frame(&frame, own_mac).is_none());
}

// ---------------------------------------------------------------------------
// parse_frame -- IPv4
// ---------------------------------------------------------------------------

#[test]
fn parse_frame_ipv4_valid() {
    let own_mac = MacAddr::new(0xDE, 0xAD, 0xBE, 0xEF, 0x00, 0x01);
    let src_mac = MacAddr::new(0x00, 0x1A, 0x2B, 0x3C, 0x4D, 0x5E);
    let src_ip = Ipv4Addr::new(10, 0, 0, 42);

    let frame = make_ipv4_frame(src_mac, src_ip);
    let obs = parse_frame(&frame, own_mac).expect("should parse IPv4 frame");

    assert_eq!(obs.mac, "00:1A:2B:3C:4D:5E");
    assert_eq!(obs.ip, "10.0.0.42");
    assert_eq!(obs.source, PacketSource::Ip);
}

#[test]
fn parse_frame_ipv4_filtered_own_mac() {
    let own_mac = MacAddr::new(0x00, 0x1A, 0x2B, 0x3C, 0x4D, 0x5E);
    let frame = make_ipv4_frame(own_mac, Ipv4Addr::new(10, 0, 0, 1));
    assert!(parse_frame(&frame, own_mac).is_none());
}

#[test]
fn parse_frame_ipv4_filtered_unspecified_ip() {
    let own_mac = MacAddr::new(0xDE, 0xAD, 0xBE, 0xEF, 0x00, 0x01);
    let src_mac = MacAddr::new(0x00, 0x1A, 0x2B, 0x3C, 0x4D, 0x5E);
    let frame = make_ipv4_frame(src_mac, Ipv4Addr::UNSPECIFIED);
    assert!(parse_frame(&frame, own_mac).is_none());
}

// ---------------------------------------------------------------------------
// parse_frame -- edge cases
// ---------------------------------------------------------------------------

#[test]
fn parse_frame_empty_data() {
    let own_mac = MacAddr::new(0xDE, 0xAD, 0xBE, 0xEF, 0x00, 0x01);
    assert!(parse_frame(&[], own_mac).is_none());
}

#[test]
fn parse_frame_truncated_ethernet_header() {
    let own_mac = MacAddr::new(0xDE, 0xAD, 0xBE, 0xEF, 0x00, 0x01);
    // Ethernet header needs at least 14 bytes; provide only 10
    assert!(parse_frame(&[0u8; 10], own_mac).is_none());
}

#[test]
fn parse_frame_unknown_ethertype() {
    let own_mac = MacAddr::new(0xDE, 0xAD, 0xBE, 0xEF, 0x00, 0x01);
    // Build a frame with an IPv6 ethertype (0x86DD) -- not handled by parse_frame
    let mut buf = vec![0u8; 54]; // enough for ethernet + some payload
    {
        let mut eth = MutableEthernetPacket::new(&mut buf).unwrap();
        eth.set_source(MacAddr::new(0xAA, 0xBB, 0xCC, 0x11, 0x22, 0x33));
        eth.set_destination(own_mac);
        eth.set_ethertype(EtherTypes::Ipv6);
    }
    assert!(parse_frame(&buf, own_mac).is_none());
}

// ---------------------------------------------------------------------------
// find_interface
// ---------------------------------------------------------------------------

#[test]
fn find_interface_nonexistent() {
    let result = find_interface("this_interface_does_not_exist_xyz_12345");
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("not found"),
        "expected 'not found' in error: {err_msg}"
    );
}
