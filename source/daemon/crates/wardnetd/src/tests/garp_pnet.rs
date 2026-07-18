//! Tests for the pnet-backed GARP frame builder and pulse timing.

use std::net::Ipv4Addr;
use std::time::Duration;

use pnet::packet::arp::{ArpHardwareTypes, ArpOperations};
use pnet::packet::ethernet::EtherTypes;
use pnet::util::MacAddr;

use crate::garp_pnet::{PULSE_COUNT, PULSE_GAP, build_garp_reply};

use pnet::packet::Packet;
use pnet::packet::arp::ArpPacket;
use pnet::packet::ethernet::EthernetPacket;

#[test]
fn frame_length_is_42_bytes() {
    let frame = build_garp_reply(
        MacAddr(0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF),
        Ipv4Addr::new(192, 168, 1, 1),
    )
    .expect("frame builder returns Some");
    assert_eq!(frame.len(), 42);
}

#[test]
fn ethernet_header_uses_broadcast_dest_and_phase_src() {
    let phase_mac = MacAddr(0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF);
    let frame = build_garp_reply(phase_mac, Ipv4Addr::new(192, 168, 1, 1)).unwrap();
    let eth = EthernetPacket::new(&frame).unwrap();

    assert_eq!(eth.get_destination(), MacAddr::broadcast());
    assert_eq!(eth.get_source(), phase_mac);
    assert_eq!(eth.get_ethertype(), EtherTypes::Arp);
}

#[test]
fn arp_payload_is_a_reply_with_aligned_sender_hw_and_target_proto() {
    // Per decision 4: clients use the ARP sender-HW field to update
    // their cache, so it MUST equal the Ethernet src-MAC.
    let phase_mac = MacAddr(0x11, 0x22, 0x33, 0x44, 0x55, 0x66);
    let router_ip = Ipv4Addr::new(10, 91, 0, 1);
    let frame = build_garp_reply(phase_mac, router_ip).unwrap();
    let eth = EthernetPacket::new(&frame).unwrap();
    let arp = ArpPacket::new(eth.payload()).unwrap();

    assert_eq!(arp.get_hardware_type(), ArpHardwareTypes::Ethernet);
    assert_eq!(arp.get_protocol_type(), EtherTypes::Ipv4);
    assert_eq!(arp.get_hw_addr_len(), 6);
    assert_eq!(arp.get_proto_addr_len(), 4);
    assert_eq!(arp.get_operation(), ArpOperations::Reply);
    assert_eq!(arp.get_sender_hw_addr(), phase_mac);
    assert_eq!(arp.get_sender_proto_addr(), router_ip);
    assert_eq!(arp.get_target_hw_addr(), MacAddr::broadcast());
    assert_eq!(arp.get_target_proto_addr(), router_ip);
}

#[test]
fn pulse_constants_keep_phase_under_one_second() {
    // Acceptance criterion: full GARP sequence (2 pulses) ≤ 1s/phase.
    let gaps = u32::try_from(PULSE_COUNT).expect("PULSE_COUNT fits in u32") - 1;
    let total = PULSE_GAP * gaps;
    assert!(total < Duration::from_secs(1), "phase budget exceeded");
    const { assert!(PULSE_COUNT >= 2, "need at least 2 pulses for redundancy") };
}
