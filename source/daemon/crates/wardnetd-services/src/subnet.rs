//! Small IPv4-subnet helpers shared by the DHCP scope resolver and the Network-
//! Zone L3 enforcer (issue #737).
//!
//! Both derive the same facts from a zone subnet — the Wardnet gateway alias, the
//! DHCP pool bounds, and a canonical `network/prefix` string — so the arithmetic
//! lives here once instead of being hand-rolled at each site.

use std::net::Ipv4Addr;

use ipnetwork::Ipv4Network;

/// First host of a subnet (the Wardnet gateway alias): network + 1.
#[must_use]
pub fn gateway_for(net: Ipv4Network) -> Ipv4Addr {
    Ipv4Addr::from(u32::from(net.network()) + 1)
}

/// DHCP pool bounds within a subnet: (network+10 ..= broadcast-6). Returns None
/// when the subnet is too small to yield a usable pool (start >= end).
#[must_use]
pub fn pool_bounds(net: Ipv4Network) -> Option<(Ipv4Addr, Ipv4Addr)> {
    let start = u32::from(net.network()) + 10;
    let end = u32::from(net.broadcast()).saturating_sub(6);
    if start >= end {
        None
    } else {
        Some((Ipv4Addr::from(start), Ipv4Addr::from(end)))
    }
}

/// Canonical "network/prefix" string (host bits cleared).
///
/// `Ipv4Network`'s `Display` keeps the host bits as-constructed
/// (`192.168.1.1/24`), so we normalise to the network address to make
/// cross-subnet pairs comparable and nftables matches canonical.
#[must_use]
pub fn canonical_cidr(net: Ipv4Network) -> String {
    format!("{}/{}", net.network(), net.prefix())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gateway_is_first_host() {
        let net: Ipv4Network = "10.44.0.0/24".parse().unwrap();
        assert_eq!(gateway_for(net), Ipv4Addr::new(10, 44, 0, 1));
    }

    #[test]
    fn pool_bounds_for_slash24() {
        let net: Ipv4Network = "10.44.0.0/24".parse().unwrap();
        assert_eq!(
            pool_bounds(net),
            Some((Ipv4Addr::new(10, 44, 0, 10), Ipv4Addr::new(10, 44, 0, 249)))
        );
    }

    #[test]
    fn pool_bounds_none_for_slash30() {
        let net: Ipv4Network = "10.44.0.0/30".parse().unwrap();
        assert_eq!(pool_bounds(net), None);
    }

    #[test]
    fn canonical_cidr_clears_host_bits() {
        let net: Ipv4Network = "10.44.1.5/24".parse().unwrap();
        assert_eq!(canonical_cidr(net), "10.44.1.0/24");
    }
}
