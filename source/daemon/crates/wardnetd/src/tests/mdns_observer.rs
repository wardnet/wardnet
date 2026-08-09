//! Unit tests for the mDNS observer (issue #1115).
//!
//! Same constraint the advertiser's tests state: nothing here binds real
//! multicast. What is testable in-crate is the pure decision logic — which
//! names get browsed, what gets recorded, and what the repeat-suppression
//! remembers. The behaviour that actually names a device (the ambiguity rule)
//! is covered at the service layer against real SQL, in
//! `wardnetd-services/src/device/tests/identification.rs`.

use std::net::Ipv4Addr;

use wardnetd_data::vendor_catalog;

use crate::mdns_observer::{
    MAX_REMEMBERED_OBSERVATIONS, ObservationCache, bare_service_type, browse_name,
};

#[test]
fn browse_name_qualifies_a_bare_catalog_type() {
    assert_eq!(browse_name("_govee._tcp"), "_govee._tcp.local.");
}

#[test]
fn browse_name_is_idempotent() {
    // A catalog entry written in wire form must not become
    // `_govee._tcp.local..local.` — `mdns-sd` would reject it and the vendor
    // would silently never be observed.
    assert_eq!(browse_name("_govee._tcp.local."), "_govee._tcp.local.");
    assert_eq!(browse_name("_govee._tcp.local"), "_govee._tcp.local.");
}

#[test]
fn every_catalogued_type_produces_a_browsable_name() {
    // The guard that a careless `vendors.toml` edit cannot silently break the
    // browse: `mdns-sd` requires the fully-qualified `._tcp.local.` /
    // `._udp.local.` form, and a rejected browse is a vendor we stop seeing.
    for service_type in vendor_catalog::mdns_service_types() {
        let name = browse_name(service_type);
        assert!(
            name.ends_with("._tcp.local.") || name.ends_with("._udp.local."),
            "catalog type {service_type} yields un-browsable name {name}"
        );
    }
}

#[test]
fn a_browse_result_is_normalised_to_the_catalog_spelling() {
    // Stored signals should read the way `vendors.toml` and the device detail
    // view spell them, not the way they arrive off the wire.
    assert_eq!(bare_service_type("_govee._tcp.local."), "_govee._tcp");
    assert_eq!(
        bare_service_type("_GooglecasT._TCP.local."),
        "_googlecast._tcp"
    );
    assert_eq!(bare_service_type("_hue._tcp"), "_hue._tcp");
}

#[test]
fn a_normalised_browse_result_still_resolves_to_its_vendor() {
    // The observer records the normalised form, so that form is what the
    // catalog has to be able to look up.
    let vendor = vendor_catalog::lookup_mdns_service(&bare_service_type("_govee._tcp.local."));
    assert_eq!(vendor, Some("Govee"));
}

#[test]
fn the_cache_suppresses_a_repeat_announcement() {
    // mDNS re-announces on a timer. Without this, every announcement is a
    // write for a fact already recorded.
    let ip: Ipv4Addr = "192.168.1.42".parse().unwrap();
    let mut cache = ObservationCache::default();

    assert!(!cache.contains(ip, "_govee._tcp"));
    cache.remember(ip, "_govee._tcp");
    assert!(cache.contains(ip, "_govee._tcp"));
}

#[test]
fn the_cache_keys_on_both_address_and_service_type() {
    // A device advertising a second service, and a second device advertising
    // the same service, are both new observations.
    let first: Ipv4Addr = "192.168.1.42".parse().unwrap();
    let second: Ipv4Addr = "192.168.1.43".parse().unwrap();
    let mut cache = ObservationCache::default();

    cache.remember(first, "_govee._tcp");

    assert!(!cache.contains(first, "_airplay._tcp"));
    assert!(!cache.contains(second, "_govee._tcp"));
}

#[test]
fn the_cache_is_bounded() {
    // A hostile responder cycling service types must not grow the set without
    // limit. Overflow clears rather than evicting: the only cost is a
    // redundant upsert on the next announcement.
    let ip: Ipv4Addr = "192.168.1.42".parse().unwrap();
    let mut cache = ObservationCache::default();

    for n in 0..=MAX_REMEMBERED_OBSERVATIONS {
        cache.remember(ip, &format!("_service{n}._tcp"));
    }

    assert!(
        cache.len() <= MAX_REMEMBERED_OBSERVATIONS,
        "cache grew past its bound: {}",
        cache.len()
    );
}
