//! Host-independent tests for the netlink policy router's pure helpers.
//!
//! The rtnetlink socket calls have no mockable boundary (they need a real
//! kernel) and are exercised by the e2e harness. What we CAN test here is the
//! predicate deciding which kernel routes `remove_host_route` is allowed to
//! delete — the one that locked a live Pi out of its own network (#886).

use std::net::Ipv4Addr;

use rtnetlink::packet_route::AddressFamily;
use rtnetlink::packet_route::route::{
    RouteAddress, RouteAttribute, RouteHeader, RouteMessage, RouteScope, RouteType,
};
use rtnetlink::packet_route::rule::{RuleAttribute, RuleMessage};

use crate::policy_router_netlink::{is_removable_host_route, rule_table};

/// `main` (254) is where `add_host_route` files our routes; `local` (255) is
/// the kernel-owned table holding the `scope host` route that delivers each of
/// the box's own addresses to itself — production code must never touch it
/// (issue #886). netlink-packet-route exports no `RT_TABLE_LOCAL`, so libc is
/// the canonical source for that one.
const RT_TABLE_MAIN: u8 = RouteHeader::RT_TABLE_MAIN;
const RT_TABLE_LOCAL: u8 = libc::RT_TABLE_LOCAL;

const OIF: u32 = 2;
const LAN_IP: Ipv4Addr = Ipv4Addr::new(192, 168, 100, 2);
const DEVICE_IP: Ipv4Addr = Ipv4Addr::new(192, 168, 100, 50);

/// A `/32` route for `addr` out of [`OIF`], in `table` with `scope`/`kind` —
/// the fields that distinguish the kernel's own `local` route from ours.
fn host_route(addr: Ipv4Addr, table: u8, scope: RouteScope, kind: RouteType) -> RouteMessage {
    let mut msg = RouteMessage::default();
    msg.header = RouteHeader {
        address_family: AddressFamily::Inet,
        destination_prefix_length: 32,
        table,
        scope,
        kind,
        ..RouteHeader::default()
    };
    msg.attributes = vec![
        RouteAttribute::Destination(RouteAddress::Inet(addr)),
        RouteAttribute::Oif(OIF),
        RouteAttribute::Table(u32::from(table)),
    ];
    msg
}

/// What `add_host_route` installs: table `main`, scope `link`, unicast.
fn our_host_route(addr: Ipv4Addr) -> RouteMessage {
    host_route(addr, RT_TABLE_MAIN, RouteScope::Link, RouteType::Unicast)
}

/// The kernel's own route making an address on this box deliver locally:
/// table `local` (255), scope `host`, type `local`.
fn kernel_local_route(addr: Ipv4Addr) -> RouteMessage {
    host_route(addr, RT_TABLE_LOCAL, RouteScope::Host, RouteType::Local)
}

/// Regression test for #886.
///
/// `remove_host_route` used to match on `/32` + destination + oif alone, and
/// the kernel's `local` route for an address on this box matches all three. A
/// device row holding the daemon's own LAN IP therefore made it delete the
/// route that delivers its own address to itself: the Pi kept answering ARP but
/// blackholed every packet to it, replying `ICMP destination host unreachable`
/// sourced from its own address. Total admin lockout over every path, only
/// cleared by a reboot (which re-creates local routes on address assignment).
#[test]
fn kernel_local_route_for_our_own_ip_is_never_removable() {
    assert!(
        !is_removable_host_route(&kernel_local_route(LAN_IP), LAN_IP, OIF),
        "the kernel's local-table route for the daemon's own IP must never be \
         treated as a removable host route (#886)"
    );
}

/// The same protection for any address on the box, not just the LAN IP.
#[test]
fn kernel_local_route_is_never_removable_for_any_address() {
    assert!(!is_removable_host_route(
        &kernel_local_route(DEVICE_IP),
        DEVICE_IP,
        OIF
    ));
}

#[test]
fn our_own_host_route_is_removable() {
    assert!(is_removable_host_route(
        &our_host_route(DEVICE_IP),
        DEVICE_IP,
        OIF
    ));
}

#[test]
fn host_route_for_a_different_ip_or_interface_is_not_removable() {
    assert!(
        !is_removable_host_route(
            &our_host_route(DEVICE_IP),
            Ipv4Addr::new(192, 168, 100, 51),
            OIF
        ),
        "must not match a different destination"
    );
    assert!(
        !is_removable_host_route(&our_host_route(DEVICE_IP), DEVICE_IP, OIF + 1),
        "must not match a different output interface"
    );
}

#[test]
fn non_host_prefix_is_not_removable() {
    let mut route = our_host_route(DEVICE_IP);
    route.header.destination_prefix_length = 24;
    assert!(!is_removable_host_route(&route, DEVICE_IP, OIF));
}

// -- rule_table: switchback prune must only see `main`-table rules ------------

#[test]
fn rule_table_reads_small_table_from_header() {
    // main (254) fits in the u8 header, so `add_switchback_rule` files it there.
    let mut rule = RuleMessage::default();
    rule.header.family = AddressFamily::Inet;
    rule.header.table = RT_TABLE_MAIN;
    assert_eq!(rule_table(&rule), u32::from(RT_TABLE_MAIN));
}

#[test]
fn rule_table_prefers_wide_table_attribute_over_header() {
    // A table > 255 rides an attribute; it must win over the header so a wide
    // table can't alias onto a small one — and so the switchback prune (which
    // filters on `== main`) never mistakes it for a carve-out.
    let mut rule = RuleMessage::default();
    rule.header.family = AddressFamily::Inet;
    rule.header.table = 0; // RT_TABLE_UNSPEC placeholder for a wide table
    rule.attributes.push(RuleAttribute::Table(300));
    assert_eq!(rule_table(&rule), 300);
    assert_ne!(rule_table(&rule), u32::from(RT_TABLE_MAIN));
}
