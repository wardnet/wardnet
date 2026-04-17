//! Tests for the `AppState` struct -- verifying accessors and cloneability.

use super::stubs::test_app_state;

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
