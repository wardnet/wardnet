use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use uuid::Uuid;
use wardnet_common::auth::AuthContext;

use crate::error::AppError;
use crate::{AuthService, AuthServiceImpl, auth_context};
use wardnetd_data::repository::{
    AdminRepository, ApiKeyRepository, SessionRepository, SystemConfigRepository,
};

// -- Mock repositories ---------------------------------------------------

/// Mock admin repo that returns a preconfigured result for `find_by_username`.
struct MockAdminRepo {
    find_result: Mutex<Option<(String, String)>>,
    first_id: Mutex<Option<String>>,
}

#[async_trait]
impl AdminRepository for MockAdminRepo {
    async fn find_username_by_id(&self, _id: &str) -> anyhow::Result<Option<String>> {
        Ok(Some("admin".to_owned()))
    }
    async fn find_by_username(&self, _username: &str) -> anyhow::Result<Option<(String, String)>> {
        Ok(self.find_result.lock().unwrap().clone())
    }
    async fn create(&self, _id: &str, _u: &str, _h: &str) -> anyhow::Result<()> {
        Ok(())
    }
    async fn find_first_id(&self) -> anyhow::Result<Option<String>> {
        Ok(self.first_id.lock().unwrap().clone())
    }
    async fn exists(&self) -> anyhow::Result<bool> {
        Ok(self.find_result.lock().unwrap().is_some())
    }
}

/// Mock session repo that captures created sessions and returns preconfigured lookup results.
#[derive(Default)]
struct MockSessionRepo {
    /// Returned by `find_admin_id_by_token_hash` (drives `validate_session`).
    find_result: Mutex<Option<String>>,
    /// Returned by `find_session_for_refresh` (drives `refresh_session`).
    session_for_refresh: Mutex<Option<(String, bool, String)>>,
    /// Token hashes passed to `delete_by_token_hash` (drives `logout_session` assertions).
    deleted_hashes: Mutex<Vec<String>>,
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
        _remember_me: bool,
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
    async fn delete_by_token_hash(&self, token_hash: &str) -> anyhow::Result<u64> {
        // Report one row removed when a session is configured, zero otherwise,
        // mirroring the real repository's rows_affected semantics.
        let existed = self.find_result.lock().unwrap().take().is_some();
        self.deleted_hashes
            .lock()
            .unwrap()
            .push(token_hash.to_owned());
        Ok(u64::from(existed))
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
        Ok(self.session_for_refresh.lock().unwrap().clone())
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

/// Mock system config repo (unused in login/session tests).
struct MockSystemConfigRepo;

#[async_trait]
impl SystemConfigRepository for MockSystemConfigRepo {
    async fn get(&self, _key: &str) -> anyhow::Result<Option<String>> {
        Ok(None)
    }
    async fn set(&self, _key: &str, _value: &str) -> anyhow::Result<()> {
        Ok(())
    }
    async fn delete(&self, _key: &str) -> anyhow::Result<()> {
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
    let recent_created_at = (chrono::Utc::now() - chrono::Duration::days(30)).to_rfc3339();
    let session_for_refresh = session_find
        .as_ref()
        .map(|id| (id.clone(), true, recent_created_at));
    AuthServiceImpl::new(
        Arc::new(MockAdminRepo {
            find_result: Mutex::new(admin_find),
            first_id: Mutex::new(admin_first_id),
        }),
        Arc::new(MockSessionRepo {
            find_result: Mutex::new(session_find),
            session_for_refresh: Mutex::new(session_for_refresh),
            ..Default::default()
        }),
        Arc::new(MockApiKeyRepo {
            hashes: api_key_hashes,
        }),
        Arc::new(MockSystemConfigRepo),
        24,
        720,
    )
}

// -- Tests ----------------------------------------------------------------

#[tokio::test]
async fn login_success() {
    let hash = argon2_hash("correct-password");
    let svc = make_auth_service(Some(("admin-1".to_owned(), hash)), None, None, vec![]);

    let result = svc.login("admin", "correct-password", false).await;
    assert!(result.is_ok());
    let login = result.unwrap();
    assert!(!login.token.is_empty());
    assert_eq!(login.max_age_seconds, 24 * 3600);
}

#[tokio::test]
async fn login_success_with_remember_me() {
    let hash = argon2_hash("correct-password");
    let svc = make_auth_service(Some(("admin-1".to_owned(), hash)), None, None, vec![]);

    let result = svc.login("admin", "correct-password", true).await;
    assert!(result.is_ok());
    let login = result.unwrap();
    assert!(!login.token.is_empty());
    assert_eq!(login.max_age_seconds, 720 * 3600);
}

#[tokio::test]
async fn login_with_malformed_stored_hash_returns_internal() {
    // A corrupt password hash in the admins table must surface as Internal,
    // not masquerade as bad credentials.
    let svc = make_auth_service(
        Some(("admin-1".to_owned(), "not-an-argon2-hash".to_owned())),
        None,
        None,
        vec![],
    );

    let result = svc.login("admin", "any-password", false).await;
    assert!(matches!(result, Err(AppError::Internal(_))));
}

#[tokio::test]
async fn login_wrong_password() {
    let hash = argon2_hash("correct-password");
    let svc = make_auth_service(Some(("admin-1".to_owned(), hash)), None, None, vec![]);

    let result = svc.login("admin", "wrong-password", false).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn login_user_not_found() {
    let svc = make_auth_service(None, None, None, vec![]);

    let result = svc.login("nobody", "password", false).await;
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
async fn validate_session_with_malformed_admin_id_returns_internal() {
    // A session row whose admin_id is not a UUID must error rather than
    // silently authenticate or deny.
    let svc = make_auth_service(None, None, Some("not-a-uuid".to_owned()), vec![]);

    let result = svc.validate_session("any-token").await;
    assert!(matches!(result, Err(AppError::Internal(_))));
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

#[tokio::test]
async fn validate_api_key_no_keys_returns_none() {
    let svc = make_auth_service(None, None, None, vec![]);

    let result = svc.validate_api_key("any-key").await.unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn validate_api_key_skips_malformed_hash() {
    // One malformed hash and one valid hash -- the valid one should still match.
    let valid_hash = argon2_hash("valid-key");
    let svc = make_auth_service(
        None,
        Some("00000000-0000-0000-0000-000000000001".to_owned()),
        None,
        vec![
            ("key-bad".to_owned(), "not-a-valid-argon2-hash".to_owned()),
            ("key-good".to_owned(), valid_hash),
        ],
    );

    let result = svc.validate_api_key("valid-key").await.unwrap();
    assert!(result.is_some());
}

#[tokio::test]
async fn current_admin_username_requires_admin() {
    // No auth context → require_admin rejects before touching the repo.
    let svc = make_auth_service(None, None, None, vec![]);
    let result = svc.current_admin_username().await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}

#[tokio::test]
async fn current_admin_username_returns_the_callers_username() {
    let svc = make_auth_service(None, None, None, vec![]);
    let result = auth_context::with_context(
        AuthContext::Admin {
            admin_id: Uuid::nil(),
        },
        async { svc.current_admin_username().await },
    )
    .await;
    assert_eq!(result.unwrap(), "admin");
}

#[tokio::test]
async fn is_setup_completed_delegates() {
    let svc = make_auth_service(None, None, None, vec![]);
    // Default MockSystemConfigRepo returns false.
    let result = svc.is_setup_completed().await.unwrap();
    assert!(!result);
}

#[tokio::test]
async fn cleanup_expired_sessions_requires_admin() {
    // No auth context → require_admin rejects before touching the repo.
    let svc = make_auth_service(None, None, None, vec![]);
    let result = svc.cleanup_expired_sessions().await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}

#[tokio::test]
async fn cleanup_expired_sessions_delegates_under_admin_context() {
    // Under an admin context it delegates to SessionRepository::delete_expired
    // and returns the row count (MockSessionRepo returns 0).
    let svc = make_auth_service(None, None, None, vec![]);
    let result = auth_context::with_context(
        AuthContext::Admin {
            admin_id: Uuid::nil(),
        },
        async { svc.cleanup_expired_sessions().await },
    )
    .await;
    assert_eq!(result.unwrap(), 0);
}

#[tokio::test]
async fn refresh_session_success() {
    // Session exists and was created as remember_me=true → rotates token and extends expiry.
    let admin_uuid = "00000000-0000-0000-0000-000000000001";
    let svc = make_auth_service(None, None, Some(admin_uuid.to_owned()), vec![]);
    let result = auth_context::with_context(
        AuthContext::Admin {
            admin_id: Uuid::parse_str(admin_uuid).unwrap(),
        },
        async { svc.refresh_session("any-token").await },
    )
    .await;
    assert!(result.is_ok());
    let r = result.unwrap();
    // Token must be rotated: returned token must be non-empty and different from the input.
    assert!(!r.token.is_empty());
    assert_ne!(r.token, "any-token");
    assert_eq!(r.max_age_seconds, 720 * 3600);
}

#[tokio::test]
async fn refresh_session_not_remember_me_returns_forbidden() {
    // Session exists but remember_me=false → Forbidden.
    let admin_uuid = "00000000-0000-0000-0000-000000000001";
    // Build a service with a MockSessionRepo where remember_me=false.
    let svc = AuthServiceImpl::new(
        Arc::new(MockAdminRepo {
            find_result: Mutex::new(None),
            first_id: Mutex::new(None),
        }),
        Arc::new(MockSessionRepo {
            find_result: Mutex::new(Some(admin_uuid.to_owned())),
            session_for_refresh: Mutex::new(Some((
                admin_uuid.to_owned(),
                false,
                (chrono::Utc::now() - chrono::Duration::days(30)).to_rfc3339(),
            ))),
            ..Default::default()
        }),
        Arc::new(MockApiKeyRepo { hashes: vec![] }),
        Arc::new(MockSystemConfigRepo),
        24,
        720,
    );
    let result = auth_context::with_context(
        AuthContext::Admin {
            admin_id: Uuid::parse_str(admin_uuid).unwrap(),
        },
        async { svc.refresh_session("any-token").await },
    )
    .await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}

/// Build a service whose session row for refresh is exactly `row`.
fn make_refresh_service(row: (String, bool, String)) -> AuthServiceImpl {
    AuthServiceImpl::new(
        Arc::new(MockAdminRepo {
            find_result: Mutex::new(None),
            first_id: Mutex::new(None),
        }),
        Arc::new(MockSessionRepo {
            session_for_refresh: Mutex::new(Some(row)),
            ..Default::default()
        }),
        Arc::new(MockApiKeyRepo { hashes: vec![] }),
        Arc::new(MockSystemConfigRepo),
        24,
        720,
    )
}

#[tokio::test]
async fn refresh_session_with_malformed_admin_id_returns_internal() {
    // A session row whose admin_id is not a UUID must error out, not refresh.
    let svc = make_refresh_service((
        "not-a-uuid".to_owned(),
        true,
        (chrono::Utc::now() - chrono::Duration::days(30)).to_rfc3339(),
    ));
    let result = auth_context::with_context(
        AuthContext::Admin {
            admin_id: Uuid::nil(),
        },
        async { svc.refresh_session("any-token").await },
    )
    .await;
    assert!(matches!(result, Err(AppError::Internal(_))));
}

#[tokio::test]
async fn refresh_session_with_malformed_created_at_returns_internal() {
    // A session row whose created_at is not RFC 3339 must error out — the
    // absolute-lifetime cap cannot be enforced without it.
    let admin_uuid = "00000000-0000-0000-0000-000000000001";
    let svc = make_refresh_service((admin_uuid.to_owned(), true, "not-a-timestamp".to_owned()));
    let result = auth_context::with_context(
        AuthContext::Admin {
            admin_id: Uuid::parse_str(admin_uuid).unwrap(),
        },
        async { svc.refresh_session("any-token").await },
    )
    .await;
    assert!(matches!(result, Err(AppError::Internal(_))));
}

#[tokio::test]
async fn logout_session_requires_admin() {
    // No auth context → require_admin rejects before touching the repo.
    let svc = make_auth_service(None, None, None, vec![]);
    let result = svc.logout_session("any-token").await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}

#[tokio::test]
async fn logout_session_deletes_the_sessions_row_by_token_hash() {
    let admin_uuid = "00000000-0000-0000-0000-000000000001";
    let sessions = Arc::new(MockSessionRepo {
        find_result: Mutex::new(Some(admin_uuid.to_owned())),
        ..Default::default()
    });
    let svc = AuthServiceImpl::new(
        Arc::new(MockAdminRepo {
            find_result: Mutex::new(None),
            first_id: Mutex::new(None),
        }),
        Arc::clone(&sessions) as Arc<dyn SessionRepository>,
        Arc::new(MockApiKeyRepo { hashes: vec![] }),
        Arc::new(MockSystemConfigRepo),
        24,
        720,
    );

    let result = auth_context::with_context(
        AuthContext::Admin {
            admin_id: Uuid::parse_str(admin_uuid).unwrap(),
        },
        async { svc.logout_session("raw-token").await },
    )
    .await;
    assert!(result.is_ok());

    // The repo receives the SHA-256 hash of the raw token, never the token itself.
    let deleted = sessions.deleted_hashes.lock().unwrap().clone();
    let expected_hash = {
        use sha2::{Digest, Sha256};
        hex::encode(Sha256::digest("raw-token".as_bytes()))
    };
    assert_eq!(deleted, vec![expected_hash]);

    // The session no longer resolves afterwards.
    let after = svc.validate_session("raw-token").await.unwrap();
    assert!(after.is_none());
}

#[tokio::test]
async fn logout_session_is_idempotent_when_session_already_gone() {
    // No session row exists (delete affects 0 rows) → still Ok, the desired
    // end state (no server-side session) already holds.
    let svc = make_auth_service(None, None, None, vec![]);
    let result = auth_context::with_context(
        AuthContext::Admin {
            admin_id: Uuid::nil(),
        },
        async { svc.logout_session("any-token").await },
    )
    .await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn refresh_session_expired_returns_unauthorized() {
    // Session does not exist (find returns None) → Unauthorized.
    let svc = make_auth_service(None, None, None, vec![]);
    let result = auth_context::with_context(
        AuthContext::Admin {
            admin_id: Uuid::nil(),
        },
        async { svc.refresh_session("any-token").await },
    )
    .await;
    assert!(matches!(result, Err(AppError::Unauthorized(_))));
}
