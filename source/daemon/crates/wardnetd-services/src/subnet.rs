//! Small IPv4-subnet helpers shared by the DHCP scope resolver and the Network-
//! Zone L3 enforcer (issue #737).
//!
//! Both derive the same facts from a zone subnet — the Wardnet gateway alias, the
//! DHCP pool bounds, and a canonical `network/prefix` string — so the arithmetic
//! lives here once instead of being hand-rolled at each site.

use std::net::Ipv4Addr;

use ipnetwork::Ipv4Network;
use uuid::Uuid;
use wardnet_common::network_zone::NetworkZone;

/// First host of a subnet (the Wardnet gateway alias): network + 1.
#[must_use]
pub fn gateway_for(net: Ipv4Network) -> Ipv4Addr {
    Ipv4Addr::from(u32::from(net.network()) + 1)
}

/// Parse every zone's configured subnet CIDR, paired with its zone id.
///
/// A zone whose CIDR fails to parse is logged and skipped rather than failing
/// the whole batch — write-time validation
/// (`NetworkZoneServiceImpl::validate_targets_and_subnet`) already rejects
/// unparseable/non-private/oversized CIDRs, so this is defense-in-depth
/// against direct DB edits or older data, not an expected runtime path.
///
/// Shared by device discovery's trusted-subnet cache and zone enforcement's
/// gateway/isolation checks, so the "which subnets does this zone claim"
/// answer can't drift between the two.
#[must_use]
pub fn parse_zone_subnets(zones: &[NetworkZone]) -> Vec<(Uuid, Ipv4Network)> {
    zones
        .iter()
        .filter_map(|zone| {
            let subnet = zone.subnet.as_ref()?;
            match subnet.cidr.parse::<Ipv4Network>() {
                Ok(net) => Some((zone.id, net)),
                Err(e) => {
                    tracing::warn!(
                        zone_id = %zone.id,
                        zone = %zone.name,
                        cidr = %subnet.cidr,
                        error = %e,
                        "zone {zone} has an unparseable subnet CIDR {cidr}; skipping it",
                        zone = zone.name,
                        cidr = subnet.cidr,
                    );
                    None
                }
            }
        })
        .collect()
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
