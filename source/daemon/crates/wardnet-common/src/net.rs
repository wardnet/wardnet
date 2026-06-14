//! Shared IP-address classification.
//!
//! Two subsystems need to know whether an address is private / reserved /
//! non-globally-routable: the DDNS SSRF guard (never publish a non-public
//! IP) and DNS rebinding protection (reject public domains that resolve to
//! internal addresses). Keep one definition so coverage can't drift.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

/// `true` when `addr` is private, loopback, link-local, broadcast,
/// documentation, shared/CGNAT (RFC 6598 `100.64.0.0/10`), or
/// unspecified — i.e. not a globally routable unicast IPv4 address.
#[must_use]
pub fn is_reserved_ipv4(addr: Ipv4Addr) -> bool {
    addr.is_private()
        || addr.is_loopback()
        || addr.is_link_local()
        || addr.is_broadcast()
        || addr.is_documentation()
        || addr.is_unspecified()
        || {
            // Shared address space (RFC 6598): 100.64.0.0/10.
            let octets = addr.octets();
            octets[0] == 100 && (octets[1] & 0b1100_0000) == 64
        }
}

/// `true` when `ip` is a private / reserved / non-public address (v4 or
/// v6). IPv6 covers loopback, unspecified, unique-local (`fc00::/7`),
/// link-local (`fe80::/10`), and **IPv4-mapped** addresses
/// (`::ffff:0:0/96`) — the embedded IPv4 is classified with
/// [`is_reserved_ipv4`] so `::ffff:192.168.1.1` is caught.
#[must_use]
pub fn is_private_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_reserved_ipv4(v4),
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return is_reserved_ipv4(v4);
            }
            v6.is_loopback()
                || v6.is_unspecified()
                || is_unique_local_v6(v6)
                || is_link_local_v6(v6)
        }
    }
}

/// IPv6 unique-local addresses, `fc00::/7` (`Ipv6Addr::is_unique_local`
/// is still unstable, so match the prefix directly).
fn is_unique_local_v6(addr: Ipv6Addr) -> bool {
    (addr.segments()[0] & 0xfe00) == 0xfc00
}

/// IPv6 link-local addresses, `fe80::/10`.
fn is_link_local_v6(addr: Ipv6Addr) -> bool {
    (addr.segments()[0] & 0xffc0) == 0xfe80
}

#[cfg(test)]
mod tests {
    use super::{is_private_ip, is_reserved_ipv4};

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
}
