use crate::net::{is_private_ip, is_reserved_ipv4, is_rfc1918_subnet};
use std::net::Ipv4Addr;

#[test]
fn rfc1918_subnet_requires_the_whole_range_inside_a_block() {
    let p = |s: &str, n: u8| is_rfc1918_subnet(s.parse::<Ipv4Addr>().unwrap(), n);
    // Fully-contained subnets.
    assert!(p("10.44.0.0", 24));
    assert!(p("192.168.1.0", 24));
    assert!(p("172.16.0.0", 12)); // the canonical /12
    assert!(p("10.0.0.0", 15)); // within 10/8
    // Straddlers: private base, range spills into public space.
    assert!(!p("192.168.0.0", 15)); // → 192.169.x
    assert!(!p("172.16.0.0", 11)); // → 172.0–15 / 172.32+
    assert!(!p("10.0.0.0", 7)); // → 11.x
    // Plain public.
    assert!(!p("8.8.0.0", 16));
}

#[test]
fn reserved_ipv4_covers_private_cgnat_and_special() {
    for ip in [
        "10.0.0.1",
        "172.16.5.4",
        "192.168.1.1",
        "127.0.0.1",
        "169.254.1.1",
        "100.64.0.1", // RFC 6598 CGNAT
        "100.127.255.255",
        "255.255.255.255", // broadcast
        "192.0.2.1",       // documentation
        "0.0.0.0",
    ] {
        assert!(is_reserved_ipv4(ip.parse().unwrap()), "{ip} reserved");
    }
}

#[test]
fn reserved_ipv4_allows_public() {
    for ip in ["1.1.1.1", "8.8.8.8", "93.184.216.34", "100.128.0.1"] {
        assert!(!is_reserved_ipv4(ip.parse().unwrap()), "{ip} public");
    }
}

#[test]
fn private_ip_v6_and_mapped_v4() {
    assert!(is_private_ip("::1".parse().unwrap()));
    assert!(is_private_ip("fd00::1".parse().unwrap())); // unique-local
    assert!(is_private_ip("fe80::1".parse().unwrap())); // link-local
    // IPv4-mapped private address must be caught via the embedded v4.
    assert!(is_private_ip("::ffff:192.168.1.1".parse().unwrap()));
    assert!(is_private_ip("::ffff:100.64.0.1".parse().unwrap()));
    // Public addresses pass.
    assert!(!is_private_ip("2606:4700:4700::1111".parse().unwrap()));
    assert!(!is_private_ip("::ffff:8.8.8.8".parse().unwrap()));
}
