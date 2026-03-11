//! Tests for the `AppState` struct -- verifying accessors and cloneability.

use super::stubs::test_app_state;

#[test]
fn accessors_return_correct_types() {
    let state = test_app_state();

    // Exercise every accessor to confirm they compile and run without panic.
    let _ = state.auth_service();
    let _ = state.device_service();
    let _ = state.discovery_service();
    let _ = state.provider_service();
    let _ = state.routing_service();
    let _ = state.system_service();
    let _ = state.tunnel_service();
    let _ = state.event_publisher();
    let _ = state.config();
    let _ = state.started_at();
}

#[test]
fn clone_shares_inner_state() {
    let state = test_app_state();
    let cloned = state.clone();

    // Both should report the same started_at instant.
    assert_eq!(state.started_at(), cloned.started_at());
    assert_eq!(state.config().server.port, cloned.config().server.port);
}
