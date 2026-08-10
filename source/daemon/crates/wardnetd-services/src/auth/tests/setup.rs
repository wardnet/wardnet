//! Tests for the one-shot setup path — `setup_admin` and the
//! `is_setup_completed` signal derived from the wizard step.

use std::sync::Arc;

use wardnet_common::api::WizardStep;
use wardnet_common::auth::UserRole;
use wardnetd_data::repository::system_config::SystemConfigRepository;
use wardnetd_data::repository::user::UserRepository;
use wardnetd_data::repository::user_credential::UserCredentialRepository;

use crate::auth::{AuthService, AuthServiceImpl};
use crate::error::AppError;
use crate::tests::repo_mocks::{
    MockApiKeyRepo, MockCredentialRepo, MockSessionRepo, MockSystemConfigRepo, MockUserRepo,
};

/// Handles a setup test needs to assert on.
struct Fixture {
    svc: AuthServiceImpl,
    users: Arc<MockUserRepo>,
    credentials: Arc<MockCredentialRepo>,
    config: Arc<MockSystemConfigRepo>,
}

/// Build a service over an empty box, optionally with a pre-seeded config key.
fn fixture(users: MockUserRepo, config: MockSystemConfigRepo) -> Fixture {
    let users = Arc::new(users);
    let credentials = Arc::new(MockCredentialRepo::joined_to(Arc::clone(&users)));
    let config = Arc::new(config);
    let svc = AuthServiceImpl::new(
        Arc::clone(&users) as Arc<dyn UserRepository>,
        Arc::clone(&credentials) as Arc<dyn UserCredentialRepository>,
        Arc::new(MockSessionRepo::empty()),
        Arc::new(MockApiKeyRepo::empty()),
        Arc::clone(&config) as Arc<dyn SystemConfigRepository>,
        24,
        720,
    );
    Fixture {
        svc,
        users,
        credentials,
        config,
    }
}

fn fresh_box() -> Fixture {
    fixture(MockUserRepo::empty(), MockSystemConfigRepo::empty())
}

// -- setup_admin ----------------------------------------------------------

#[tokio::test]
async fn setup_admin_succeeds_when_not_completed() {
    let f = fresh_box();
    assert!(
        f.svc
            .setup_admin("operator", "a-good-password")
            .await
            .is_ok()
    );
    assert_eq!(f.users.count(), 1);
}

#[tokio::test]
async fn setup_admin_fails_when_a_user_already_exists() {
    // The guard reads `users.exists()` directly rather than the legacy
    // `setup_completed` key, so it cannot disagree with reality after a crash
    // between the two writes.
    let f = fixture(
        MockUserRepo::with_admin("00000000-0000-0000-0000-000000000001", "admin"),
        MockSystemConfigRepo::empty(),
    );
    let result = f.svc.setup_admin("second", "a-good-password").await;
    assert!(matches!(result, Err(AppError::Conflict(_))));
}

#[tokio::test]
async fn setup_admin_fails_with_empty_username() {
    let f = fresh_box();
    assert!(matches!(
        f.svc.setup_admin("", "a-good-password").await,
        Err(AppError::BadRequest(_))
    ));
}

#[tokio::test]
async fn setup_admin_fails_with_long_username() {
    let f = fresh_box();
    let long = "a".repeat(33);
    assert!(matches!(
        f.svc.setup_admin(&long, "a-good-password").await,
        Err(AppError::BadRequest(_))
    ));
}

#[tokio::test]
async fn setup_admin_fails_with_non_alphanumeric_username() {
    let f = fresh_box();
    assert!(matches!(
        f.svc.setup_admin("has space", "a-good-password").await,
        Err(AppError::BadRequest(_))
    ));
    assert!(matches!(
        f.svc.setup_admin("has-dash", "a-good-password").await,
        Err(AppError::BadRequest(_))
    ));
}

#[tokio::test]
async fn setup_admin_fails_with_short_password() {
    let f = fresh_box();
    assert!(matches!(
        f.svc.setup_admin("operator", "short").await,
        Err(AppError::BadRequest(_))
    ));
}

#[tokio::test]
async fn setup_admin_stores_an_argon2_hash_not_the_password() {
    let f = fresh_box();
    f.svc
        .setup_admin("operator", "a-good-password")
        .await
        .unwrap();

    let user_id = f.users.rows.lock().unwrap()[0].id.clone();
    let credential = f
        .credentials
        .find_password(&user_id)
        .await
        .unwrap()
        .expect("a password credential must exist");
    let secret = credential
        .secret
        .expect("a password credential has a secret");

    assert!(
        secret.starts_with("$argon2"),
        "expected an Argon2 PHC string, got {secret:?}"
    );
    assert!(
        !secret.contains("a-good-password"),
        "the plaintext password must never be stored"
    );
}

#[tokio::test]
async fn setup_admin_creates_an_admin_role_user() {
    // Decision 6: the first account is a `role=admin` household user, exactly
    // equal to the legacy local admin.
    let f = fresh_box();
    f.svc
        .setup_admin("operator", "a-good-password")
        .await
        .unwrap();

    let row = f.users.rows.lock().unwrap()[0].clone();
    assert_eq!(row.role, UserRole::Admin);
    assert!(row.enabled);
}

#[tokio::test]
async fn setup_admin_lowercases_the_credential_subject() {
    // The migration lowercases backfilled usernames, so this path must match or
    // a wizard-created admin would have case-sensitive login while an upgraded
    // one did not.
    let f = fresh_box();
    f.svc
        .setup_admin("Operator", "a-good-password")
        .await
        .unwrap();

    let user_id = f.users.rows.lock().unwrap()[0].id.clone();
    let credential = f
        .credentials
        .find_password(&user_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(credential.subject, "operator");
    // The display name keeps the operator's own capitalisation.
    assert_eq!(f.users.rows.lock().unwrap()[0].display_name, "Operator");
}

// -- wizard advance on setup ---------------------------------------------

#[tokio::test]
async fn setup_admin_advances_wizard_to_network() {
    let f = fixture(
        MockUserRepo::empty(),
        MockSystemConfigRepo::with("wizard_step", WizardStep::Admin.as_storage_str()),
    );
    f.svc
        .setup_admin("operator", "a-good-password")
        .await
        .unwrap();
    assert_eq!(
        f.config.read("wizard_step").as_deref(),
        Some(WizardStep::Network.as_storage_str())
    );
}

#[tokio::test]
async fn setup_admin_advances_when_wizard_step_unset() {
    let f = fresh_box();
    f.svc
        .setup_admin("operator", "a-good-password")
        .await
        .unwrap();
    assert_eq!(
        f.config.read("wizard_step").as_deref(),
        Some(WizardStep::Network.as_storage_str())
    );
}

#[tokio::test]
async fn setup_admin_leaves_a_further_along_wizard_step_alone() {
    // If an operator advanced manually before the frontend got there, we must
    // not rewind them — `advance_wizard` would reject that anyway.
    let f = fixture(
        MockUserRepo::empty(),
        MockSystemConfigRepo::with("wizard_step", WizardStep::Dhcp.as_storage_str()),
    );
    f.svc
        .setup_admin("operator", "a-good-password")
        .await
        .unwrap();
    assert_eq!(
        f.config.read("wizard_step").as_deref(),
        Some(WizardStep::Dhcp.as_storage_str())
    );
}

// -- is_setup_completed ---------------------------------------------------

#[tokio::test]
async fn is_setup_completed_returns_false_initially() {
    let f = fresh_box();
    assert!(!f.svc.is_setup_completed().await.unwrap());
}

#[tokio::test]
async fn is_setup_completed_returns_false_after_setup_admin() {
    // Creating the admin is step one of several; setup is not "completed" until
    // the wizard says so.
    let f = fresh_box();
    f.svc
        .setup_admin("operator", "a-good-password")
        .await
        .unwrap();
    assert!(!f.svc.is_setup_completed().await.unwrap());
}

#[tokio::test]
async fn is_setup_completed_returns_true_when_wizard_finished() {
    let f = fixture(
        MockUserRepo::empty(),
        MockSystemConfigRepo::with("wizard_step", WizardStep::Completed.as_storage_str()),
    );
    assert!(f.svc.is_setup_completed().await.unwrap());
}
