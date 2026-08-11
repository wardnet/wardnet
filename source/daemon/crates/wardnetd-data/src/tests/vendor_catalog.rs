use crate::vendor_catalog::{
    lookup_mdns_service, lookup_oui_override, lookup_tcp_port, lookup_vendor_class,
    mdns_service_types, probe_ports, vendors,
};

#[test]
fn catalog_parses() {
    // The catalog is parsed lazily from an embedded TOML file, so a malformed
    // edit would otherwise surface as a runtime panic in the field. This test
    // is the gate that keeps a broken `vendors.toml` from ever shipping —
    // touching the catalog at all forces the parse.
    assert!(
        !probe_ports().is_empty(),
        "catalog parsed but yielded no probe ports"
    );
}

#[test]
fn catalog_keys_are_unique_across_vendors() {
    // The catalog's contract is "adding a manufacturer is a data edit and
    // nothing else". The lookup indexes are HashMaps, so a new entry reusing a
    // port, OUI or mDNS type another vendor already claims would silently
    // disable the earlier mapping with no build or test failure. Assert the
    // keys are distinct so that edit fails loudly instead.
    let mut ouis = Vec::new();
    let mut services = Vec::new();
    let mut ports = Vec::new();
    for vendor in vendors() {
        ouis.extend(vendor.oui_override.iter().map(|s| s.to_uppercase()));
        services.extend(vendor.mdns_services.iter().map(|s| s.to_lowercase()));
        ports.extend(vendor.tcp_ports.iter().copied());
    }

    for (label, mut keys) in [
        ("OUI override", ouis),
        ("mDNS service", services),
        ("TCP port", ports.iter().map(u16::to_string).collect()),
    ] {
        let before = keys.len();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(
            keys.len(),
            before,
            "duplicate {label} key in vendors.toml — the later vendor would \
             silently shadow the earlier one"
        );
    }
}

#[test]
fn govee_private_oui_is_overridden() {
    // 5C-E7-53 is listed to `Private` in the IEEE database, so `lookup_manufacturer`
    // cannot name it. The curated override is the only thing that can — this is
    // the exact device from the report in issue #1099.
    assert_eq!(lookup_oui_override("5c:e7:53:4e:ec:db"), Some("Govee"));
}

#[test]
fn unknown_oui_has_no_override() {
    // Apple's block is public and correctly resolved by the IEEE table, so it
    // must not also carry a curated override.
    assert_eq!(lookup_oui_override("f0:ee:7a:00:11:22"), None);
    assert_eq!(lookup_oui_override("not-a-mac"), None);
}

#[test]
fn mdns_service_identifies_vendor() {
    assert_eq!(lookup_mdns_service("_googlecast._tcp"), Some("Google"));
    assert_eq!(lookup_mdns_service("_govee._tcp"), Some("Govee"));
}

#[test]
fn mdns_lookup_tolerates_browse_formatting() {
    // mDNS browse results arrive fully qualified and with inconsistent casing;
    // the caller should not have to normalise before asking.
    assert_eq!(
        lookup_mdns_service("_googlecast._tcp.local."),
        Some("Google")
    );
    assert_eq!(lookup_mdns_service("_GoogleCast._TCP"), Some("Google"));
}

#[test]
fn unknown_mdns_service_is_none() {
    assert_eq!(lookup_mdns_service("_nosuchservice._tcp"), None);
}

#[test]
fn vendor_class_matches_as_substring() {
    // Option 60 is typically a brand token inside a longer firmware string,
    // so equality would almost never match.
    assert_eq!(
        lookup_vendor_class("ubnt-unifi-ap-6.5.28"),
        Some("Ubiquiti")
    );
    assert_eq!(lookup_vendor_class("UBNT"), Some("Ubiquiti"));
    assert_eq!(lookup_vendor_class("MSFT 5.0"), None);
}

#[test]
fn tcp_port_identifies_vendor() {
    assert_eq!(lookup_tcp_port(4003), Some("Govee"));
    assert_eq!(lookup_tcp_port(6668), Some("Tuya"));
    assert_eq!(lookup_tcp_port(8009), Some("Google"));
    assert_eq!(lookup_tcp_port(22), None);
}

#[test]
fn probe_ports_are_sorted_and_bounded() {
    let ports = probe_ports();
    let mut sorted = ports.clone();
    sorted.sort_unstable();
    assert_eq!(ports, sorted, "probe ports must be deterministic");

    // The probe surface is exactly what the catalog declares. Keeping this
    // small is a policy commitment, not an accident: an admin-triggered probe
    // knocks on every one of these.
    assert!(
        ports.len() < 20,
        "probe surface grew unexpectedly large: {ports:?}"
    );
}

#[test]
fn mdns_service_types_are_sorted_bounded_and_resolvable() {
    let types = mdns_service_types();
    let mut sorted = types.clone();
    sorted.sort_unstable();
    assert_eq!(types, sorted, "browse set must be deterministic");

    // The observer issues one browse per entry, so the set is a passive-traffic
    // commitment the same way `probe_ports` bounds the active-probe surface.
    assert!(
        !types.is_empty(),
        "catalog declares mDNS services but the browse set is empty"
    );
    assert!(
        types.len() < 40,
        "mDNS browse surface grew unexpectedly large: {types:?}"
    );

    // Every type the observer browses must resolve back to a vendor — otherwise
    // the observation is recorded but never names anyone.
    for ty in types {
        assert!(
            lookup_mdns_service(ty).is_some(),
            "browsed service type {ty} does not resolve to a vendor"
        );
    }
}

#[test]
fn parse_oui_rejects_anything_that_is_not_six_hex_digits() {
    // The catalog builder silently skips a malformed `oui_override`, so this
    // guard is what stops a typo in vendors.toml becoming a bogus prefix.
    use crate::vendor_catalog::parse_oui;

    assert_eq!(parse_oui("5CE753"), Some([0x5C, 0xE7, 0x53]));
    assert_eq!(parse_oui("5CE75"), None, "too short");
    assert_eq!(parse_oui("5CE7533"), None, "too long");
    assert_eq!(parse_oui("ZZZZZZ"), None, "not hex");
    assert_eq!(parse_oui(""), None);
}
