//! Curated vendor catalog — the single extension point for teaching Wardnet
//! about a manufacturer (issue #1099).
//!
//! Every identification signal resolves through this one table: OUI overrides
//! for blocks the IEEE lists as `Private`, TCP ports an admin-triggered probe
//! may knock on, mDNS service types, and DHCP option-60 vendor class strings.
//! Adding a manufacturer is an edit to `data/vendors.toml` and nothing else.
//!
//! The catalog is embedded at compile time but parsed on first use. A malformed
//! edit therefore fails the `catalog_parses` test rather than the build — see
//! the test module, which exists precisely to keep that failure in CI.

use std::collections::HashMap;
use std::sync::LazyLock;

use serde::Deserialize;
use wardnet_common::device::DeviceSignalKind;

/// The catalog source, embedded so the daemon needs no data file at runtime.
const VENDORS_TOML: &str = include_str!("../data/vendors.toml");

/// One vendor's identifying marks, as written in `vendors.toml`.
#[derive(Debug, Deserialize)]
pub struct Vendor {
    pub name: String,
    /// OUI prefixes (6 hex digits, no separators) whose IEEE listing is the
    /// placeholder `Private`. Matches are lower-confidence by construction —
    /// see [`ManufacturerSource::Catalog`](wardnet_common::device::ManufacturerSource::Catalog).
    #[serde(default)]
    pub oui_override: Vec<String>,
    /// Ports that answer on this vendor's devices. Contacted only by an
    /// explicit admin-triggered probe, never in the background.
    #[serde(default)]
    pub tcp_ports: Vec<u16>,
    #[serde(default)]
    pub mdns_services: Vec<String>,
    /// DHCP option 60 vendor-class-identifier substrings.
    #[serde(default)]
    pub vendor_class: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct CatalogFile {
    #[serde(default)]
    vendor: Vec<Vendor>,
}

/// Lookup indexes built once from the catalog.
struct Catalog {
    vendors: Vec<Vendor>,
    /// OUI prefix (3 bytes) -> index into `vendors`.
    by_oui: HashMap<[u8; 3], usize>,
    /// mDNS service type (lowercased) -> index into `vendors`.
    by_mdns: HashMap<String, usize>,
    /// TCP port -> index into `vendors`.
    by_port: HashMap<u16, usize>,
}

static CATALOG: LazyLock<Catalog> = LazyLock::new(|| {
    // A malformed catalog is a programming error in a file we ship, not a
    // runtime condition to degrade around — the `catalog_parses` test makes
    // sure this is never reached in a released build.
    let parsed: CatalogFile =
        toml::from_str(VENDORS_TOML).expect("vendors.toml is malformed; see wardnetd-data/data");

    let mut by_oui = HashMap::new();
    let mut by_mdns = HashMap::new();
    let mut by_port = HashMap::new();

    for (index, vendor) in parsed.vendor.iter().enumerate() {
        for prefix in &vendor.oui_override {
            if let Some(bytes) = parse_oui(prefix) {
                by_oui.insert(bytes, index);
            }
        }
        for service in &vendor.mdns_services {
            by_mdns.insert(service.to_lowercase(), index);
        }
        for port in &vendor.tcp_ports {
            by_port.insert(*port, index);
        }
    }

    Catalog {
        vendors: parsed.vendor,
        by_oui,
        by_mdns,
        by_port,
    }
});

/// Parse a 6-hex-digit OUI prefix (`"5CE753"`) into its three bytes.
pub(crate) fn parse_oui(prefix: &str) -> Option<[u8; 3]> {
    if prefix.len() != 6 {
        return None;
    }
    Some([
        u8::from_str_radix(&prefix[0..2], 16).ok()?,
        u8::from_str_radix(&prefix[2..4], 16).ok()?,
        u8::from_str_radix(&prefix[4..6], 16).ok()?,
    ])
}

/// Vendor name for a MAC whose IEEE listing is `Private`, if we curate one.
///
/// Returns a *guess*, not a fact: callers must record it as
/// `ManufacturerSource::Catalog` so the UI hedges it. Takes a normalised MAC
/// (`"aa:bb:cc:dd:ee:ff"`).
#[must_use]
pub fn lookup_oui_override(mac: &str) -> Option<&'static str> {
    let parts: Vec<&str> = mac.split(':').collect();
    if parts.len() < 3 {
        return None;
    }
    let prefix = [
        u8::from_str_radix(parts[0], 16).ok()?,
        u8::from_str_radix(parts[1], 16).ok()?,
        u8::from_str_radix(parts[2], 16).ok()?,
    ];
    CATALOG
        .by_oui
        .get(&prefix)
        .map(|&i| CATALOG.vendors[i].name.as_str())
}

/// Vendor that advertises the given mDNS service type (e.g. `_govee._tcp`).
#[must_use]
pub fn lookup_mdns_service(service: &str) -> Option<&'static str> {
    // Strip the trailing `.local.` domain mDNS browse results carry, and any
    // trailing dot, so callers can pass a raw service type straight through.
    let key = service
        .trim_end_matches('.')
        .trim_end_matches(".local")
        .to_lowercase();
    CATALOG
        .by_mdns
        .get(&key)
        .map(|&i| CATALOG.vendors[i].name.as_str())
}

/// Vendor whose DHCP option-60 vendor class the given string contains.
///
/// Substring rather than equality: option 60 values are typically a brand token
/// embedded in a longer product/firmware string.
#[must_use]
pub fn lookup_vendor_class(value: &str) -> Option<&'static str> {
    let lower = value.to_lowercase();
    CATALOG.vendors.iter().find_map(|v| {
        v.vendor_class
            .iter()
            .any(|c| lower.contains(&c.to_lowercase()))
            .then_some(v.name.as_str())
    })
}

/// Vendor associated with a TCP port that answered during a probe.
#[must_use]
pub fn lookup_tcp_port(port: u16) -> Option<&'static str> {
    CATALOG
        .by_port
        .get(&port)
        .map(|&i| CATALOG.vendors[i].name.as_str())
}

/// Resolve an observed signal to a vendor, dispatching on the signal's kind.
///
/// The one place that decides which signal kinds carry vendor information, so
/// the recording service and the mock's demo seed cannot disagree about what
/// counts as an inferred match.
///
/// Returns `None` for kinds that carry no vendor information on their own.
/// Option 55 (the parameter-request list) is deliberately one of them: its
/// *ordering* is a device-class fingerprint, but matching that fingerprint
/// needs a corpus we do not ship, so we store the observation now and can
/// interpret it later without a second capture pass.
#[must_use]
pub fn lookup_signal(kind: DeviceSignalKind, value: &str) -> Option<&'static str> {
    match kind {
        DeviceSignalKind::MdnsService => lookup_mdns_service(value),
        DeviceSignalKind::DhcpVendorClass => lookup_vendor_class(value),
        DeviceSignalKind::ProbedPort => value.parse::<u16>().ok().and_then(lookup_tcp_port),
        DeviceSignalKind::DhcpHostname | DeviceSignalKind::DhcpParamList => None,
    }
}

/// Every vendor entry, for tests that need to assert properties of the catalog
/// as a whole (e.g. that its lookup keys do not collide).
#[must_use]
pub fn vendors() -> &'static [Vendor] {
    &CATALOG.vendors
}

/// Every port the catalog knows about — the complete set an identification
/// probe may contact, and by construction its upper bound.
#[must_use]
pub fn probe_ports() -> Vec<u16> {
    let mut ports: Vec<u16> = CATALOG.by_port.keys().copied().collect();
    ports.sort_unstable();
    ports
}

/// Every mDNS service type the catalog knows, sorted for a deterministic
/// browse order.
///
/// The mDNS observer (issue #1115) browses exactly this set, so teaching
/// Wardnet to recognise a new vendor's advertisement stays a `vendors.toml`
/// edit rather than a code change — the property decision 3 of ADR 0025
/// asserts for every other signal kind.
///
/// Entries are the bare service types as written in the catalog
/// (`_govee._tcp`); browsing wants the fully-qualified `_govee._tcp.local.`
/// form, which the observer appends.
#[must_use]
pub fn mdns_service_types() -> Vec<&'static str> {
    let mut types: Vec<&'static str> = CATALOG.by_mdns.keys().map(String::as_str).collect();
    types.sort_unstable();
    types
}
