//! Shutdown tears tunnels down *through the service*, so the recorded status
//! follows the kernel.
//!
//! This is what makes the teardown recoverable. Deleting the interface behind
//! the database's back leaves the tunnel recorded as `Up` with no interface;
//! the monitor then flips it to `Down`, `handle_tunnel_down` strips the routing
//! for every device using it, and nothing ever recreates it — tunnel-routed
//! devices would silently sit on direct WAN after any `systemctl stop`.

use std::sync::Arc;

use wardnet_common::tunnel::TunnelStatus;
use wardnetd_services::inbound_wg::InboundWgInterface;
use wardnetd_services::routing::FirewallManager;
use wardnetd_services::tunnel::TunnelService;

use super::doubles::{
    CallLog, RecordingFirewall, RecordingInboundWg, RecordingTunnelService, call_log, recorded,
    tunnel,
};
use crate::shutdown::{TunnelTeardown, teardown_runtime_state};

fn wire(
    log: &CallLog,
    service: RecordingTunnelService,
) -> (
    Arc<dyn FirewallManager>,
    Arc<dyn TunnelService>,
    Arc<dyn InboundWgInterface>,
) {
    (
        Arc::new(RecordingFirewall::new(log.clone())),
        Arc::new(service),
        Arc::new(RecordingInboundWg::new(log.clone())),
    )
}

#[tokio::test]
async fn tears_down_every_live_tunnel_through_the_service() {
    let log = call_log();
    let tunnels = vec![
        tunnel("wg_ward0", TunnelStatus::Up),
        tunnel("wg_ward1", TunnelStatus::Connecting),
    ];
    let (firewall, service, inbound) =
        wire(&log, RecordingTunnelService::new(log.clone(), tunnels));

    let failures =
        teardown_runtime_state(&firewall, TunnelTeardown::Service(&service), &inbound).await;

    assert!(failures.is_empty(), "got {failures:?}");
    assert_eq!(
        recorded(&log),
        vec![
            "destroy_wardnet_table",
            "list_tunnels",
            "tear_down_internal:wg_ward0:daemon shutdown",
            "tear_down_internal:wg_ward1:daemon shutdown",
            "tear_down_server:wg_wardin0",
        ]
    );
}

#[tokio::test]
async fn leaves_already_down_tunnels_alone() {
    // Nothing to remove, and the status is already what the next boot's
    // on-demand bring-up keys off.
    let log = call_log();
    let tunnels = vec![
        tunnel("wg_ward0", TunnelStatus::Down),
        tunnel("wg_ward1", TunnelStatus::Up),
    ];
    let (firewall, service, inbound) =
        wire(&log, RecordingTunnelService::new(log.clone(), tunnels));

    teardown_runtime_state(&firewall, TunnelTeardown::Service(&service), &inbound).await;

    let calls = recorded(&log);
    assert!(
        !calls.iter().any(|c| c.contains("wg_ward0")),
        "already-down tunnel was torn down again: {calls:?}"
    );
    assert!(calls.iter().any(|c| c.contains("wg_ward1")), "{calls:?}");
}

#[tokio::test]
async fn reports_a_tunnel_that_would_not_tear_down() {
    let log = call_log();
    let tunnels = vec![
        tunnel("wg_ward0", TunnelStatus::Up),
        tunnel("wg_ward1", TunnelStatus::Up),
    ];
    let (firewall, service, inbound) = wire(
        &log,
        RecordingTunnelService::failing_teardown(log.clone(), tunnels, &["wg_ward0"]),
    );

    let failures =
        teardown_runtime_state(&firewall, TunnelTeardown::Service(&service), &inbound).await;

    assert_eq!(failures.len(), 1, "got {failures:?}");
    assert!(failures[0].contains("wg_ward0"), "got {failures:?}");
    // The failure must not stop the rest of the teardown.
    let calls = recorded(&log);
    assert!(calls.iter().any(|c| c.contains("wg_ward1")), "{calls:?}");
    assert!(
        calls.contains(&"tear_down_server:wg_wardin0".to_owned()),
        "{calls:?}"
    );
}

#[tokio::test]
async fn reports_a_failed_listing_and_still_tears_down_the_inbound_server() {
    let log = call_log();
    let (firewall, service, inbound) =
        wire(&log, RecordingTunnelService::failing_list(log.clone()));

    let failures =
        teardown_runtime_state(&firewall, TunnelTeardown::Service(&service), &inbound).await;

    assert_eq!(failures.len(), 1, "got {failures:?}");
    assert!(failures[0].contains("listing tunnels"), "got {failures:?}");
    assert!(
        recorded(&log).contains(&"tear_down_server:wg_wardin0".to_owned()),
        "{:?}",
        recorded(&log)
    );
}

#[tokio::test]
async fn does_nothing_tunnel_shaped_when_there_are_no_tunnels() {
    let log = call_log();
    let (firewall, service, inbound) =
        wire(&log, RecordingTunnelService::new(log.clone(), Vec::new()));

    teardown_runtime_state(&firewall, TunnelTeardown::Service(&service), &inbound).await;

    assert_eq!(
        recorded(&log),
        vec![
            "destroy_wardnet_table",
            "list_tunnels",
            "tear_down_server:wg_wardin0",
        ]
    );
}
