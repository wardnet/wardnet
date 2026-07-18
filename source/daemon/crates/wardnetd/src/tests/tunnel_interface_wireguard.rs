use crate::tunnel_interface_wireguard::{
    PeerStatsInput, WireGuardTunnelInterface, aggregate_peer_stats, is_interface_absent_error,
};
use std::time::{Duration, SystemTime};
use wardnetd_services::tunnel::interface::TunnelInterface;
use wireguard_control::{Backend, Device, DeviceUpdate, InterfaceName, Key, PeerConfigBuilder};

#[test]
fn aggregate_empty_peers() {
    let stats = aggregate_peer_stats(&[]);

    assert_eq!(stats.bytes_tx, 0);
    assert_eq!(stats.bytes_rx, 0);
    assert!(stats.last_handshake.is_none());
}

#[test]
fn aggregate_single_peer() {
    let handshake = SystemTime::UNIX_EPOCH + Duration::from_secs(1000);
    let peers = vec![PeerStatsInput {
        tx_bytes: 100,
        rx_bytes: 200,
        last_handshake_time: Some(handshake),
    }];

    let stats = aggregate_peer_stats(&peers);

    assert_eq!(stats.bytes_tx, 100);
    assert_eq!(stats.bytes_rx, 200);
    assert_eq!(
        stats.last_handshake,
        Some(chrono::DateTime::<chrono::Utc>::from(handshake))
    );
}

#[test]
fn aggregate_multiple_peers_sums_bytes() {
    let peers = vec![
        PeerStatsInput {
            tx_bytes: 100,
            rx_bytes: 200,
            last_handshake_time: None,
        },
        PeerStatsInput {
            tx_bytes: 300,
            rx_bytes: 400,
            last_handshake_time: None,
        },
    ];

    let stats = aggregate_peer_stats(&peers);

    assert_eq!(stats.bytes_tx, 400);
    assert_eq!(stats.bytes_rx, 600);
}

#[test]
fn aggregate_picks_latest_handshake() {
    let earlier = SystemTime::UNIX_EPOCH + Duration::from_secs(1000);
    let later = SystemTime::UNIX_EPOCH + Duration::from_secs(2000);

    let peers = vec![
        PeerStatsInput {
            tx_bytes: 0,
            rx_bytes: 0,
            last_handshake_time: Some(earlier),
        },
        PeerStatsInput {
            tx_bytes: 0,
            rx_bytes: 0,
            last_handshake_time: Some(later),
        },
    ];

    let stats = aggregate_peer_stats(&peers);

    assert_eq!(
        stats.last_handshake,
        Some(chrono::DateTime::<chrono::Utc>::from(later))
    );
}

#[test]
fn aggregate_ignores_none_handshakes() {
    let handshake = SystemTime::UNIX_EPOCH + Duration::from_secs(1000);

    let peers = vec![
        PeerStatsInput {
            tx_bytes: 0,
            rx_bytes: 0,
            last_handshake_time: Some(handshake),
        },
        PeerStatsInput {
            tx_bytes: 0,
            rx_bytes: 0,
            last_handshake_time: None,
        },
    ];

    let stats = aggregate_peer_stats(&peers);

    assert_eq!(
        stats.last_handshake,
        Some(chrono::DateTime::<chrono::Utc>::from(handshake))
    );
}

/// When the later handshake comes first in the array, the aggregation should
/// still pick it (testing the `prev > hs => prev` branch).
#[test]
fn aggregate_picks_latest_handshake_when_later_comes_first() {
    let earlier = SystemTime::UNIX_EPOCH + Duration::from_secs(1000);
    let later = SystemTime::UNIX_EPOCH + Duration::from_secs(2000);

    // later comes first in the array, then earlier.
    let peers = vec![
        PeerStatsInput {
            tx_bytes: 10,
            rx_bytes: 20,
            last_handshake_time: Some(later),
        },
        PeerStatsInput {
            tx_bytes: 30,
            rx_bytes: 40,
            last_handshake_time: Some(earlier),
        },
    ];

    let stats = aggregate_peer_stats(&peers);

    assert_eq!(stats.bytes_tx, 40);
    assert_eq!(stats.bytes_rx, 60);
    assert_eq!(
        stats.last_handshake,
        Some(chrono::DateTime::<chrono::Utc>::from(later)),
        "should pick the later handshake even when it comes first in the array"
    );
}

/// Three peers with mixed handshake times to exercise the aggregation more fully.
#[test]
fn aggregate_three_peers_with_mixed_handshakes() {
    let t1 = SystemTime::UNIX_EPOCH + Duration::from_secs(100);
    let t2 = SystemTime::UNIX_EPOCH + Duration::from_mins(5);
    let t3 = SystemTime::UNIX_EPOCH + Duration::from_secs(200);

    let peers = vec![
        PeerStatsInput {
            tx_bytes: 1,
            rx_bytes: 2,
            last_handshake_time: Some(t1),
        },
        PeerStatsInput {
            tx_bytes: 3,
            rx_bytes: 4,
            last_handshake_time: Some(t2),
        },
        PeerStatsInput {
            tx_bytes: 5,
            rx_bytes: 6,
            last_handshake_time: Some(t3),
        },
    ];

    let stats = aggregate_peer_stats(&peers);

    assert_eq!(stats.bytes_tx, 9);
    assert_eq!(stats.bytes_rx, 12);
    assert_eq!(
        stats.last_handshake,
        Some(chrono::DateTime::<chrono::Utc>::from(t2)),
        "t2 (300) is the latest"
    );
}

#[test]
fn absent_error_enodev_is_absent() {
    let err = std::io::Error::from_raw_os_error(libc::ENODEV);
    assert!(is_interface_absent_error(&err));
}

#[test]
fn absent_error_enoent_is_absent() {
    let err = std::io::Error::from_raw_os_error(libc::ENOENT);
    assert!(is_interface_absent_error(&err));
}

/// A `NotFound` error without a raw OS errno (e.g. synthesized by the
/// userspace backend) still counts as "interface absent".
#[test]
fn absent_error_not_found_kind_without_errno_is_absent() {
    let err = std::io::Error::new(std::io::ErrorKind::NotFound, "no such socket");
    assert!(is_interface_absent_error(&err));
}

/// A stale userspace control socket with no `wireguard-go` listening surfaces
/// as `ECONNREFUSED` — the interface is effectively dead, so it counts as
/// absent (the old empty-`apply` path returned Ok here too).
#[test]
fn absent_error_econnrefused_is_absent() {
    let err = std::io::Error::from_raw_os_error(libc::ECONNREFUSED);
    assert!(is_interface_absent_error(&err));
}

/// A permission error is a real failure, not an absent interface — `remove()`
/// must propagate it instead of pretending the removal was a no-op.
#[test]
fn absent_error_eperm_is_not_absent() {
    let err = std::io::Error::from_raw_os_error(libc::EPERM);
    assert!(!is_interface_absent_error(&err));
}

#[test]
fn absent_error_other_kind_is_not_absent() {
    let err = std::io::Error::other("netlink payload mismatch");
    assert!(!is_interface_absent_error(&err));
}

/// Full kernel round-trip for `remove()`: stand up a real `WireGuard` interface
/// with a configured private key and peer, then assert `remove()` actually
/// deletes it (not merely re-applies an empty config on top).
///
/// Requires `CAP_NET_ADMIN` and kernel `WireGuard` support, so it is ignored by
/// default; run with `cargo test -p wardnetd -- --ignored kernel_`.
#[tokio::test]
#[ignore = "requires CAP_NET_ADMIN and kernel WireGuard support"]
async fn kernel_remove_deletes_configured_interface() {
    let name = "wgtest832a";
    let iface: InterfaceName = name.parse().unwrap();

    // Configure the interface directly via netlink (what `create()` does,
    // minus the address assignment that needs iproute2).
    let apply_result = DeviceUpdate::new()
        .set_private_key(Key::generate_private())
        .add_peer(PeerConfigBuilder::new(
            &Key::generate_private().get_public(),
        ))
        .apply(&iface, Backend::Kernel);
    if let Err(e) = &apply_result
        && e.raw_os_error() == Some(libc::EOPNOTSUPP)
    {
        eprintln!("skipping: this kernel has no WireGuard support ({e})");
        return;
    }
    apply_result.expect("failed to create kernel wireguard interface (needs CAP_NET_ADMIN)");

    let device = Device::get(&iface, Backend::Kernel).expect("interface should exist after apply");
    assert!(
        device.private_key.is_some(),
        "private key should be configured before removal"
    );
    assert_eq!(device.peers.len(), 1, "peer should be configured");

    WireGuardTunnelInterface
        .remove(name)
        .await
        .expect("remove() should succeed on an existing interface");

    let err =
        Device::get(&iface, Backend::Kernel).expect_err("interface should be gone after remove()");
    assert!(
        is_interface_absent_error(&err),
        "expected a not-found error after removal, got: {err}"
    );
}

/// `remove()` on an interface that does not exist is an idempotent success:
/// no error, nothing to remove.
#[tokio::test]
#[ignore = "requires CAP_NET_ADMIN and kernel WireGuard support"]
async fn kernel_remove_absent_interface_is_idempotent() {
    let name = "wgtest832b";
    let iface: InterfaceName = name.parse().unwrap();
    assert!(
        Device::get(&iface, Backend::Kernel).is_err(),
        "precondition: interface must not exist"
    );

    WireGuardTunnelInterface
        .remove(name)
        .await
        .expect("remove() of an absent interface should be an idempotent no-op");
}

#[test]
fn aggregate_all_none_handshakes() {
    let peers = vec![
        PeerStatsInput {
            tx_bytes: 10,
            rx_bytes: 20,
            last_handshake_time: None,
        },
        PeerStatsInput {
            tx_bytes: 30,
            rx_bytes: 40,
            last_handshake_time: None,
        },
    ];

    let stats = aggregate_peer_stats(&peers);

    assert_eq!(stats.bytes_tx, 40);
    assert_eq!(stats.bytes_rx, 60);
    assert!(stats.last_handshake.is_none());
}
