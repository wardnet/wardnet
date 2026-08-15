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
    // A probe against the default service reports an empty surface rather than
    // failing: the identification card renders its "nothing answered" copy from
    // this shape, and a build that has not wired identification must not make
    // the button look broken (issue #1116).
    let probe = svc.probe_device("dev-1").await.unwrap();
    assert!(probe.ports_probed.is_empty());
    assert!(probe.answering_ports.is_empty());
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

/// The default [`UserService`] refuses every call, including the reads.
///
/// The counterpart of `default_identification_service_is_a_silent_no_op`, and
/// deliberately the opposite shape. Recording a device signal is advisory, so
/// a no-op that succeeds is honest. A household directory is not: answering an
/// empty list would render an admin screen showing a working household with
/// nobody in it, indistinguishable from a real empty box and from a wiring
/// bug. The credential paths must fail for a stronger reason still — a `Noop`
/// returning `Ok` from `redeem_enrolment` or `complete_oauth_callback` would
/// be an authentication bypass.
#[tokio::test]
async fn default_user_service_refuses_every_call() {
    let state = test_app_state();
    let svc = state.user_service();
    let id = uuid::Uuid::nil();

    // Reads fail rather than answering emptily.
    assert!(
        svc.list_users().await.is_err(),
        "list_users must not report an empty household"
    );
    assert!(svc.get_user(id).await.is_err());
    assert!(svc.list_credentials(id).await.is_err());
    assert!(svc.list_enrolments(id).await.is_err());
    assert!(svc.available_methods().await.is_err());

    // Credential paths fail: an `Ok` here would be an authentication bypass.
    assert!(svc.redeem_enrolment("token", "password").await.is_err());
    assert!(svc.complete_oauth_callback("state", "code").await.is_err());
    assert!(
        svc.start_oauth(
            wardnetd_services::user::OauthProvider::Google,
            wardnetd_services::user::ReturnTo::Admin,
            false,
        )
        .await
        .is_err()
    );

    // Writes fail too, so a mis-wired state cannot silently discard changes.
    assert!(
        svc.create_user(wardnetd_services::user::NewUser {
            display_name: "Ana".to_owned(),
            email: None,
            role: wardnet_common::auth::UserRole::Admin,
        })
        .await
        .is_err()
    );
    assert!(svc.update_profile(id, "Ana", None).await.is_err());
    assert!(svc.set_enabled(id, false).await.is_err());
    assert!(
        svc.set_role(id, wardnet_common::auth::UserRole::Member)
            .await
            .is_err()
    );
    assert!(svc.delete_user(id).await.is_err());
    assert!(svc.change_own_password("a", "b").await.is_err());
    assert!(svc.issue_enrolment(id).await.is_err());
    assert!(svc.revoke_enrolment(id, id).await.is_err());
    assert!(svc.cleanup_expired_enrolments().await.is_err());
    assert!(
        svc.configure_oauth_provider(
            wardnetd_services::user::OauthProvider::Github,
            "id",
            Some("secret"),
            true,
        )
        .await
        .is_err()
    );
    assert!(
        svc.clear_oauth_provider(wardnetd_services::user::OauthProvider::Github)
            .await
            .is_err()
    );
    assert!(
        svc.unlink_oauth(id, wardnetd_services::user::OauthProvider::Google)
            .await
            .is_err()
    );
}
