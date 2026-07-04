use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use crate::{AuthService, AuthServiceImpl};
use wardnetd_data::repository::{
    AdminRepository, ApiKeyRepository, SessionRepository, SystemConfigRepository,
};

// -- Mock repositories ---------------------------------------------------

/// Mock admin repo that tracks created admins.
///
/// `exists()` returns `true` once any admin has been `create`-d (or
/// when seeded via `with_existing_admin`), which mirrors the real
/// `SQLite` repo's behaviour — `setup_admin`'s 409 guard now reads
/// this directly instead of the legacy `setup_completed` key.
struct MockAdminRepo {
    created: Mutex<Vec<(String, String, String)>>,
    seeded_exists: Mutex<bool>,
}

impl MockAdminRepo {
    fn new() -> Self {
        Self {
            created: Mutex::new(Vec::new()),
            seeded_exists: Mutex::new(false),
        }
    }

    fn with_existing_admin() -> Self {
        Self {
            created: Mutex::new(Vec::new()),
            seeded_exists: Mutex::new(true),
        }
    }
}

#[async_trait]
impl AdminRepository for MockAdminRepo {
    async fn find_username_by_id(&self, _id: &str) -> anyhow::Result<Option<String>> {
        Ok(Some("admin".to_owned()))
    }
    async fn find_by_username(&self, _username: &str) -> anyhow::Result<Option<(String, String)>> {
        Ok(None)
    }
    async fn create(&self, id: &str, username: &str, password_hash: &str) -> anyhow::Result<()> {
        self.created.lock().unwrap().push((
            id.to_owned(),
            username.to_owned(),
            password_hash.to_owned(),
        ));
        Ok(())
    }
    async fn find_first_id(&self) -> anyhow::Result<Option<String>> {
        Ok(None)
    }
    async fn exists(&self) -> anyhow::Result<bool> {
        Ok(*self.seeded_exists.lock().unwrap() || !self.created.lock().unwrap().is_empty())
    }
}

/// Mock session repo (unused in setup tests).
struct MockSessionRepo;

#[async_trait]
impl SessionRepository for MockSessionRepo {
    async fn create(
        &self,
        _id: &str,
        _admin_id: &str,
        _token_hash: &str,
        _created_at: &str,
        _expires_at: &str,
        _remember_me: bool,
    ) -> anyhow::Result<()> {
        Ok(())
    }
    async fn find_admin_id_by_token_hash(
        &self,
        _token_hash: &str,
        _now: &str,
    ) -> anyhow::Result<Option<String>> {
        Ok(None)
    }
    async fn delete_expired(&self, _now: &str) -> anyhow::Result<u64> {
        Ok(0)
    }
    async fn extend_expiry(&self, _token_hash: &str, _new_expires_at: &str) -> anyhow::Result<()> {
        Ok(())
    }
    async fn rotate_token(
        &self,
        _old_token_hash: &str,
        _new_token_hash: &str,
        _new_expires_at: &str,
    ) -> anyhow::Result<()> {
        Ok(())
    }
    async fn find_session_for_refresh(
        &self,
        _token_hash: &str,
        _now: &str,
    ) -> anyhow::Result<Option<(String, bool, String)>> {
        Ok(None)
    }
}

/// Mock API key repo (unused in setup tests).
struct MockApiKeyRepo;

#[async_trait]
impl ApiKeyRepository for MockApiKeyRepo {
    async fn find_all_hashes(&self) -> anyhow::Result<Vec<(String, String)>> {
        Ok(vec![])
    }
    async fn create(&self, _id: &str, _l: &str, _h: &str, _c: &str) -> anyhow::Result<()> {
        Ok(())
    }
    async fn update_last_used(&self, _id: &str, _now: &str) -> anyhow::Result<()> {
        Ok(())
    }
}

/// Mock system config repo backed by an in-memory `HashMap`.
///
/// Exposes `setup_completed` as a typed mutex to keep the existing
/// assertions readable; everything else (`wizard_step`, `default_policy`,
/// `router_mac` …) is stored in `store`.
struct MockSystemConfigRepo {
    store: Mutex<HashMap<String, String>>,
    setup_completed: Mutex<bool>,
}

impl MockSystemConfigRepo {
    fn new(completed: bool) -> Self {
        let mut store = HashMap::new();
        if completed {
            store.insert("setup_completed".to_owned(), "true".to_owned());
        }
        Self {
            store: Mutex::new(store),
            setup_completed: Mutex::new(completed),
        }
    }
}

#[async_trait]
impl SystemConfigRepository for MockSystemConfigRepo {
    async fn get(&self, key: &str) -> anyhow::Result<Option<String>> {
        if key == "setup_completed" {
            let completed = *self.setup_completed.lock().unwrap();
            return Ok(Some(if completed { "true" } else { "false" }.to_owned()));
        }
        Ok(self.store.lock().unwrap().get(key).cloned())
    }
    async fn set(&self, key: &str, value: &str) -> anyhow::Result<()> {
        if key == "setup_completed" {
            *self.setup_completed.lock().unwrap() = value == "true";
        }
        self.store
            .lock()
            .unwrap()
            .insert(key.to_owned(), value.to_owned());
        Ok(())
    }
    async fn delete(&self, key: &str) -> anyhow::Result<()> {
        self.store.lock().unwrap().remove(key);
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

// -- Helpers --------------------------------------------------------------

fn make_service(
    admin_exists: bool,
) -> (
    AuthServiceImpl,
    Arc<MockAdminRepo>,
    Arc<MockSystemConfigRepo>,
) {
    let admin_repo = Arc::new(if admin_exists {
        MockAdminRepo::with_existing_admin()
    } else {
        MockAdminRepo::new()
    });
    // `setup_completed` legacy key starts unset; the 409 guard now uses
    // admin existence so this only matters for tests that exercise
    // `is_setup_completed` directly.
    let system_config = Arc::new(MockSystemConfigRepo::new(false));
    let svc = AuthServiceImpl::new(
        admin_repo.clone(),
        Arc::new(MockSessionRepo),
        Arc::new(MockApiKeyRepo),
        system_config.clone(),
        24,
        720,
    );
    (svc, admin_repo, system_config)
}

// -- Tests ----------------------------------------------------------------

#[tokio::test]
async fn setup_admin_succeeds_when_not_completed() {
    let (svc, admin_repo, system_config) = make_service(false);

    let result = svc.setup_admin("adminuser", "password123").await;
    assert!(result.is_ok());

    // Verify admin was created — this is now the canonical "setup of
    // step 1 is done" signal that drives the 409 guard on retry.
    let created = admin_repo.created.lock().unwrap();
    assert_eq!(created.len(), 1);
    assert_eq!(created[0].1, "adminuser");

    // Verify the legacy setup_completed key is NOT touched here. It
    // used to flip to true alongside admin creation but that produced
    // a race window between the two writes; we now derive
    // setup_completed from wizard_step == Completed instead.
    assert!(!*system_config.setup_completed.lock().unwrap());
}

#[tokio::test]
async fn setup_admin_fails_when_already_completed() {
    let (svc, _, _) = make_service(true);

    let result = svc.setup_admin("adminuser", "password123").await;
    assert!(result.is_err());

    let err = result.unwrap_err();
    let err_msg = err.to_string();
    assert!(
        err_msg.contains("setup already completed"),
        "expected conflict error, got: {err_msg}"
    );
}

#[tokio::test]
async fn setup_admin_fails_with_empty_username() {
    let (svc, _, _) = make_service(false);

    let result = svc.setup_admin("ab", "password123").await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("username"));
}

#[tokio::test]
async fn setup_admin_fails_with_long_username() {
    let (svc, _, _) = make_service(false);

    let long_name = "a".repeat(33);
    let result = svc.setup_admin(&long_name, "password123").await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("username"));
}

#[tokio::test]
async fn setup_admin_fails_with_non_alphanumeric_username() {
    let (svc, _, _) = make_service(false);

    let result = svc.setup_admin("admin@user", "password123").await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("username"));
}

#[tokio::test]
async fn setup_admin_fails_with_short_password() {
    let (svc, _, _) = make_service(false);

    let result = svc.setup_admin("adminuser", "short").await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("password"));
}

#[tokio::test]
async fn setup_admin_hashes_password() {
    let (svc, admin_repo, _) = make_service(false);

    svc.setup_admin("adminuser", "mysecretpassword")
        .await
        .unwrap();

    let created = admin_repo.created.lock().unwrap();
    assert_eq!(created.len(), 1);

    let stored_hash = &created[0].2;
    // Argon2 hashes start with "$argon2".
    assert!(
        stored_hash.starts_with("$argon2"),
        "password should be hashed with argon2, got: {stored_hash}"
    );
    // Ensure the plaintext is NOT stored.
    assert_ne!(stored_hash, "mysecretpassword");
}

#[tokio::test]
async fn is_setup_completed_returns_false_initially() {
    let (svc, _, _) = make_service(false);

    let result = svc.is_setup_completed().await.unwrap();
    assert!(!result);
}

#[tokio::test]
async fn is_setup_completed_returns_false_after_setup_admin() {
    // `is_setup_completed()` now derives from `wizard_step == Completed`,
    // not from "an admin row exists". After setup_admin, the wizard is
    // at "network" — operator still needs to walk through the rest of
    // the wizard before this method reports true.
    let (svc, _, _) = make_service(false);

    svc.setup_admin("adminuser", "password123").await.unwrap();

    let result = svc.is_setup_completed().await.unwrap();
    assert!(!result);
}

#[tokio::test]
async fn is_setup_completed_returns_true_when_wizard_finished() {
    let (svc, _, system_config) = make_service(false);
    // Drive the system_config straight to wizard_step=completed (the
    // production path is via advance_wizard, exercised in `wizard.rs`).
    system_config.set_wizard_step("completed").await.unwrap();

    let result = svc.is_setup_completed().await.unwrap();
    assert!(result);
}
