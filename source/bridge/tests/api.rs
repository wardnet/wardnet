//! Full-stack integration tests for the bridge HTTP API.
//!
//! These tests build the complete Axum router with mock in-memory repositories
//! and a `MockDnsProvider`, then drive requests through
//! [`tower::ServiceExt::oneshot`].
//!
//! # Test conventions
//!
//! - Every test creates its own isolated state via `test_state()`.
//! - Challenges are inserted directly with `difficulty = 0` so no real `PoW`
//!   computation is needed.
//! - Ed25519 signing uses a deterministic test key derived from `[1u8; 32]`.
//! - The loopback peer (`127.0.0.1`) is injected via `MockConnectInfo` so
//!   handlers that call `client_ip()` see a non-forwarded address.

mod common;

use std::future::IntoFuture;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::extract::connect_info::MockConnectInfo;
use axum::http::{Request, StatusCode};
use base64::Engine as _;
use chrono::Utc;
use ed25519_dalek::{Signer, SigningKey};
use futures_util::{SinkExt as _, StreamExt as _};
use http_body_util::BodyExt;
use sha2::{Digest, Sha256};
use tokio_tungstenite::tungstenite::Message as WsMessage;
use tower::ServiceExt as _;
use uuid::Uuid;

use common::{MockChallengeRepository, MockInstallRepository};
use wardnet_bridge::config::Config;
use wardnet_bridge::dns_provider::DnsProvider;
use wardnet_bridge::repository::{Install, InstallRepository, RegistrationChallenge};
use wardnet_bridge::state::AppState;
use wardnet_bridge::tunnel::TunnelRegistry;

// ── Mock DNS provider ────────────────────────────────────────────────────────

#[derive(Debug)]
enum DnsCall {
    UpsertA,
    UpsertTxt,
    DeleteRecord,
}

struct MockDnsProvider {
    calls: Mutex<Vec<DnsCall>>,
    error: Option<String>,
}

impl MockDnsProvider {
    fn new() -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
            error: None,
        }
    }

    fn with_error(msg: &str) -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
            error: Some(msg.to_string()),
        }
    }

    fn call_count(&self) -> usize {
        self.calls.lock().unwrap().len()
    }
}

#[async_trait::async_trait]
impl DnsProvider for MockDnsProvider {
    async fn upsert_a_record(
        &self,
        fqdn: &str,
        _ip: &str,
        _existing_record_id: Option<&str>,
    ) -> anyhow::Result<String> {
        if let Some(e) = &self.error {
            return Err(anyhow::anyhow!("{e}"));
        }
        self.calls.lock().unwrap().push(DnsCall::UpsertA);
        Ok(format!("cf-a-{fqdn}"))
    }

    async fn upsert_txt_record(
        &self,
        fqdn: &str,
        _content: &str,
        _existing_record_id: Option<&str>,
    ) -> anyhow::Result<String> {
        if let Some(e) = &self.error {
            return Err(anyhow::anyhow!("{e}"));
        }
        self.calls.lock().unwrap().push(DnsCall::UpsertTxt);
        Ok(format!("cf-txt-{fqdn}"))
    }

    async fn delete_record(&self, _record_id: &str) -> anyhow::Result<()> {
        if let Some(e) = &self.error {
            return Err(anyhow::anyhow!("{e}"));
        }
        self.calls.lock().unwrap().push(DnsCall::DeleteRecord);
        Ok(())
    }
}

// ── Shared test fixtures ─────────────────────────────────────────────────────

fn test_config() -> Config {
    Config {
        listen_addr: "127.0.0.1:0".to_string(),
        database_url: "postgres://ignored".to_string(),
        cloudflare_api_token: "test-cf-token".to_string(),
        cloudflare_zone_id: "test-cf-zone".to_string(),
        region: "test".to_string(),
        subdomain_parent: "test.wardnet.local".to_string(),
        sni_listen_addr: "0.0.0.0:443".to_string(),
        dot_listen_addr: "0.0.0.0:853".to_string(),
        caddy_addr: "127.0.0.1:8443".to_string(),
        bridge_hostname: "bridge.test.wardnet.network".to_string(),
    }
}

fn test_state() -> (AppState, Arc<MockDnsProvider>) {
    let dns = Arc::new(MockDnsProvider::new());
    let installs = Arc::new(MockInstallRepository::new());
    let challenges = Arc::new(MockChallengeRepository::new());
    let tunnel_registry = Arc::new(TunnelRegistry::new());
    let state = AppState::new(
        test_config(),
        dummy_pools(),
        installs as Arc<dyn InstallRepository>,
        challenges as Arc<dyn wardnet_bridge::repository::ChallengeRepository>,
        Arc::clone(&dns) as Arc<dyn DnsProvider>,
        tunnel_registry,
    );
    (state, dns)
}

fn test_state_with_error_dns(dns: Arc<MockDnsProvider>) -> AppState {
    let installs = Arc::new(MockInstallRepository::new());
    let challenges = Arc::new(MockChallengeRepository::new());
    let tunnel_registry = Arc::new(TunnelRegistry::new());
    AppState::new(
        test_config(),
        dummy_pools(),
        installs as Arc<dyn InstallRepository>,
        challenges as Arc<dyn wardnet_bridge::repository::ChallengeRepository>,
        dns as Arc<dyn DnsProvider>,
        tunnel_registry,
    )
}

/// Build an `AppState` backed by mock repos and the given DNS mock.
fn test_state_dns(dns: Arc<MockDnsProvider>) -> AppState {
    test_state_with_error_dns(dns)
}

/// Build the Axum router under test with a fixed loopback peer address.
fn test_app(state: AppState) -> axum::Router {
    wardnet_bridge::api::router(state)
        .layer(MockConnectInfo(SocketAddr::from(([127, 0, 0, 1], 12345))))
}

/// Produce a dummy `DbPools` that is stored in `AppState` but never accessed
/// (mock repositories bypass the pool entirely).
fn dummy_pools() -> wardnet_bridge::db::DbPools {
    // SAFETY: The AppState stores DbPools but mock repos own their own state
    // and never touch the pool. We construct a pool that connects lazily so
    // it doesn't fail at construction time.
    // Use sqlx's lazy connect: PgPool::connect_lazy returns immediately.
    let pool = sqlx::PgPool::connect_lazy("postgres://postgres:postgres@127.0.0.1:5432/dummy")
        .expect("lazy connect should not fail at construction");
    wardnet_bridge::db::DbPools::single(pool)
}

/// Ed25519 signing key for tests — deterministic, derived from `[1u8; 32]`.
fn test_signing_key() -> SigningKey {
    SigningKey::from_bytes(&[1u8; 32])
}

/// Return the verifying-key bytes and their base64 encoding for the test key.
fn test_pub_key() -> ([u8; 32], String) {
    let key = test_signing_key();
    let bytes = key.verifying_key().to_bytes();
    let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
    (bytes, b64)
}

/// Return a deterministic `(raw_token_hex, token_hash_hex)` pair.
fn test_bearer_token() -> (String, String) {
    let raw = hex::encode([42u8; 32]);
    let hash = hex::encode(Sha256::digest(raw.as_bytes()));
    (raw, hash)
}

/// Insert a test install row directly and return the install + raw token.
async fn insert_test_install(state: &AppState, name: &str) -> (Install, String) {
    let (raw_token, token_hash) = test_bearer_token();
    let (pub_key_bytes, public_key) = test_pub_key();
    let now = Utc::now();
    let install = Install {
        id: Uuid::new_v4().to_string(),
        name: name.to_string(),
        public_key,
        pub_key_bytes,
        token_hash,
        ip: None,
        cf_a_record_id: None,
        cf_acme_record_id: None,
        created_at: now,
        updated_at: now,
    };
    state.installs().insert(&install).await.unwrap();
    (install, raw_token)
}

/// Insert a challenge with `difficulty = 0` (any `proof` satisfies it).
async fn insert_easy_challenge(state: &AppState, remote_ip: &str) -> RegistrationChallenge {
    let now = Utc::now();
    let challenge = RegistrationChallenge {
        id: Uuid::new_v4().to_string(),
        nonce: hex::encode([7u8; 32]),
        difficulty: 0,
        remote_ip: remote_ip.to_string(),
        created_at: now,
        expires_at: now + chrono::Duration::minutes(5),
        used_at: None,
    };
    state.challenges().insert(&challenge).await.unwrap();
    challenge
}

/// Build a signed request for an authenticated endpoint.
fn signed_request(
    method: &str,
    path: &str,
    body: &[u8],
    bearer: &str,
    signing_key: &SigningKey,
) -> Request<Body> {
    signed_request_at(
        method,
        path,
        body,
        bearer,
        signing_key,
        Utc::now().timestamp(),
    )
}

/// Like `signed_request` but with a caller-supplied timestamp.
fn signed_request_at(
    method: &str,
    path: &str,
    body: &[u8],
    bearer: &str,
    signing_key: &SigningKey,
    timestamp: i64,
) -> Request<Body> {
    let body_hash = hex::encode(Sha256::digest(body));
    let payload = format!("{method}\n{path}\n{timestamp}\n{body_hash}");
    let signature = signing_key.sign(payload.as_bytes());
    let sig_b64 = base64::engine::general_purpose::STANDARD.encode(signature.to_bytes());

    Request::builder()
        .method(method)
        .uri(path)
        .header("Authorization", format!("Bearer {bearer}"))
        .header("X-Wardnet-Timestamp", timestamp.to_string())
        .header("X-Wardnet-Signature", sig_b64)
        .header("Content-Type", "application/json")
        .body(Body::from(body.to_vec()))
        .unwrap()
}

/// Collect the response body into a UTF-8 string.
async fn body_string(body: axum::body::Body) -> String {
    let bytes = body.collect().await.unwrap().to_bytes();
    String::from_utf8(bytes.to_vec()).unwrap()
}

// ── Health endpoint ──────────────────────────────────────────────────────────

#[tokio::test]
async fn health_returns_200() {
    let (state, _dns) = test_state();
    let app = test_app(state);
    let req = Request::builder()
        .uri("/v1/health")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

// ── Challenge endpoint ───────────────────────────────────────────────────────

#[tokio::test]
async fn get_challenge_returns_200_with_fields() {
    let (state, _dns) = test_state();
    let app = test_app(state);
    let req = Request::builder()
        .uri("/v1/register/challenge")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let json: serde_json::Value =
        serde_json::from_str(&body_string(resp.into_body()).await).unwrap();
    assert!(json["challenge_id"].as_str().is_some());
    assert!(json["nonce"].as_str().is_some());
    assert!(json["difficulty"].as_u64().is_some());
    assert!(json["expires_at"].as_str().is_some());
}

#[tokio::test]
async fn get_challenge_rate_limited_at_20_per_hour() {
    let (state, _dns) = test_state();

    for _ in 0..20 {
        let c = RegistrationChallenge {
            id: Uuid::new_v4().to_string(),
            nonce: hex::encode([0u8; 32]),
            difficulty: 0,
            remote_ip: "127.0.0.1".to_string(),
            created_at: Utc::now() - chrono::Duration::minutes(10),
            expires_at: Utc::now() + chrono::Duration::minutes(5),
            used_at: None,
        };
        state.challenges().insert(&c).await.unwrap();
    }

    let app = test_app(state);
    let req = Request::builder()
        .uri("/v1/register/challenge")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
}

// ── Name-availability endpoint ───────────────────────────────────────────────

#[tokio::test]
async fn name_available_for_fresh_name() {
    let (state, _dns) = test_state();
    let app = test_app(state);
    let req = Request::builder()
        .uri("/v1/names/happy-einstein/available")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let json: serde_json::Value =
        serde_json::from_str(&body_string(resp.into_body()).await).unwrap();
    assert_eq!(json["available"], true);
}

#[tokio::test]
async fn name_unavailable_when_already_registered() {
    let (state, _dns) = test_state();
    insert_test_install(&state, "happy-einstein").await;

    let app = test_app(state);
    let req = Request::builder()
        .uri("/v1/names/happy-einstein/available")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let json: serde_json::Value =
        serde_json::from_str(&body_string(resp.into_body()).await).unwrap();
    assert_eq!(json["available"], false);
}

#[tokio::test]
async fn name_unavailable_for_reserved_slug() {
    let (state, _dns) = test_state();
    let app = test_app(state);
    for reserved in &["www", "admin", "us", "api"] {
        let req = Request::builder()
            .uri(format!("/v1/names/{reserved}/available"))
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        let json: serde_json::Value =
            serde_json::from_str(&body_string(resp.into_body()).await).unwrap();
        assert_eq!(
            json["available"], false,
            "expected {reserved} to be unavailable"
        );
    }
}

#[tokio::test]
async fn name_unavailable_for_syntactically_invalid_name() {
    let (state, _dns) = test_state();
    let app = test_app(state);
    for invalid in &["-bad", "bad-", "ab", "ALLCAPS", "has space"] {
        let encoded = invalid.replace(' ', "%20");
        let req = Request::builder()
            .uri(format!("/v1/names/{encoded}/available"))
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        let json: serde_json::Value =
            serde_json::from_str(&body_string(resp.into_body()).await).unwrap();
        assert_eq!(
            json["available"], false,
            "expected '{invalid}' to be unavailable"
        );
    }
}

// ── Register endpoint ────────────────────────────────────────────────────────

fn register_body(
    name: &str,
    pub_key_b64: &str,
    challenge_id: &str,
    proof: u64,
) -> serde_json::Value {
    serde_json::json!({
        "name": name,
        "public_key": pub_key_b64,
        "challenge_id": challenge_id,
        "proof": proof,
    })
}

#[tokio::test]
async fn register_success_creates_install_and_returns_token() {
    let (state, _dns) = test_state();
    let challenge = insert_easy_challenge(&state, "127.0.0.1").await;
    let (_, pub_key_b64) = test_pub_key();

    let body = register_body("happy-einstein", &pub_key_b64, &challenge.id, 0);
    let app = test_app(state);
    let req = Request::builder()
        .method("POST")
        .uri("/v1/register")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    let json: serde_json::Value =
        serde_json::from_str(&body_string(resp.into_body()).await).unwrap();
    assert!(json["id"].as_str().is_some(), "must return an id");
    assert!(
        json["bearer_token"].as_str().is_some(),
        "must return a bearer_token"
    );
    assert_eq!(json["region"], "test");
    assert!(
        json["subdomain"]
            .as_str()
            .unwrap()
            .ends_with(".test.wardnet.local"),
        "subdomain should end with subdomain_parent"
    );
}

#[tokio::test]
async fn register_returns_400_for_invalid_name() {
    let (state, _dns) = test_state();
    let challenge = insert_easy_challenge(&state, "127.0.0.1").await;
    let (_, pub_key_b64) = test_pub_key();

    for bad_name in &["AB", "-foo", "foo-", "a b", "CAPS"] {
        let body = register_body(bad_name, &pub_key_b64, &challenge.id, 0);
        let app = test_app(state.clone());
        let req = Request::builder()
            .method("POST")
            .uri("/v1/register")
            .header("Content-Type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::BAD_REQUEST,
            "expected 400 for name '{bad_name}'"
        );
    }
}

#[tokio::test]
async fn register_returns_400_for_reserved_name() {
    let (state, _dns) = test_state();
    let challenge = insert_easy_challenge(&state, "127.0.0.1").await;
    let (_, pub_key_b64) = test_pub_key();

    let body = register_body("www", &pub_key_b64, &challenge.id, 0);
    let app = test_app(state);
    let req = Request::builder()
        .method("POST")
        .uri("/v1/register")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn register_returns_400_for_invalid_public_key() {
    let (state, _dns) = test_state();
    let challenge = insert_easy_challenge(&state, "127.0.0.1").await;

    let body = register_body("test-name", "not!valid!base64", &challenge.id, 0);
    let app = test_app(state.clone());
    let req = Request::builder()
        .method("POST")
        .uri("/v1/register")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    let short_key = base64::engine::general_purpose::STANDARD.encode([0u8; 31]);
    let body2 = register_body("test-name", &short_key, &challenge.id, 0);
    let app2 = test_app(state);
    let req2 = Request::builder()
        .method("POST")
        .uri("/v1/register")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&body2).unwrap()))
        .unwrap();
    let resp2 = app2.oneshot(req2).await.unwrap();
    assert_eq!(resp2.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn register_returns_400_for_unknown_challenge() {
    let (state, _dns) = test_state();
    let (_, pub_key_b64) = test_pub_key();

    let body = register_body("test-name", &pub_key_b64, &Uuid::new_v4().to_string(), 0);
    let app = test_app(state);
    let req = Request::builder()
        .method("POST")
        .uri("/v1/register")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let json: serde_json::Value =
        serde_json::from_str(&body_string(resp.into_body()).await).unwrap();
    assert!(
        json["error"]
            .as_str()
            .unwrap()
            .contains("unknown challenge_id")
    );
}

#[tokio::test]
async fn register_returns_400_for_expired_challenge() {
    let (state, _dns) = test_state();
    let (_, pub_key_b64) = test_pub_key();

    let now = Utc::now();
    let expired = RegistrationChallenge {
        id: Uuid::new_v4().to_string(),
        nonce: hex::encode([1u8; 32]),
        difficulty: 0,
        remote_ip: "127.0.0.1".to_string(),
        created_at: now - chrono::Duration::minutes(10),
        expires_at: now - chrono::Duration::minutes(1),
        used_at: None,
    };
    state.challenges().insert(&expired).await.unwrap();

    let body = register_body("test-name", &pub_key_b64, &expired.id, 0);
    let app = test_app(state);
    let req = Request::builder()
        .method("POST")
        .uri("/v1/register")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let json: serde_json::Value =
        serde_json::from_str(&body_string(resp.into_body()).await).unwrap();
    assert!(json["error"].as_str().unwrap().contains("expired"));
}

#[tokio::test]
async fn register_returns_400_when_challenge_issued_to_different_ip() {
    let (state, _dns) = test_state();
    let (_, pub_key_b64) = test_pub_key();

    let now = Utc::now();
    let challenge = RegistrationChallenge {
        id: Uuid::new_v4().to_string(),
        nonce: hex::encode([2u8; 32]),
        difficulty: 0,
        remote_ip: "9.9.9.9".to_string(),
        created_at: now,
        expires_at: now + chrono::Duration::minutes(5),
        used_at: None,
    };
    state.challenges().insert(&challenge).await.unwrap();

    let body = register_body("test-name", &pub_key_b64, &challenge.id, 0);
    let app = test_app(state);
    let req = Request::builder()
        .method("POST")
        .uri("/v1/register")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let json: serde_json::Value =
        serde_json::from_str(&body_string(resp.into_body()).await).unwrap();
    assert!(
        json["error"].as_str().unwrap().contains("IP"),
        "error should mention IP address"
    );
}

#[tokio::test]
async fn register_returns_400_for_failing_pow_proof() {
    let (state, _dns) = test_state();
    let (_, pub_key_b64) = test_pub_key();

    let now = Utc::now();
    let challenge = RegistrationChallenge {
        id: Uuid::new_v4().to_string(),
        nonce: hex::encode([3u8; 32]),
        difficulty: 24,
        remote_ip: "127.0.0.1".to_string(),
        created_at: now,
        expires_at: now + chrono::Duration::minutes(5),
        used_at: None,
    };
    state.challenges().insert(&challenge).await.unwrap();

    let body = register_body("test-name", &pub_key_b64, &challenge.id, 0);
    let app = test_app(state);
    let req = Request::builder()
        .method("POST")
        .uri("/v1/register")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let json: serde_json::Value =
        serde_json::from_str(&body_string(resp.into_body()).await).unwrap();
    assert!(json["error"].as_str().unwrap().contains("proof-of-work"));
}

#[tokio::test]
async fn register_returns_409_when_name_is_taken() {
    let (state, _dns) = test_state();
    insert_test_install(&state, "happy-einstein").await;
    let challenge = insert_easy_challenge(&state, "127.0.0.1").await;
    let (_, pub_key_b64) = test_pub_key();

    let body = register_body("happy-einstein", &pub_key_b64, &challenge.id, 0);
    let app = test_app(state);
    let req = Request::builder()
        .method("POST")
        .uri("/v1/register")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CONFLICT);
    let json: serde_json::Value =
        serde_json::from_str(&body_string(resp.into_body()).await).unwrap();
    assert!(json["error"].as_str().unwrap().contains("taken"));
}

#[tokio::test]
async fn register_returns_400_when_challenge_already_consumed() {
    let (state, _dns) = test_state();
    let challenge = insert_easy_challenge(&state, "127.0.0.1").await;
    let (_, pub_key_b64) = test_pub_key();

    state
        .challenges()
        .consume(&challenge.id, Utc::now())
        .await
        .unwrap();

    let body = register_body("test-name", &pub_key_b64, &challenge.id, 0);
    let app = test_app(state);
    let req = Request::builder()
        .method("POST")
        .uri("/v1/register")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let json: serde_json::Value =
        serde_json::from_str(&body_string(resp.into_body()).await).unwrap();
    assert!(
        json["error"]
            .as_str()
            .unwrap()
            .contains("already been used")
    );
}

#[tokio::test]
async fn register_returns_429_when_rate_limit_reached() {
    let (state, _dns) = test_state();

    for _ in 0..3 {
        state
            .installs()
            .log_registration("127.0.0.1", Utc::now())
            .await
            .unwrap();
    }

    let challenge = insert_easy_challenge(&state, "127.0.0.1").await;
    let (_, pub_key_b64) = test_pub_key();
    let body = register_body("test-name", &pub_key_b64, &challenge.id, 0);

    let app = test_app(state);
    let req = Request::builder()
        .method("POST")
        .uri("/v1/register")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
}

// ── Auth middleware ──────────────────────────────────────────────────────────

#[tokio::test]
async fn auth_rejects_body_over_1_mib() {
    let (state, _dns) = test_state();
    let (install, raw_token) = insert_test_install(&state, "test-node").await;

    let app = test_app(state);
    let oversized = vec![b'x'; 1024 * 1024 + 1];
    let timestamp = Utc::now().timestamp();
    let req = Request::builder()
        .method("PUT")
        .uri(format!("/v1/installs/{}/ip", install.id))
        .header("Authorization", format!("Bearer {raw_token}"))
        .header("X-Wardnet-Timestamp", timestamp.to_string())
        .header("X-Wardnet-Signature", "dGVzdA==")
        .header("Content-Type", "application/json")
        .body(Body::from(oversized))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::PAYLOAD_TOO_LARGE);
    let json: serde_json::Value =
        serde_json::from_str(&body_string(resp.into_body()).await).unwrap();
    assert!(json["error"].as_str().unwrap().contains("1 MiB"));
}

#[tokio::test]
async fn auth_passes_unauthenticated_endpoints_without_header() {
    let (state, _dns) = test_state();
    let app = test_app(state);
    let req = Request::builder()
        .uri("/v1/health")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn auth_returns_401_when_bearer_prefix_is_missing() {
    let (state, _dns) = test_state();
    let (install, _raw_token) = insert_test_install(&state, "test-node").await;

    let app = test_app(state);
    let req = Request::builder()
        .method("PUT")
        .uri(format!("/v1/installs/{}/ip", install.id))
        .header("Authorization", "Token not-bearer")
        .header("X-Wardnet-Timestamp", Utc::now().timestamp().to_string())
        .header("X-Wardnet-Signature", "dGVzdA==")
        .header("Content-Type", "application/json")
        .body(Body::from(r#"{"ip":"203.0.113.1"}"#))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    let json: serde_json::Value =
        serde_json::from_str(&body_string(resp.into_body()).await).unwrap();
    assert!(
        json["error"]
            .as_str()
            .unwrap()
            .contains("Authorization header")
    );
}

#[tokio::test]
async fn auth_returns_401_for_unknown_bearer_token() {
    let (state, _dns) = test_state();
    let (install, _) = insert_test_install(&state, "test-node").await;

    let app = test_app(state);
    let req = Request::builder()
        .method("PUT")
        .uri(format!("/v1/installs/{}/ip", install.id))
        .header("Authorization", "Bearer unknowntoken")
        .header("X-Wardnet-Timestamp", Utc::now().timestamp().to_string())
        .header("X-Wardnet-Signature", "dGVzdA==")
        .header("Content-Type", "application/json")
        .body(Body::from(r#"{"ip":"203.0.113.1"}"#))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    let json: serde_json::Value =
        serde_json::from_str(&body_string(resp.into_body()).await).unwrap();
    assert_eq!(json["error"], "unknown bearer token");
}

#[tokio::test]
async fn auth_returns_401_when_timestamp_header_is_absent() {
    let (state, _dns) = test_state();
    let (install, raw_token) = insert_test_install(&state, "test-node").await;

    let app = test_app(state);
    let req = Request::builder()
        .method("PUT")
        .uri(format!("/v1/installs/{}/ip", install.id))
        .header("Authorization", format!("Bearer {raw_token}"))
        .header("X-Wardnet-Signature", "dGVzdA==")
        .header("Content-Type", "application/json")
        .body(Body::from(r#"{"ip":"203.0.113.1"}"#))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    let json: serde_json::Value =
        serde_json::from_str(&body_string(resp.into_body()).await).unwrap();
    assert!(json["error"].as_str().unwrap().contains("Timestamp"));
}

#[tokio::test]
async fn auth_returns_401_when_timestamp_is_not_a_number() {
    let (state, _dns) = test_state();
    let (install, raw_token) = insert_test_install(&state, "test-node").await;

    let app = test_app(state);
    let req = Request::builder()
        .method("PUT")
        .uri(format!("/v1/installs/{}/ip", install.id))
        .header("Authorization", format!("Bearer {raw_token}"))
        .header("X-Wardnet-Timestamp", "not-a-number")
        .header("X-Wardnet-Signature", "dGVzdA==")
        .header("Content-Type", "application/json")
        .body(Body::from(r#"{"ip":"203.0.113.1"}"#))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn auth_returns_401_when_timestamp_is_stale() {
    let (state, _dns) = test_state();
    let (install, raw_token) = insert_test_install(&state, "test-node").await;

    let app = test_app(state);
    let past_ts = Utc::now().timestamp() - 120;
    let req = Request::builder()
        .method("PUT")
        .uri(format!("/v1/installs/{}/ip", install.id))
        .header("Authorization", format!("Bearer {raw_token}"))
        .header("X-Wardnet-Timestamp", past_ts.to_string())
        .header("X-Wardnet-Signature", "dGVzdA==")
        .header("Content-Type", "application/json")
        .body(Body::from(r#"{"ip":"203.0.113.1"}"#))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    let json: serde_json::Value =
        serde_json::from_str(&body_string(resp.into_body()).await).unwrap();
    assert!(json["error"].as_str().unwrap().contains("window"));
}

#[tokio::test]
async fn auth_returns_401_for_invalid_signature() {
    let (state, _dns) = test_state();
    let (install, raw_token) = insert_test_install(&state, "test-node").await;

    let app = test_app(state);
    let bad_sig = base64::engine::general_purpose::STANDARD.encode([0u8; 64]);
    let req = Request::builder()
        .method("PUT")
        .uri(format!("/v1/installs/{}/ip", install.id))
        .header("Authorization", format!("Bearer {raw_token}"))
        .header("X-Wardnet-Timestamp", Utc::now().timestamp().to_string())
        .header("X-Wardnet-Signature", bad_sig)
        .header("Content-Type", "application/json")
        .body(Body::from(r#"{"ip":"203.0.113.1"}"#))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    let json: serde_json::Value =
        serde_json::from_str(&body_string(resp.into_body()).await).unwrap();
    assert!(json["error"].as_str().unwrap().contains("signature"));
}

#[tokio::test]
async fn auth_returns_401_when_signature_base64_is_invalid() {
    let (state, _dns) = test_state();
    let (install, raw_token) = insert_test_install(&state, "test-node").await;

    let app = test_app(state);
    let req = Request::builder()
        .method("PUT")
        .uri(format!("/v1/installs/{}/ip", install.id))
        .header("Authorization", format!("Bearer {raw_token}"))
        .header("X-Wardnet-Timestamp", Utc::now().timestamp().to_string())
        .header("X-Wardnet-Signature", "not!valid!base64!!!")
        .header("Content-Type", "application/json")
        .body(Body::from(r#"{"ip":"203.0.113.1"}"#))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn auth_returns_401_on_replayed_request() {
    let (state, _dns) = test_state();
    let (install, raw_token) = insert_test_install(&state, "test-node").await;
    let signing_key = test_signing_key();

    let timestamp = Utc::now().timestamp();
    let body = br#"{"ip":"8.8.8.8"}"#;
    let path = format!("/v1/installs/{}/ip", install.id);

    let app = test_app(state);

    let req1 = signed_request_at("PUT", &path, body, &raw_token, &signing_key, timestamp);
    let resp1 = app.clone().oneshot(req1).await.unwrap();
    assert_eq!(
        resp1.status(),
        StatusCode::NO_CONTENT,
        "first signed request should succeed"
    );

    let req2 = signed_request_at("PUT", &path, body, &raw_token, &signing_key, timestamp);
    let resp2 = app.oneshot(req2).await.unwrap();
    assert_eq!(
        resp2.status(),
        StatusCode::UNAUTHORIZED,
        "replayed request should be rejected"
    );
    let json: serde_json::Value =
        serde_json::from_str(&body_string(resp2.into_body()).await).unwrap();
    assert!(json["error"].as_str().unwrap().contains("replay"));
}

#[tokio::test]
async fn auth_missing_from_authenticated_endpoint_returns_401() {
    let (state, _dns) = test_state();
    let (install, _) = insert_test_install(&state, "test-node").await;

    let app = test_app(state);
    let req = Request::builder()
        .method("PUT")
        .uri(format!("/v1/installs/{}/ip", install.id))
        .header("Content-Type", "application/json")
        .body(Body::from(r#"{"ip":"203.0.113.1"}"#))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

// ── IP-update endpoint ───────────────────────────────────────────────────────

#[tokio::test]
async fn update_ip_success() {
    let (state, dns) = test_state();
    let (install, raw_token) = insert_test_install(&state, "test-node").await;
    let signing_key = test_signing_key();

    let body = br#"{"ip":"8.8.8.8"}"#;
    let path = format!("/v1/installs/{}/ip", install.id);
    let req = signed_request("PUT", &path, body, &raw_token, &signing_key);

    let app = test_app(state);
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        dns.call_count(),
        1,
        "one DNS upsert-A should have been made"
    );
}

#[tokio::test]
async fn update_ip_returns_400_for_invalid_ip_string() {
    let (state, _dns) = test_state();
    let (install, raw_token) = insert_test_install(&state, "test-node").await;
    let signing_key = test_signing_key();

    let body = br#"{"ip":"not-an-ip"}"#;
    let path = format!("/v1/installs/{}/ip", install.id);
    let req = signed_request("PUT", &path, body, &raw_token, &signing_key);

    let app = test_app(state);
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let json: serde_json::Value =
        serde_json::from_str(&body_string(resp.into_body()).await).unwrap();
    assert!(json["error"].as_str().unwrap().contains("valid IPv4"));
}

#[tokio::test]
async fn update_ip_returns_400_for_private_addresses() {
    let (state, _dns) = test_state();
    let (install, raw_token) = insert_test_install(&state, "test-node").await;
    let signing_key = test_signing_key();

    for private in &[
        "192.168.1.1",
        "10.0.0.1",
        "172.16.0.1",
        "127.0.0.1",
        "169.254.0.1",
    ] {
        let body = format!(r#"{{"ip":"{private}"}}"#).into_bytes();
        let path = format!("/v1/installs/{}/ip", install.id);
        let req = signed_request("PUT", &path, &body, &raw_token, &signing_key);

        let app = test_app(state.clone());
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::BAD_REQUEST,
            "expected 400 for private IP {private}"
        );
        let json: serde_json::Value =
            serde_json::from_str(&body_string(resp.into_body()).await).unwrap();
        assert!(
            json["error"].as_str().unwrap().contains("private"),
            "error for {private} should mention 'private'"
        );
    }
}

#[tokio::test]
async fn update_ip_returns_403_when_install_id_does_not_match_token() {
    let (state, _dns) = test_state();
    let (_, raw_token) = insert_test_install(&state, "test-node").await;
    let signing_key = test_signing_key();

    let other_id = Uuid::new_v4().to_string();
    let body = br#"{"ip":"203.0.113.1"}"#;
    let path = format!("/v1/installs/{other_id}/ip");
    let req = signed_request("PUT", &path, body, &raw_token, &signing_key);

    let app = test_app(state);
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn update_ip_returns_500_when_dns_fails() {
    let dns = Arc::new(MockDnsProvider::with_error("cloudflare unavailable"));
    let state = test_state_dns(dns);
    let (install, raw_token) = insert_test_install(&state, "test-node").await;
    let signing_key = test_signing_key();

    let body = br#"{"ip":"8.8.8.8"}"#;
    let path = format!("/v1/installs/{}/ip", install.id);
    let req = signed_request("PUT", &path, body, &raw_token, &signing_key);

    let app = test_app(state);
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

// ── ACME-challenge endpoint ──────────────────────────────────────────────────

#[tokio::test]
async fn set_acme_challenge_success() {
    let (state, dns) = test_state();
    let (install, raw_token) = insert_test_install(&state, "test-node").await;
    let signing_key = test_signing_key();

    let body = br#"{"value":"my-acme-token"}"#;
    let path = format!("/v1/installs/{}/acme-challenge", install.id);
    let req = signed_request("PUT", &path, body, &raw_token, &signing_key);

    let app = test_app(state);
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        dns.call_count(),
        1,
        "one DNS upsert-TXT should have been made"
    );
}

#[tokio::test]
async fn set_acme_challenge_returns_403_on_id_mismatch() {
    let (state, _dns) = test_state();
    let (_, raw_token) = insert_test_install(&state, "test-node").await;
    let signing_key = test_signing_key();

    let other_id = Uuid::new_v4().to_string();
    let body = br#"{"value":"my-acme-token"}"#;
    let path = format!("/v1/installs/{other_id}/acme-challenge");
    let req = signed_request("PUT", &path, body, &raw_token, &signing_key);

    let app = test_app(state);
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn set_acme_challenge_returns_500_when_dns_fails() {
    let dns = Arc::new(MockDnsProvider::with_error("cloudflare unavailable"));
    let state = test_state_dns(dns);
    let (install, raw_token) = insert_test_install(&state, "test-node").await;
    let signing_key = test_signing_key();

    let body = br#"{"value":"my-acme-token"}"#;
    let path = format!("/v1/installs/{}/acme-challenge", install.id);
    let req = signed_request("PUT", &path, body, &raw_token, &signing_key);

    let app = test_app(state);
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn delete_acme_challenge_deletes_dns_record_when_present() {
    let (state, dns) = test_state();
    let (install, raw_token) = insert_test_install(&state, "test-node").await;
    let signing_key = test_signing_key();

    state
        .installs()
        .update_acme_record(&install.id, Some("cf-txt-existing"), Utc::now())
        .await
        .unwrap();

    let body = b"";
    let path = format!("/v1/installs/{}/acme-challenge", install.id);
    let req = signed_request("DELETE", &path, body, &raw_token, &signing_key);

    let app = test_app(state);
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
    assert_eq!(dns.call_count(), 1, "one DNS delete should have been made");
}

#[tokio::test]
async fn delete_acme_challenge_is_noop_when_no_record_set() {
    let (state, dns) = test_state();
    let (install, raw_token) = insert_test_install(&state, "test-node").await;
    let signing_key = test_signing_key();

    let body = b"";
    let path = format!("/v1/installs/{}/acme-challenge", install.id);
    let req = signed_request("DELETE", &path, body, &raw_token, &signing_key);

    let app = test_app(state);
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        dns.call_count(),
        0,
        "no DNS calls expected when record absent"
    );
}

#[tokio::test]
async fn delete_acme_challenge_returns_403_on_id_mismatch() {
    let (state, _dns) = test_state();
    let (_, raw_token) = insert_test_install(&state, "test-node").await;
    let signing_key = test_signing_key();

    let other_id = Uuid::new_v4().to_string();
    let body = b"";
    let path = format!("/v1/installs/{other_id}/acme-challenge");
    let req = signed_request("DELETE", &path, body, &raw_token, &signing_key);

    let app = test_app(state);
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn delete_acme_challenge_returns_500_when_dns_fails() {
    let dns = Arc::new(MockDnsProvider::with_error("cloudflare unavailable"));
    let state = test_state_dns(dns);
    let (install, raw_token) = insert_test_install(&state, "test-node").await;
    let signing_key = test_signing_key();

    state
        .installs()
        .update_acme_record(&install.id, Some("cf-txt-exists"), Utc::now())
        .await
        .unwrap();

    let body = b"";
    let path = format!("/v1/installs/{}/acme-challenge", install.id);
    let req = signed_request("DELETE", &path, body, &raw_token, &signing_key);

    let app = test_app(state);
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

// ── Deregister endpoint ──────────────────────────────────────────────────────

#[tokio::test]
async fn deregister_success_with_no_dns_records() {
    let (state, dns) = test_state();
    let (install, raw_token) = insert_test_install(&state, "test-node").await;
    let signing_key = test_signing_key();

    let body = b"";
    let path = format!("/v1/installs/{}", install.id);
    let req = signed_request("DELETE", &path, body, &raw_token, &signing_key);

    let app = test_app(state);
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
    assert_eq!(dns.call_count(), 0, "no DNS calls when no records are set");
}

#[tokio::test]
async fn deregister_success_deletes_both_dns_records() {
    let (state, dns) = test_state();
    let (install, raw_token) = insert_test_install(&state, "test-node").await;
    let signing_key = test_signing_key();

    state
        .installs()
        .update_ip(&install.id, "8.8.8.8", "cf-a-id", Utc::now())
        .await
        .unwrap();
    state
        .installs()
        .update_acme_record(&install.id, Some("cf-txt-id"), Utc::now())
        .await
        .unwrap();

    let body = b"";
    let path = format!("/v1/installs/{}", install.id);
    let req = signed_request("DELETE", &path, body, &raw_token, &signing_key);

    let app = test_app(state);
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        dns.call_count(),
        2,
        "both A and TXT records should be deleted"
    );
}

#[tokio::test]
async fn deregister_success_with_only_a_record() {
    let (state, dns) = test_state();
    let (install, raw_token) = insert_test_install(&state, "test-node").await;
    let signing_key = test_signing_key();

    state
        .installs()
        .update_ip(&install.id, "8.8.8.8", "cf-a-id", Utc::now())
        .await
        .unwrap();

    let body = b"";
    let path = format!("/v1/installs/{}", install.id);
    let req = signed_request("DELETE", &path, body, &raw_token, &signing_key);

    let app = test_app(state);
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
    assert_eq!(dns.call_count(), 1, "only the A record should be deleted");
}

#[tokio::test]
async fn deregister_returns_403_when_install_id_does_not_match() {
    let (state, _dns) = test_state();
    let (_, raw_token) = insert_test_install(&state, "test-node").await;
    let signing_key = test_signing_key();

    let other_id = Uuid::new_v4().to_string();
    let body = b"";
    let path = format!("/v1/installs/{other_id}");
    let req = signed_request("DELETE", &path, body, &raw_token, &signing_key);

    let app = test_app(state);
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn deregister_returns_500_when_dns_fails() {
    let dns = Arc::new(MockDnsProvider::with_error("cloudflare unavailable"));
    let state = test_state_dns(dns);
    let (install, raw_token) = insert_test_install(&state, "test-node").await;
    let signing_key = test_signing_key();

    state
        .installs()
        .update_ip(&install.id, "8.8.8.8", "cf-a-id", Utc::now())
        .await
        .unwrap();

    let body = b"";
    let path = format!("/v1/installs/{}", install.id);
    let req = signed_request("DELETE", &path, body, &raw_token, &signing_key);

    let app = test_app(state);
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

// ── State accessors ──────────────────────────────────────────────────────────

#[tokio::test]
async fn config_accessors_return_expected_values() {
    let (state, _dns) = test_state();
    let cfg = state.config();
    assert_eq!(cfg.region, "test");
    assert_eq!(cfg.subdomain_parent, "test.wardnet.local");
    assert_eq!(cfg.install_fqdn("mynode"), "mynode.test.wardnet.local");
    assert_eq!(
        cfg.acme_fqdn("mynode"),
        "_acme-challenge.mynode.test.wardnet.local"
    );
}

#[tokio::test]
async fn state_tunnel_registry_returns_same_instance() {
    let (state, _dns) = test_state();
    let r1 = state.tunnel_registry();
    let r2 = state.tunnel_registry();
    // Both arcs should point to the same allocation.
    assert!(Arc::ptr_eq(&r1, &r2));
}

// ── Tunnel WebSocket endpoint ─────────────────────────────────────────────────

#[tokio::test]
async fn tunnel_connect_returns_401_without_auth() {
    let (state, _dns) = test_state();
    let app = test_app(state);
    let req = Request::builder()
        .method("GET")
        .uri("/v1/installs/some-id/tunnel")
        .header("Upgrade", "websocket")
        .header("Connection", "Upgrade")
        .header("Sec-WebSocket-Version", "13")
        .header("Sec-WebSocket-Key", "dGhlIHNhbXBsZSBub25jZQ==")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn tunnel_connect_returns_403_when_install_id_does_not_match() {
    use tokio::net::TcpListener as StdTcpListener;
    use tokio_tungstenite::{connect_async, tungstenite::client::IntoClientRequest};

    let (state, _dns) = test_state();
    let (install, raw_token) = insert_test_install(&state, "test-node").await;
    let signing_key = test_signing_key();

    // Spin up a real server so WebSocketUpgrade extraction succeeds.
    let listener = StdTcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let router = wardnet_bridge::api::router(state)
        .into_make_service_with_connect_info::<std::net::SocketAddr>();
    tokio::spawn(axum::serve(listener, router).into_future());

    // Sign for a different install id — auth passes (valid token) but handler
    // rejects because the install id in the path doesn't match the token.
    let other_id = Uuid::new_v4().to_string();
    let path = format!("/v1/installs/{other_id}/tunnel");
    let dummy_req = signed_request("GET", &path, b"", &raw_token, &signing_key);
    let ts = dummy_req
        .headers()
        .get("X-Wardnet-Timestamp")
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    let sig = dummy_req
        .headers()
        .get("X-Wardnet-Signature")
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();

    let url = format!("ws://127.0.0.1:{port}{path}");
    let mut req = url.into_client_request().unwrap();
    req.headers_mut().insert(
        "Authorization",
        format!("Bearer {raw_token}").parse().unwrap(),
    );
    req.headers_mut()
        .insert("X-Wardnet-Timestamp", ts.parse().unwrap());
    req.headers_mut()
        .insert("X-Wardnet-Signature", sig.parse().unwrap());

    // Should be rejected with 403 — handler checks install.id != path id.
    let result = connect_async(req).await;
    match result {
        Err(tokio_tungstenite::tungstenite::Error::Http(resp)) => {
            assert_eq!(resp.status(), 403);
        }
        other => panic!("expected HTTP 403 error, got: {other:?}"),
    }
    drop(install);
}

#[tokio::test]
async fn tunnel_connect_establishes_websocket_and_handler_runs() {
    use tokio::net::TcpListener as StdTcpListener;
    use tokio_tungstenite::{connect_async, tungstenite::client::IntoClientRequest};

    let (state, _dns) = test_state();
    let (install, raw_token) = insert_test_install(&state, "test-node").await;
    let signing_key = test_signing_key();

    // Spin up a real server (needed for actual WS upgrade handshake).
    let listener = StdTcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let router = wardnet_bridge::api::router(state)
        .into_make_service_with_connect_info::<std::net::SocketAddr>();
    tokio::spawn(axum::serve(listener, router).into_future());

    // Build the signed request headers.
    let path = format!("/v1/installs/{}/tunnel", install.id);
    let dummy_req = signed_request("GET", &path, b"", &raw_token, &signing_key);
    let ts = dummy_req
        .headers()
        .get("X-Wardnet-Timestamp")
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    let sig = dummy_req
        .headers()
        .get("X-Wardnet-Signature")
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();

    let url = format!("ws://127.0.0.1:{port}{path}");
    let mut req = url.into_client_request().unwrap();
    req.headers_mut().insert(
        "Authorization",
        format!("Bearer {raw_token}").parse().unwrap(),
    );
    req.headers_mut()
        .insert("X-Wardnet-Timestamp", ts.parse().unwrap());
    req.headers_mut()
        .insert("X-Wardnet-Signature", sig.parse().unwrap());

    // Connect — should get 101 and a live WebSocket.
    let (mut ws, response) = connect_async(req).await.expect("WS connect failed");
    assert_eq!(response.status(), 101);

    // Send a custom PING frame (conn_id=0) and the handler echoes PONG.
    ws.send(WsMessage::Binary(vec![0x05u8, 0, 0, 0, 0].into()))
        .await
        .unwrap();

    // Receive the PONG (first message back from handler).
    if let Some(Ok(WsMessage::Binary(bytes))) = ws.next().await {
        assert_eq!(bytes[0], 0x06); // FRAME_PONG
    }

    ws.close(None).await.ok();
}

/// Exercises `handler::run()`'s `forward_rx` arm (inbound TCP from SNI demuxer)
/// and `tcp_out_rx` arms (DATA and EOF from the active TCP connection).
#[tokio::test]
async fn tunnel_handler_forward_and_tcp_data_flow() {
    use tokio::io::AsyncWriteExt;
    use tokio::net::TcpListener as StdTcpListener;
    use tokio_tungstenite::{connect_async, tungstenite::client::IntoClientRequest};
    use wardnet_bridge::tunnel::ForwardRequest;

    let (state, _dns) = test_state();
    let (install, raw_token) = insert_test_install(&state, "test-node").await;
    let signing_key = test_signing_key();
    let registry = state.tunnel_registry();

    let server_listener = StdTcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = server_listener.local_addr().unwrap().port();
    let router = wardnet_bridge::api::router(state)
        .into_make_service_with_connect_info::<std::net::SocketAddr>();
    tokio::spawn(axum::serve(server_listener, router).into_future());

    // Connect Pi via WebSocket.
    let path = format!("/v1/installs/{}/tunnel", install.id);
    let dummy_req = signed_request("GET", &path, b"", &raw_token, &signing_key);
    let ts = dummy_req
        .headers()
        .get("X-Wardnet-Timestamp")
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    let sig = dummy_req
        .headers()
        .get("X-Wardnet-Signature")
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();

    let url = format!("ws://127.0.0.1:{port}{path}");
    let mut req = url.into_client_request().unwrap();
    req.headers_mut().insert(
        "Authorization",
        format!("Bearer {raw_token}").parse().unwrap(),
    );
    req.headers_mut()
        .insert("X-Wardnet-Timestamp", ts.parse().unwrap());
    req.headers_mut()
        .insert("X-Wardnet-Signature", sig.parse().unwrap());

    let (mut ws, _) = connect_async(req).await.expect("WS connect failed");

    // Wait for handler::run() to call registry.register().
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // Set up a "Pi-local server" that the Pi will connect to when it gets READY.
    let pi_local = StdTcpListener::bind("127.0.0.1:0").await.unwrap();
    let pi_local_addr = pi_local.local_addr().unwrap();

    // Forward a fake inbound TCP stream to the Pi — exercises the forward_rx arm.
    let inbound = tokio::net::TcpStream::connect(pi_local_addr).await.unwrap();
    let _ = registry.forward(
        &install.name,
        ForwardRequest {
            stream: inbound,
            dest_port: 443,
        },
    );

    // Pi receives CONNECT frame.
    let conn_id = if let Some(Ok(WsMessage::Binary(bytes))) = ws.next().await {
        assert_eq!(bytes[0], 0x01, "expected FRAME_CONNECT");
        u32::from_be_bytes([bytes[1], bytes[2], bytes[3], bytes[4]])
    } else {
        panic!("expected CONNECT frame");
    };

    // Pi sends READY — exercises handle_pi_frame READY branch and spawns tcp tasks.
    let mut ready = vec![0x02u8, 0, 0, 0, 0];
    ready[1..5].copy_from_slice(&conn_id.to_be_bytes());
    ws.send(WsMessage::Binary(ready.into())).await.unwrap();

    // Accept the Pi-side TCP connection and write some data + close.
    let (mut pi_conn, _) = pi_local.accept().await.unwrap();
    pi_conn.write_all(b"hello").await.unwrap();
    drop(pi_conn); // EOF → tcp_reader sends empty Bytes → bridge sends CLOSE

    // Pi receives DATA frame then CLOSE frame — exercises the tcp_out_rx arms.
    let mut saw_data = false;
    let mut saw_close = false;
    while let Ok(Some(Ok(WsMessage::Binary(bytes)))) =
        tokio::time::timeout(std::time::Duration::from_secs(2), ws.next()).await
    {
        match bytes[0] {
            0x03 => saw_data = true,
            0x04 => {
                saw_close = true;
                break;
            }
            _ => {}
        }
    }
    assert!(saw_data, "expected DATA frame from bridge");
    assert!(saw_close, "expected CLOSE frame from bridge");

    ws.close(None).await.ok();
}
