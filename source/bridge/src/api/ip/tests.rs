// Tests for the reserved-IPv4 guard.
// Full-stack IP-update handler tests live in tests/api.rs.

use super::is_reserved_ipv4;

#[test]
fn public_addresses_allowed() {
    for addr in ["1.2.3.4", "8.8.8.8", "203.0.114.1"] {
        let ip: std::net::Ipv4Addr = addr.parse().unwrap();
        assert!(!is_reserved_ipv4(ip), "{addr} should be allowed");
    }
}

#[test]
fn private_addresses_rejected() {
    for addr in [
        "10.0.0.1",
        "172.16.0.1",
        "192.168.1.1",
        "127.0.0.1",
        "169.254.1.1",
        "255.255.255.255",
        "192.0.2.1",    // TEST-NET-1
        "198.51.100.1", // TEST-NET-2
        "203.0.113.1",  // TEST-NET-3
        "0.0.0.0",
        "100.64.0.1", // Shared address space
    ] {
        let ip: std::net::Ipv4Addr = addr.parse().unwrap();
        assert!(is_reserved_ipv4(ip), "{addr} should be reserved");
    }
}
