use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use crate::repository::{
    AdminRepository, ApiKeyRepository, SessionRepository, SystemConfigRepository,
};
use crate::service::{AuthService, AuthServiceImpl};

// -- Mock repositories ---------------------------------------------------

/// Mock admin repo that tracks created admins.
struct MockAdminRepo {
    created: Mutex<Vec<(String, String, String)>>,
}

impl MockAdminRepo {
    fn new() -> Self {
        Self {
            created: Mutex::new(Vec::new()),
        }
    }
}

#[async_trait]
impl AdminRepository for MockAdminRepo {
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
        Ok(false)
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

/// Mock system config repo that tracks `setup_completed` state.
struct MockSystemConfigRepo {
    setup_completed: Mutex<bool>,
}

impl MockSystemConfigRepo {
    fn new(completed: bool) -> Self {
        Self {
            setup_completed: Mutex::new(completed),
        }
    }
}

#[async_trait]
impl SystemConfigRepository for MockSystemConfigRepo {
    async fn get(&self, key: &str) -> anyhow::Result<Option<String>> {
        if key == "setup_completed" {
            let completed = *self.setup_completed.lock().unwrap();
            Ok(Some(if completed { "true" } else { "false" }.to_owned()))
        } else {
            Ok(None)
        }
    }
    async fn set(&self, key: &str, value: &str) -> anyhow::Result<()> {
        if key == "setup_completed" {
            *self.setup_completed.lock().unwrap() = value == "true";
        }
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
    setup_completed: bool,
) -> (
    AuthServiceImpl,
    Arc<MockAdminRepo>,
    Arc<MockSystemConfigRepo>,
) {
    let admin_repo = Arc::new(MockAdminRepo::new());
    let system_config = Arc::new(MockSystemConfigRepo::new(setup_completed));
    let svc = AuthServiceImpl::new(
        admin_repo.clone(),
        Arc::new(MockSessionRepo),
        Arc::new(MockApiKeyRepo),
        system_config.clone(),
        24,
    );
    (svc, admin_repo, system_config)
}

// -- Tests ----------------------------------------------------------------

#[tokio::test]
async fn setup_admin_succeeds_when_not_completed() {
    let (svc, admin_repo, system_config) = make_service(false);

    let result = svc.setup_admin("adminuser", "password123").await;
    assert!(result.is_ok());

    // Verify admin was created.
    let created = admin_repo.created.lock().unwrap();
    assert_eq!(created.len(), 1);
    assert_eq!(created[0].1, "adminuser");

    // Verify setup was marked completed.
    assert!(*system_config.setup_completed.lock().unwrap());
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
async fn is_setup_completed_returns_true_after_setup() {
    let (svc, _, _) = make_service(false);

    svc.setup_admin("adminuser", "password123").await.unwrap();

    let result = svc.is_setup_completed().await.unwrap();
    assert!(result);
}
