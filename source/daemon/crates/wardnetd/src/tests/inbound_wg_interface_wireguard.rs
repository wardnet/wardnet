//! Pure-function tests for the inbound `WireGuard` interface impl.
//!
//! Deliberately does NOT touch the kernel / netlink — only the pure
//! `peer_stats_from` mapping is exercised here, matching how
//! `tunnel_interface_wireguard`'s own test file is scoped.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::inbound_wg_interface_wireguard::peer_stats_from;

#[test]
fn maps_fields_and_converts_handshake_time() {
    let key = [7u8; 32];
    let hs = UNIX_EPOCH + Duration::from_secs(1_700_000_000);
    let stats = peer_stats_from(key, 111, 222, Some(hs));

    assert_eq!(stats.public_key, key);
    assert_eq!(stats.bytes_tx, 111);
    assert_eq!(stats.bytes_rx, 222);
    let expected = chrono::DateTime::<chrono::Utc>::from(hs);
    assert_eq!(stats.last_handshake, Some(expected));
}

#[test]
fn absent_handshake_maps_to_none() {
    let stats = peer_stats_from([0u8; 32], 0, 0, None);
    assert!(stats.last_handshake.is_none());
    assert_eq!(stats.bytes_tx, 0);
    assert_eq!(stats.bytes_rx, 0);
}

#[test]
fn now_handshake_round_trips_within_a_second() {
    let now = SystemTime::now();
    let stats = peer_stats_from([1u8; 32], 1, 1, Some(now));
    let got = stats.last_handshake.expect("handshake present");
    let expected = chrono::DateTime::<chrono::Utc>::from(now);
    assert!((got - expected).num_seconds().abs() <= 1);
}
