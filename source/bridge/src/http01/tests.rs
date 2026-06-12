use std::sync::Arc;

use async_trait::async_trait;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use chrono::{DateTime, Utc};
use tower::ServiceExt;

use super::{build_router, is_valid_acme_token};
use crate::repository::{SealedCert, TlsRepository};

// ── is_valid_acme_token (pure) ────────────────────────────────────────────

#[test]
fn accepts_realistic_token() {
    // A typical Let's Encrypt HTTP-01 token (base64url, 43 chars).
    assert!(is_valid_acme_token(
        "evaGxfADs6pSRb2Lav9IZf6DuOmjmQAfW ".trim()
    ));
    assert!(is_valid_acme_token("abcXYZ012-_"));
}

#[test]
fn rejects_empty() {
    assert!(!is_valid_acme_token(""));
}

#[test]
fn rejects_overlong() {
    assert!(!is_valid_acme_token(&"a".repeat(129)));
}

#[test]
fn rejects_path_traversal_and_specials() {
    assert!(!is_valid_acme_token("../../etc/passwd"));
    assert!(!is_valid_acme_token("token with space"));
    assert!(!is_valid_acme_token("token/slash"));
    assert!(!is_valid_acme_token("token.dot"));
    assert!(!is_valid_acme_token("token%2e"));
}

// ── Router handlers (health / challenge / fallback) ───────────────────────

/// What the mock repo's `get_challenge` should return.
enum Resp {
    Found(&'static str),
    Missing,
    Error,
}

struct MockRepo(Resp);

#[async_trait]
impl TlsRepository for MockRepo {
    async fn get_challenge(&self, _token: &str) -> anyhow::Result<Option<String>> {
        match self.0 {
            Resp::Found(s) => Ok(Some(s.to_owned())),
            Resp::Missing => Ok(None),
            Resp::Error => Err(anyhow::anyhow!("challenge store unavailable")),
        }
    }

    async fn load_cert(&self, _fqdn: &str) -> anyhow::Result<Option<SealedCert>> {
        unimplemented!()
    }
    async fn store_cert(
        &self,
        _fqdn: &str,
        _sealed_blob: &[u8],
        _nonce: &[u8],
        _not_after: DateTime<Utc>,
    ) -> anyhow::Result<i64> {
        unimplemented!()
    }
    async fn put_challenge(
        &self,
        _token: &str,
        _key_authorization: &str,
        _expires_at: DateTime<Utc>,
    ) -> anyhow::Result<()> {
        unimplemented!()
    }
    async fn delete_challenge(&self, _token: &str) -> anyhow::Result<()> {
        unimplemented!()
    }
    async fn delete_expired_challenges(&self, _now: DateTime<Utc>) -> anyhow::Result<u64> {
        unimplemented!()
    }
    async fn acquire_lease(
        &self,
        _fqdn: &str,
        _holder: &str,
        _locked_until: DateTime<Utc>,
    ) -> anyhow::Result<bool> {
        unimplemented!()
    }
    async fn release_lease(&self, _fqdn: &str, _holder: &str) -> anyhow::Result<()> {
        unimplemented!()
    }
}

async fn get(repo: Resp, uri: &str) -> (StatusCode, String) {
    let router = build_router(Arc::new(MockRepo(repo)));
    let resp = router
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = resp.status();
    let body = axum::body::to_bytes(resp.into_body(), 64 * 1024)
        .await
        .unwrap();
    (status, String::from_utf8_lossy(&body).into_owned())
}

#[tokio::test]
async fn health_is_ok() {
    let (status, body) = get(Resp::Missing, "/health").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, "ok");
}

#[tokio::test]
async fn challenge_found_returns_key_authorization() {
    let (status, body) = get(
        Resp::Found("token123.keyauth"),
        "/.well-known/acme-challenge/validtoken123",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, "token123.keyauth");
}

#[tokio::test]
async fn challenge_absent_is_404() {
    let (status, _) = get(Resp::Missing, "/.well-known/acme-challenge/validtoken123").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn challenge_repo_error_is_500() {
    let (status, _) = get(Resp::Error, "/.well-known/acme-challenge/validtoken123").await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn challenge_malformed_token_is_404_without_db_hit() {
    // A path-traversal-looking token fails the shape guard before any lookup;
    // the repo is set to error to prove the DB is never consulted.
    let (status, _) = get(Resp::Error, "/.well-known/acme-challenge/bad..token").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn unknown_path_falls_back_to_404() {
    let (status, _) = get(Resp::Missing, "/nope").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
