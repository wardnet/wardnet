//! Tests for the inbound `WireGuard` interface impl: the pure
//! `peer_stats_from` mapping, plus opt-in (`#[ignore]`) kernel round-trips
//! for `tear_down_server`, matching how `tunnel_interface_wireguard`'s own
//! test file is scoped.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::inbound_wg_interface_wireguard::{WireGuardInboundInterface, peer_stats_from};
use crate::wireguard_interface::is_interface_absent_error;
use wardnetd_services::inbound_wg::interface::InboundWgInterface;
use wireguard_control::{Backend, Device, DeviceUpdate, InterfaceName, Key};

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

/// Full kernel round-trip for `tear_down_server()`: stand up a real
/// `WireGuard` interface with a configured private key, then assert the
/// tear-down actually deletes it.
///
/// Requires `CAP_NET_ADMIN` and kernel `WireGuard` support, so it is ignored
/// by default; run with `cargo test -p wardnetd -- --ignored kernel_`.
#[tokio::test]
#[ignore = "requires CAP_NET_ADMIN and kernel WireGuard support"]
async fn kernel_tear_down_server_deletes_interface() {
    let name = "wgtest832c";
    let iface: InterfaceName = name.parse().unwrap();

    let apply_result = DeviceUpdate::new()
        .set_private_key(Key::generate_private())
        .apply(&iface, Backend::Kernel);
    if let Err(e) = &apply_result
        && e.raw_os_error() == Some(libc::EOPNOTSUPP)
    {
        eprintln!("skipping: this kernel has no WireGuard support ({e})");
        return;
    }
    apply_result.expect("failed to create kernel wireguard interface (needs CAP_NET_ADMIN)");
    Device::get(&iface, Backend::Kernel).expect("interface should exist after apply");

    WireGuardInboundInterface
        .tear_down_server(name)
        .await
        .expect("tear_down_server should succeed on an existing interface");

    let err = Device::get(&iface, Backend::Kernel)
        .expect_err("interface should be gone after tear_down_server()");
    assert!(
        is_interface_absent_error(&err),
        "expected a not-found error after tear-down, got: {err}"
    );
}

/// `tear_down_server()` on an interface that does not exist is an idempotent
/// success — the expected steady state when the server was never enabled.
#[tokio::test]
#[ignore = "requires CAP_NET_ADMIN and kernel WireGuard support"]
async fn kernel_tear_down_server_absent_interface_is_idempotent() {
    let name = "wgtest832d";
    let iface: InterfaceName = name.parse().unwrap();
    assert!(
        Device::get(&iface, Backend::Kernel).is_err(),
        "precondition: interface must not exist"
    );

    WireGuardInboundInterface
        .tear_down_server(name)
        .await
        .expect("tear_down_server of an absent interface should be an idempotent no-op");
}
