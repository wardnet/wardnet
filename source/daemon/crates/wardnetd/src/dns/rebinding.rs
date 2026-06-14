//! DNS rebinding protection (Stage 4, #218).
//!
//! A DNS-rebinding attack returns a private/internal IP for an attacker
//! controlled *public* domain so a victim browser is tricked into talking
//! to a LAN service. When enabled, the server rejects upstream answers for
//! external domains that resolve to a private address. This check runs only
//! on the default/recursive upstream path — authoritative records,
//! `.lan`, and conditional-forwarding rules legitimately return private
//! IPs and short-circuit earlier in the pipeline, so they never reach here.

use std::net::{IpAddr, Ipv6Addr};

/// Returns `true` if `ip` is a non-public address: RFC1918, loopback,
/// link-local, unspecified (v4/v6), or IPv6 unique-local (`fc00::/7`).
#[must_use]
pub(crate) fn is_private_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_private() || v4.is_loopback() || v4.is_link_local() || v4.is_unspecified()
        }
        IpAddr::V6(v6) => {
            v6.is_loopback()
                || v6.is_unspecified()
                || is_unique_local_v6(v6)
                || is_link_local_v6(v6)
        }
    }
}

/// IPv6 unique-local addresses, `fc00::/7` (`Ipv6Addr::is_unique_local` is
/// still unstable, so match the prefix directly).
fn is_unique_local_v6(addr: Ipv6Addr) -> bool {
    (addr.segments()[0] & 0xfe00) == 0xfc00
}

/// IPv6 link-local addresses, `fe80::/10`.
fn is_link_local_v6(addr: Ipv6Addr) -> bool {
    (addr.segments()[0] & 0xffc0) == 0xfe80
}

#[cfg(test)]
mod tests {
    use super::is_private_ip;

    #[test]
    fn flags_private_v4() {
        for ip in [
            "10.0.0.1",
            "172.16.5.4",
            "192.168.1.1",
            "127.0.0.1",
            "169.254.1.1",
            "0.0.0.0",
        ] {
            assert!(is_private_ip(ip.parse().unwrap()), "{ip} should be private");
        }
    }

    #[test]
    fn allows_public_v4() {
        for ip in ["1.1.1.1", "8.8.8.8", "93.184.216.34"] {
            assert!(!is_private_ip(ip.parse().unwrap()), "{ip} should be public");
        }
    }

    #[test]
    fn flags_private_v6_and_allows_public_v6() {
        assert!(is_private_ip("::1".parse().unwrap()));
        assert!(is_private_ip("fd00::1".parse().unwrap())); // unique-local
        assert!(is_private_ip("fe80::1".parse().unwrap())); // link-local
        assert!(!is_private_ip("2606:4700:4700::1111".parse().unwrap())); // Cloudflare
    }
}
