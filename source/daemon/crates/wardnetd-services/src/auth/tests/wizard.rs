//! Tests for the setup-wizard state machine — `wizard_state` and
//! `advance_wizard`.

use std::sync::Arc;

use uuid::Uuid;
use wardnet_common::api::{WizardMode, WizardStep};
use wardnet_common::auth::AuthContext;
use wardnet_test_support::principal;
use wardnetd_data::repository::system_config::SystemConfigRepository;
use wardnetd_data::repository::user::UserRepository;

use crate::auth::{AuthService, AuthServiceImpl};
use crate::auth_context;
use crate::error::AppError;
use crate::tests::repo_mocks::{
    MockApiKeyRepo, MockCredentialRepo, MockSessionRepo, MockSystemConfigRepo, MockUserRepo,
};

const ADMIN_ID: &str = "00000000-0000-0000-0000-000000000001";
const MEMBER_ID: &str = "00000000-0000-0000-0000-000000000002";

/// Build a service, optionally with an existing admin, seeded with the given
/// `system_config` keys.
fn make_service(
    has_admin: bool,
    initial_state: &[(&str, &str)],
) -> (AuthServiceImpl, Arc<MockSystemConfigRepo>) {
    let users = Arc::new(if has_admin {
        MockUserRepo::with_admin(ADMIN_ID, "admin")
    } else {
        MockUserRepo::empty()
    });
    let config = Arc::new(MockSystemConfigRepo::empty());
    for (k, v) in initial_state {
        config
            .values
            .lock()
            .unwrap()
            .insert((*k).to_owned(), (*v).to_owned());
    }
    let svc = AuthServiceImpl::new(
        Arc::clone(&users) as Arc<dyn UserRepository>,
        Arc::new(MockCredentialRepo::joined_to(Arc::clone(&users))),
        Arc::new(MockSessionRepo::empty()),
        Arc::new(MockApiKeyRepo::empty()),
        Arc::clone(&config) as Arc<dyn SystemConfigRepository>,
        24,
        720,
    );
    (svc, config)
}

/// Run `f` as the system actor, which carries `role = admin`.
async fn as_admin<F: std::future::Future>(f: F) -> F::Output {
    auth_context::with_context(AuthContext::system(), f).await
}

// -- wizard_state ---------------------------------------------------------

#[tokio::test]
async fn wizard_state_defaults_to_admin_for_fresh_install() {
    let (svc, _) = make_service(false, &[]);

    let state = svc.wizard_state().await.unwrap();

    assert_eq!(state.step, WizardStep::Admin);
    assert_eq!(state.mode, None);
    assert!(!state.setup_completed());
}

#[tokio::test]
async fn wizard_state_reads_persisted_step_and_mode() {
    let (svc, _) = make_service(
        true,
        &[("wizard_step", "dhcp"), ("wizard_mode", "locked_router")],
    );

    let state = svc.wizard_state().await.unwrap();

    assert_eq!(state.step, WizardStep::Dhcp);
    assert_eq!(state.mode, Some(WizardMode::LockedRouter));
    assert!(!state.setup_completed());
}

#[tokio::test]
async fn wizard_state_setup_completed_is_derived_from_completed_step() {
    let (svc, _) = make_service(true, &[("wizard_step", "completed")]);

    assert!(svc.wizard_state().await.unwrap().setup_completed());
}

#[tokio::test]
async fn wizard_state_does_not_require_auth() {
    // Exposed via the unauthenticated GET /api/setup/status, so it must succeed
    // with no `AuthContext` at all — that is what lets a fresh browser be
    // redirected to the wizard.
    let (svc, _) = make_service(false, &[]);

    // Note: no `as_admin` wrapper — called raw.
    let state = svc.wizard_state().await.unwrap();

    assert_eq!(state.step, WizardStep::Admin);
}

#[tokio::test]
async fn wizard_state_reads_persisted_dns_and_review_steps() {
    for (raw, expected) in [("dns", WizardStep::Dns), ("review", WizardStep::Review)] {
        let (svc, _) = make_service(true, &[("wizard_step", raw)]);
        let state = svc.wizard_state().await.unwrap();
        assert_eq!(state.step, expected, "storage string {raw:?}");
    }
}

#[tokio::test]
async fn wizard_step_ordinals_pin_the_full_sequence() {
    // Pins the canonical order so inserting a step is a conscious decision
    // (rewind validation and the web rail both depend on it).
    let expected = [
        WizardStep::Admin,
        WizardStep::Network,
        WizardStep::Dhcp,
        WizardStep::RouterMac,
        WizardStep::Dns,
        WizardStep::Tunnel,
        WizardStep::Policy,
        WizardStep::RemoteAccess,
        WizardStep::Review,
        WizardStep::Completed,
    ];
    for (i, step) in expected.iter().enumerate() {
        assert_eq!(step.ordinal() as usize, i, "{step:?}");
        assert_eq!(
            WizardStep::from_storage_str(step.as_storage_str()),
            *step,
            "storage round-trip for {step:?}"
        );
    }
}

// -- advance_wizard -------------------------------------------------------

#[tokio::test]
async fn advance_wizard_requires_admin_auth() {
    let (svc, _) = make_service(true, &[("wizard_step", "admin")]);

    let result = svc
        .advance_wizard(WizardStep::Network, None)
        .await
        .expect_err("advance_wizard without an admin context must fail");

    assert!(matches!(result, AppError::Forbidden(_)));
}

#[tokio::test]
async fn advance_wizard_refuses_a_member() {
    // Setting the box up is an admin action. A member is authenticated but not
    // authorized, and `require_admin` is what draws that line.
    let (svc, _) = make_service(true, &[("wizard_step", "network")]);
    let ctx = principal::member_context(Uuid::parse_str(MEMBER_ID).unwrap());

    let result = auth_context::with_context(ctx, async {
        svc.advance_wizard(WizardStep::Dhcp, None).await
    })
    .await;

    assert!(matches!(result, Err(AppError::Forbidden(_))));
}

#[tokio::test]
async fn advance_wizard_persists_step_and_mode() {
    let (svc, store) = make_service(true, &[("wizard_step", "network")]);

    let state = as_admin(svc.advance_wizard(WizardStep::Dhcp, Some(WizardMode::Primary)))
        .await
        .unwrap();

    assert_eq!(state.step, WizardStep::Dhcp);
    assert_eq!(state.mode, Some(WizardMode::Primary));
    assert_eq!(
        store.get("wizard_step").await.unwrap().as_deref(),
        Some("dhcp")
    );
    assert_eq!(
        store.get("wizard_mode").await.unwrap().as_deref(),
        Some("primary")
    );
}

#[tokio::test]
async fn advance_wizard_preserves_existing_mode_when_omitted() {
    let (svc, _) = make_service(true, &[("wizard_step", "dhcp"), ("wizard_mode", "primary")]);

    let state = as_admin(svc.advance_wizard(WizardStep::Tunnel, None))
        .await
        .unwrap();

    assert_eq!(state.step, WizardStep::Tunnel);
    assert_eq!(state.mode, Some(WizardMode::Primary));
}

#[tokio::test]
async fn advance_wizard_rejects_rewind_to_admin() {
    // Admin creation is one-shot, so the wizard can never rewind to it.
    let (svc, _) = make_service(true, &[("wizard_step", "tunnel")]);

    let err = as_admin(svc.advance_wizard(WizardStep::Admin, None))
        .await
        .expect_err("rewind to admin should be rejected");

    assert!(matches!(err, AppError::BadRequest(_)));
}

#[tokio::test]
async fn advance_wizard_allows_rewind_to_network() {
    // Network is the rewind floor — revisiting it from any later step is
    // allowed and persisted.
    let (svc, store) = make_service(true, &[("wizard_step", "policy")]);

    let state = as_admin(svc.advance_wizard(WizardStep::Network, None))
        .await
        .unwrap();

    assert_eq!(state.step, WizardStep::Network);
    assert_eq!(
        store.get("wizard_step").await.unwrap().as_deref(),
        Some("network")
    );
}

#[tokio::test]
async fn advance_wizard_allows_rewind_to_mid_step() {
    let (svc, _) = make_service(true, &[("wizard_step", "review")]);

    let state = as_admin(svc.advance_wizard(WizardStep::Dns, None))
        .await
        .unwrap();

    assert_eq!(state.step, WizardStep::Dns);
}

#[tokio::test]
async fn advance_wizard_rejects_rewind_from_completed() {
    // Completed is terminal — once setup finishes the wizard never reopens.
    let (svc, _) = make_service(true, &[("wizard_step", "completed")]);

    let err = as_admin(svc.advance_wizard(WizardStep::Review, None))
        .await
        .expect_err("rewind from completed should be rejected");

    assert!(matches!(err, AppError::BadRequest(_)));
}

#[tokio::test]
async fn advance_wizard_allows_idempotent_completed() {
    let (svc, _) = make_service(true, &[("wizard_step", "completed")]);

    let state = as_admin(svc.advance_wizard(WizardStep::Completed, None))
        .await
        .unwrap();

    assert_eq!(state.step, WizardStep::Completed);
}

#[tokio::test]
async fn advance_wizard_to_completed_requires_a_user_to_exist() {
    // The check now reads `users.exists()`: finishing setup with no account at
    // all would leave a box nobody can log into.
    let (svc, _) = make_service(false, &[("wizard_step", "policy")]);

    let err = as_admin(svc.advance_wizard(WizardStep::Completed, None))
        .await
        .expect_err("completing without an account should fail");

    assert!(matches!(err, AppError::BadRequest(_)));
}

#[tokio::test]
async fn advance_wizard_allows_idempotent_same_step() {
    let (svc, _) = make_service(true, &[("wizard_step", "network")]);

    let state = as_admin(svc.advance_wizard(WizardStep::Network, None))
        .await
        .unwrap();

    assert_eq!(state.step, WizardStep::Network);
}
