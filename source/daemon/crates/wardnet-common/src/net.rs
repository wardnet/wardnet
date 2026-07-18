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

/// Shared human-readable hint naming the RFC 1918 ranges, so the several
/// "must be …" validation errors (zone subnet, DHCP pool start/end, router)
/// don't drift apart.
pub const PRIVATE_RANGE_HINT: &str = "a private range (10.x, 172.16-31.x, or 192.168.x)";

/// `true` when the *entire* IPv4 subnet (`base`/`prefix`, where `base` is the
/// network address) sits inside a single RFC 1918 block: `10/8`, `172.16/12`,
/// or `192.168/16`. Checking only the base is insufficient — a supernet like
/// `192.168.0.0/15` has a private base but straddles into public space; the
/// subnet is fully private only if its prefix is at least the block's and its
/// base masks to the block.
#[must_use]
pub fn is_rfc1918_subnet(base: Ipv4Addr, prefix: u8) -> bool {
    const BLOCKS: [(Ipv4Addr, u8); 3] = [
        (Ipv4Addr::new(10, 0, 0, 0), 8),
        (Ipv4Addr::new(172, 16, 0, 0), 12),
        (Ipv4Addr::new(192, 168, 0, 0), 16),
    ];
    let base_u = u32::from(base);
    BLOCKS.iter().any(|&(net, block_prefix)| {
        if prefix < block_prefix {
            return false;
        }
        let mask = if block_prefix == 0 {
            0
        } else {
            u32::MAX << (32 - block_prefix)
        };
        (base_u & mask) == u32::from(net)
    })
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
