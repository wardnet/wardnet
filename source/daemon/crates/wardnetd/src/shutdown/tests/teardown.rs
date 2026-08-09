//! `teardown_runtime_state` removes every piece of kernel state the daemon
//! owns, and keeps going when any single step fails.

use std::sync::Arc;

use wardnetd_services::inbound_wg::InboundWgInterface;
use wardnetd_services::routing::FirewallManager;
use wardnetd_services::tunnel::TunnelInterface;

use super::doubles::{
    CallLog, RecordingFirewall, RecordingInboundWg, RecordingTunnelInterface, call_log, recorded,
};
use crate::shutdown::{TunnelTeardown, teardown_runtime_state};

/// Wire the three doubles up as the trait objects `teardown_runtime_state`
/// takes, sharing one call log.
fn backends(
    log: &CallLog,
    firewall: RecordingFirewall,
    tunnels: RecordingTunnelInterface,
) -> (
    Arc<dyn FirewallManager>,
    Arc<dyn TunnelInterface>,
    Arc<dyn InboundWgInterface>,
) {
    (
        Arc::new(firewall),
        Arc::new(tunnels),
        Arc::new(RecordingInboundWg::new(log.clone())),
    )
}

#[tokio::test]
async fn removes_the_table_then_every_tunnel_then_the_inbound_server() {
    let log = call_log();
    let (firewall, tunnels, inbound) = backends(
        &log,
        RecordingFirewall::new(log.clone()),
        RecordingTunnelInterface::new(log.clone(), &["wg_ward0", "wg_ward1", "wg_wardin0", "wg0"]),
    );

    teardown_runtime_state(&firewall, TunnelTeardown::Interface(&tunnels), &inbound).await;

    assert_eq!(
        recorded(&log),
        vec![
            "destroy_wardnet_table",
            "list",
            "remove:wg_ward0",
            "remove:wg_ward1",
            "tear_down_server:wg_wardin0",
        ]
    );
}

#[tokio::test]
async fn never_removes_a_wireguard_interface_we_do_not_own() {
    let log = call_log();
    let (firewall, tunnels, inbound) = backends(
        &log,
        RecordingFirewall::new(log.clone()),
        RecordingTunnelInterface::new(log.clone(), &["wg0", "corp-vpn"]),
    );

    teardown_runtime_state(&firewall, TunnelTeardown::Interface(&tunnels), &inbound).await;

    let calls = recorded(&log);
    assert!(
        !calls.iter().any(|c| c.starts_with("remove:")),
        "expected no interface removals, got {calls:?}"
    );
}

#[tokio::test]
async fn reports_a_firewall_failure_to_the_caller() {
    // `wardnetd uninstall` runs before tracing is initialised, so a log-only
    // contract would let it print "Wardnet has been removed" while the table
    // was still filtering traffic.
    let log = call_log();
    let (firewall, tunnels, inbound) = backends(
        &log,
        RecordingFirewall::failing(log.clone()),
        RecordingTunnelInterface::new(log.clone(), &[]),
    );

    let failures =
        teardown_runtime_state(&firewall, TunnelTeardown::Interface(&tunnels), &inbound).await;

    assert_eq!(failures.len(), 1, "got {failures:?}");
    assert!(failures[0].contains("inet wardnet"), "got {failures:?}");
}

#[tokio::test]
async fn reports_nothing_when_teardown_succeeds() {
    let log = call_log();
    let (firewall, tunnels, inbound) = backends(
        &log,
        RecordingFirewall::new(log.clone()),
        RecordingTunnelInterface::new(log.clone(), &["wg_ward0"]),
    );

    assert!(
        teardown_runtime_state(&firewall, TunnelTeardown::Interface(&tunnels), &inbound)
            .await
            .is_empty()
    );
}

#[tokio::test]
async fn reports_a_failed_enumeration_rather_than_silently_skipping() {
    let log = call_log();
    let (firewall, tunnels, inbound) = backends(
        &log,
        RecordingFirewall::new(log.clone()),
        RecordingTunnelInterface::failing_list(log.clone()),
    );

    let failures =
        teardown_runtime_state(&firewall, TunnelTeardown::Interface(&tunnels), &inbound).await;

    assert_eq!(failures.len(), 1, "got {failures:?}");
    assert!(failures[0].contains("listing"), "got {failures:?}");
}

#[tokio::test]
async fn continues_tearing_down_after_a_firewall_failure() {
    // Shutdown is the wrong place to give up: a failed table delete must not
    // strand the WireGuard interfaces.
    let log = call_log();
    let (firewall, tunnels, inbound) = backends(
        &log,
        RecordingFirewall::failing(log.clone()),
        RecordingTunnelInterface::new(log.clone(), &["wg_ward0"]),
    );

    teardown_runtime_state(&firewall, TunnelTeardown::Interface(&tunnels), &inbound).await;

    assert_eq!(
        recorded(&log),
        vec![
            "destroy_wardnet_table",
            "list",
            "remove:wg_ward0",
            "tear_down_server:wg_wardin0",
        ]
    );
}

#[tokio::test]
async fn still_tears_down_the_inbound_server_when_enumeration_fails() {
    let log = call_log();
    let (firewall, tunnels, inbound) = backends(
        &log,
        RecordingFirewall::new(log.clone()),
        RecordingTunnelInterface::failing_list(log.clone()),
    );

    teardown_runtime_state(&firewall, TunnelTeardown::Interface(&tunnels), &inbound).await;

    assert_eq!(
        recorded(&log),
        vec![
            "destroy_wardnet_table",
            "list",
            "tear_down_server:wg_wardin0",
        ]
    );
}

#[tokio::test]
async fn is_idempotent_on_an_already_clean_host() {
    let log = call_log();
    let (firewall, tunnels, inbound) = backends(
        &log,
        RecordingFirewall::new(log.clone()),
        RecordingTunnelInterface::new(log.clone(), &[]),
    );

    teardown_runtime_state(&firewall, TunnelTeardown::Interface(&tunnels), &inbound).await;
    teardown_runtime_state(&firewall, TunnelTeardown::Interface(&tunnels), &inbound).await;

    // Both passes make the same calls; nothing errors on the second run. This
    // is what lets `wardnetd uninstall` re-run the sweep after a clean stop.
    assert_eq!(
        recorded(&log),
        vec![
            "destroy_wardnet_table",
            "list",
            "tear_down_server:wg_wardin0",
            "destroy_wardnet_table",
            "list",
            "tear_down_server:wg_wardin0",
        ]
    );
}
