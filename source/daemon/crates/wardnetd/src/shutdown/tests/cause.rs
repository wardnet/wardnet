//! Which shutdown causes remove the kernel state the daemon owns, and which
//! `WireGuard` interfaces count as ours.

use crate::shutdown::{ShutdownCause, wardnet_tunnel_interfaces};

#[test]
fn signal_tears_down_runtime_state() {
    assert!(ShutdownCause::Signal.tears_down_runtime_state());
}

#[test]
fn restart_leaves_runtime_state_in_place() {
    // A self-initiated restart hands over to a replacement process. Tunnels
    // have no synchronous boot reconcile, so removing them here would black
    // out the user's VPN until the next tunnel-monitor tick.
    assert!(!ShutdownCause::Restart.tears_down_runtime_state());
}

#[test]
fn selects_wardnet_tunnel_interfaces_by_prefix() {
    let all = vec![
        "wg_ward0".to_owned(),
        "wg_ward12".to_owned(),
        "wg0".to_owned(),
        "utun3".to_owned(),
    ];

    assert_eq!(
        wardnet_tunnel_interfaces(all),
        vec!["wg_ward0".to_owned(), "wg_ward12".to_owned()]
    );
}

#[test]
fn leaves_foreign_wireguard_interfaces_alone() {
    // `TunnelInterface::list` enumerates every WireGuard device on the host.
    // Deleting one the user created themselves would be a serious overreach.
    let all = vec![
        "wg0".to_owned(),
        "corp-vpn".to_owned(),
        "wgward0".to_owned(), // no underscore: not ours
    ];

    assert!(wardnet_tunnel_interfaces(all).is_empty());
}

#[test]
fn excludes_the_inbound_server_from_the_tunnel_sweep() {
    // `wg_wardin0` shares the `wg_ward` prefix by design, but it is torn down
    // through `tear_down_server`, not the outbound-tunnel sweep.
    let all = vec!["wg_ward0".to_owned(), "wg_wardin0".to_owned()];

    assert_eq!(wardnet_tunnel_interfaces(all), vec!["wg_ward0".to_owned()]);
}

#[test]
fn tolerates_a_host_with_no_wireguard_devices() {
    assert!(wardnet_tunnel_interfaces(Vec::new()).is_empty());
}
