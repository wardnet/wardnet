use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use uuid::Uuid;

use crate::auth::AuthServiceImpl;
use crate::auth::service::AuthService;
use crate::auth_context;
use crate::error::AppError;
use wardnet_common::api::{WizardMode, WizardStep};
use wardnet_common::auth::AuthContext;
use wardnetd_data::repository::{
    AdminRepository, ApiKeyRepository, SessionRepository, SystemConfigRepository,
};

// -- Mocks -------------------------------------------------------------------

struct MockAdminRepo {
    has_admin: Mutex<bool>,
}

impl MockAdminRepo {
    fn new(has_admin: bool) -> Self {
        Self {
            has_admin: Mutex::new(has_admin),
        }
    }
}

#[async_trait]
impl AdminRepository for MockAdminRepo {
    async fn find_by_username(&self, _u: &str) -> anyhow::Result<Option<(String, String)>> {
        Ok(None)
    }
    async fn create(&self, _id: &str, _u: &str, _hash: &str) -> anyhow::Result<()> {
        *self.has_admin.lock().unwrap() = true;
        Ok(())
    }
    async fn find_first_id(&self) -> anyhow::Result<Option<String>> {
        Ok(None)
    }
    async fn exists(&self) -> anyhow::Result<bool> {
        Ok(*self.has_admin.lock().unwrap())
    }
}

struct MockSessionRepo;

#[async_trait]
impl SessionRepository for MockSessionRepo {
    async fn create(
        &self,
        _id: &str,
        _admin_id: &str,
        _hash: &str,
        _created_at: &str,
        _expires_at: &str,
    ) -> anyhow::Result<()> {
        Ok(())
    }
    async fn find_admin_id_by_token_hash(
        &self,
        _hash: &str,
        _now: &str,
    ) -> anyhow::Result<Option<String>> {
        Ok(None)
    }
    async fn delete_expired(&self, _now: &str) -> anyhow::Result<u64> {
        Ok(0)
    }
}

struct MockApiKeyRepo;

#[async_trait]
impl ApiKeyRepository for MockApiKeyRepo {
    async fn create(&self, _id: &str, _name: &str, _hash: &str, _now: &str) -> anyhow::Result<()> {
        Ok(())
    }
    async fn find_all_hashes(&self) -> anyhow::Result<Vec<(String, String)>> {
        Ok(vec![])
    }
    async fn update_last_used(&self, _id: &str, _now: &str) -> anyhow::Result<()> {
        Ok(())
    }
}

#[derive(Default)]
struct MockSystemConfigRepo {
    store: Mutex<HashMap<String, String>>,
}

impl MockSystemConfigRepo {
    fn with(initial: &[(&str, &str)]) -> Self {
        let mut map = HashMap::new();
        for (k, v) in initial {
            map.insert((*k).to_owned(), (*v).to_owned());
        }
        Self {
            store: Mutex::new(map),
        }
    }
}

#[async_trait]
impl SystemConfigRepository for MockSystemConfigRepo {
    async fn get(&self, key: &str) -> anyhow::Result<Option<String>> {
        Ok(self.store.lock().unwrap().get(key).cloned())
    }
    async fn set(&self, key: &str, value: &str) -> anyhow::Result<()> {
        self.store
            .lock()
            .unwrap()
            .insert(key.to_owned(), value.to_owned());
        Ok(())
    }
    async fn device_count(&self) -> anyhow::Result<i64> {
        Ok(0)
    }
    async fn tunnel_count(&self) -> anyhow::Result<i64> {
        Ok(0)
    }
    async fn db_size_bytes(&self) -> anyhow::Result<u64> {
        Ok(0)
    }
}

// -- Helpers -----------------------------------------------------------------

fn make_service(
    has_admin: bool,
    initial_state: &[(&str, &str)],
) -> (AuthServiceImpl, Arc<MockSystemConfigRepo>) {
    let system_config = Arc::new(MockSystemConfigRepo::with(initial_state));
    let svc = AuthServiceImpl::new(
        Arc::new(MockAdminRepo::new(has_admin)),
        Arc::new(MockSessionRepo),
        Arc::new(MockApiKeyRepo),
        system_config.clone(),
        24,
    );
    (svc, system_config)
}

async fn as_admin<F: std::future::Future>(f: F) -> F::Output {
    auth_context::with_context(
        AuthContext::Admin {
            admin_id: Uuid::nil(),
        },
        f,
    )
    .await
}

// -- wizard_state -----------------------------------------------------------

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

    let state = svc.wizard_state().await.unwrap();

    assert!(state.setup_completed());
}

#[tokio::test]
async fn wizard_state_does_not_require_auth() {
    // The wizard_state path is exposed via the unauthenticated
    // GET /api/setup/status, so it must succeed without an AuthContext.
    let (svc, _) = make_service(false, &[]);

    // Note: no `as_admin` wrapper — calling raw.
    let state = svc.wizard_state().await.unwrap();

    assert_eq!(state.step, WizardStep::Admin);
}

// -- advance_wizard ---------------------------------------------------------

#[tokio::test]
async fn advance_wizard_requires_admin_auth() {
    let (svc, _) = make_service(true, &[("wizard_step", "admin")]);

    // Call without admin context — should be Forbidden.
    let result = svc
        .advance_wizard(WizardStep::Network, None)
        .await
        .expect_err("advance_wizard without admin context must fail");

    assert!(matches!(result, AppError::Forbidden(_)));
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
async fn advance_wizard_rejects_rewind() {
    let (svc, _) = make_service(true, &[("wizard_step", "tunnel")]);

    let err = as_admin(svc.advance_wizard(WizardStep::Network, None))
        .await
        .expect_err("rewind should be rejected");

    assert!(matches!(err, AppError::BadRequest(_)));
}

#[tokio::test]
async fn advance_wizard_to_completed_requires_admin_to_exist() {
    let (svc, _) = make_service(false, &[("wizard_step", "policy")]);

    let err = as_admin(svc.advance_wizard(WizardStep::Completed, None))
        .await
        .expect_err("completing without an admin should fail");

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
