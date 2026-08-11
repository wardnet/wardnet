//! Tests for the `AppState` struct -- verifying accessors and cloneability.

use super::stubs::test_app_state;
use wardnet_common::device::DeviceSignalKind;

#[test]
fn accessors_return_correct_types() {
    let state = test_app_state();

    // Exercise every accessor to confirm they compile and run without panic.
    let _ = state.auth_service();
    let _ = state.device_service();
    let _ = state.dhcp_service();
    let _ = state.dns_service();
    let _ = state.discovery_service();
    let _ = state.log_service();
    let _ = state.provider_service();
    let _ = state.routing_service();
    let _ = state.system_service();
    let _ = state.tunnel_service();
    let _ = state.event_publisher();
    let _ = state.dhcp_server();
    let _ = state.dns_server();
    let _ = state.device_identification_service();
}

/// The identification service defaults to a no-op until production or the mock
/// injects the live one (issue #1099). Reads must return *no signals* rather
/// than an error: a device detail page still has to render on a build that has
/// not wired identification, and "nothing observed" is already a first-class
/// state in that view.
#[tokio::test]
async fn default_identification_service_is_a_silent_no_op() {
    let state = test_app_state();
    let svc = state.device_identification_service();

    assert!(
        svc.record_signal("dev-1", DeviceSignalKind::MdnsService, "_govee._tcp")
            .await
            .is_ok()
    );
    assert!(
        svc.record_signal_for_mac(
            "aa:bb:cc:dd:ee:01",
            DeviceSignalKind::DhcpHostname,
            "some-host"
        )
        .await
        .is_ok()
    );
    assert!(
        svc.record_signal_for_ip(
            "192.168.1.10".parse().unwrap(),
            DeviceSignalKind::MdnsService,
            "_googlecast._tcp.local."
        )
        .await
        .is_ok()
    );
    assert_eq!(svc.reconcile_from_catalog().await.unwrap(), 0);
    assert!(svc.signals_for("dev-1").await.unwrap().is_empty());
}

#[test]
fn clone_shares_inner_state() {
    let state = test_app_state();
    let cloned = state.clone();

    // Both clones should return the same system service version (sanity check).
    assert_eq!(
        state.system_service().version(),
        cloned.system_service().version()
    );
}
