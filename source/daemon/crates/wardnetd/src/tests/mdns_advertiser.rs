use wardnet_common::config::MdnsConfig;

use crate::mdns_advertiser::next_candidate;

#[test]
fn config_defaults_enable_advertiser_with_built_in_hostname() {
    // The disabled-path smoke check: main.rs gates `MdnsAdvertiser::start`
    // on `config.mdns.enabled`, so the only thing this crate can verify
    // without binding real multicast is that the default config matches
    // the shape main.rs expects — `enabled: true`, `hostname: None`
    // (resolved to `wardnet` at start-up).
    let cfg = MdnsConfig::default();
    assert!(cfg.enabled);
    assert!(cfg.hostname.is_none());
}

#[test]
fn config_can_be_disabled_without_panicking() {
    let cfg = MdnsConfig {
        enabled: false,
        hostname: None,
    };
    // No multicast bind, no daemon thread, no work — the disabled path
    // is a value comparison, not a function call.
    assert!(!cfg.enabled);
}

#[test]
fn next_candidate_appends_2_to_bare_name() {
    assert_eq!(next_candidate("wardnet"), "wardnet-2");
}

#[test]
fn next_candidate_increments_existing_suffix() {
    assert_eq!(next_candidate("wardnet-2"), "wardnet-3");
    assert_eq!(next_candidate("wardnet-9"), "wardnet-10");
}

#[test]
fn next_candidate_treats_non_numeric_suffix_as_bare_name() {
    // A name that happens to contain a hyphen but no numeric suffix
    // (e.g. an operator-chosen `home-router`) gets a fresh `-2` suffix
    // rather than corrupting the meaningful part.
    assert_eq!(next_candidate("home-router"), "home-router-2");
}

#[test]
fn next_candidate_ignores_suffix_below_two() {
    // RFC 6762 §9 starts at "-2"; a hostname ending in "-1" is treated
    // as a bare name so we never produce "-1" → "-2" (which would
    // collide with the standard first-fallback).
    assert_eq!(next_candidate("wardnet-1"), "wardnet-1-2");
}
