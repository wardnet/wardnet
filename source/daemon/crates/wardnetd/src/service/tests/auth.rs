use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use crate::repository::{AdminRepository, ApiKeyRepository, SessionRepository};
use crate::service::{AuthService, AuthServiceImpl};

// -- Mock repositories ---------------------------------------------------

/// Mock admin repo that returns a preconfigured result for `find_by_username`.
struct MockAdminRepo {
    find_result: Mutex<Option<(String, String)>>,
    first_id: Mutex<Option<String>>,
}

#[async_trait]
impl AdminRepository for MockAdminRepo {
    async fn find_by_username(&self, _username: &str) -> anyhow::Result<Option<(String, String)>> {
        Ok(self.find_result.lock().unwrap().clone())
    }
    async fn create(&self, _id: &str, _u: &str, _h: &str) -> anyhow::Result<()> {
        Ok(())
    }
    async fn find_first_id(&self) -> anyhow::Result<Option<String>> {
        Ok(self.first_id.lock().unwrap().clone())
    }
}

/// Mock session repo that captures created sessions and returns a preconfigured lookup result.
struct MockSessionRepo {
    find_result: Mutex<Option<String>>,
}

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
        Ok(self.find_result.lock().unwrap().clone())
    }
    async fn delete_expired(&self, _now: &str) -> anyhow::Result<u64> {
        Ok(0)
    }
}

/// Mock API key repo that returns preconfigured key hashes.
struct MockApiKeyRepo {
    hashes: Vec<(String, String)>,
}

#[async_trait]
impl ApiKeyRepository for MockApiKeyRepo {
    async fn find_all_hashes(&self) -> anyhow::Result<Vec<(String, String)>> {
        Ok(self.hashes.clone())
    }
    async fn create(&self, _id: &str, _l: &str, _h: &str, _c: &str) -> anyhow::Result<()> {
        Ok(())
    }
    async fn update_last_used(&self, _id: &str, _now: &str) -> anyhow::Result<()> {
        Ok(())
    }
}

// -- Helpers --------------------------------------------------------------

fn argon2_hash(password: &str) -> String {
    use argon2::PasswordHasher;
    let salt = argon2::password_hash::SaltString::from_b64("dGVzdHNhbHR2YWx1ZTEyMw").unwrap();
    argon2::Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .unwrap()
        .to_string()
}

fn make_auth_service(
    admin_find: Option<(String, String)>,
    admin_first_id: Option<String>,
    session_find: Option<String>,
    api_key_hashes: Vec<(String, String)>,
) -> AuthServiceImpl {
    AuthServiceImpl::new(
        Arc::new(MockAdminRepo {
            find_result: Mutex::new(admin_find),
            first_id: Mutex::new(admin_first_id),
        }),
        Arc::new(MockSessionRepo {
            find_result: Mutex::new(session_find),
        }),
        Arc::new(MockApiKeyRepo {
            hashes: api_key_hashes,
        }),
        24,
    )
}

// -- Tests ----------------------------------------------------------------

#[tokio::test]
async fn login_success() {
    let hash = argon2_hash("correct-password");
    let svc = make_auth_service(Some(("admin-1".to_owned(), hash)), None, None, vec![]);

    let result = svc.login("admin", "correct-password").await;
    assert!(result.is_ok());
    let login = result.unwrap();
    assert!(!login.token.is_empty());
    assert_eq!(login.max_age_seconds, 24 * 3600);
}

#[tokio::test]
async fn login_wrong_password() {
    let hash = argon2_hash("correct-password");
    let svc = make_auth_service(Some(("admin-1".to_owned(), hash)), None, None, vec![]);

    let result = svc.login("admin", "wrong-password").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn login_user_not_found() {
    let svc = make_auth_service(None, None, None, vec![]);

    let result = svc.login("nobody", "password").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn validate_session_valid() {
    let admin_uuid = "00000000-0000-0000-0000-000000000001";
    let svc = make_auth_service(None, None, Some(admin_uuid.to_owned()), vec![]);

    let result = svc.validate_session("any-token").await.unwrap();
    assert!(result.is_some());
    assert_eq!(result.unwrap().to_string(), admin_uuid);
}

#[tokio::test]
async fn validate_session_expired() {
    let svc = make_auth_service(None, None, None, vec![]);

    let result = svc.validate_session("any-token").await.unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn validate_api_key_valid() {
    let hash = argon2_hash("my-secret-key");
    let admin_uuid = "00000000-0000-0000-0000-000000000001";
    let svc = make_auth_service(
        None,
        Some(admin_uuid.to_owned()),
        None,
        vec![("key-1".to_owned(), hash)],
    );

    let result = svc.validate_api_key("my-secret-key").await.unwrap();
    assert!(result.is_some());
    assert_eq!(result.unwrap().to_string(), admin_uuid);
}

#[tokio::test]
async fn validate_api_key_invalid() {
    let hash = argon2_hash("my-secret-key");
    let svc = make_auth_service(
        None,
        Some("00000000-0000-0000-0000-000000000001".to_owned()),
        None,
        vec![("key-1".to_owned(), hash)],
    );

    let result = svc.validate_api_key("wrong-key").await.unwrap();
    assert!(result.is_none());
}
