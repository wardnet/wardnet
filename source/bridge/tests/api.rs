//! Full-stack integration tests for the bridge HTTP API.
//!
//! These tests build the complete Axum router with an in-memory `SQLite` database
//! and a `MockDnsProvider`, then drive requests through
//! [`tower::ServiceExt::oneshot`].
//!
//! # Test conventions
//!
//! - Every test creates its own isolated in-memory database via `test_state()`.
//! - Challenges are inserted directly with `difficulty = 0` so no real `PoW`
//!   computation is needed. The register handler calls `verify_pow` with
//!   whatever difficulty is in the challenge row — 0 bits means any `proof`
//!   value passes.
//! - Ed25519 signing uses a deterministic test key derived from `[1u8; 32]`.
//! - The loopback peer (`127.0.0.1`) is injected via `MockConnectInfo` so
//!   handlers that call `client_ip()` see a non-forwarded address.

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::extract::connect_info::MockConnectInfo;
use axum::http::{Request, StatusCode};
use base64::Engine as _;
use chrono::Utc;
use ed25519_dalek::{Signer, SigningKey};
use http_body_util::BodyExt;
use sha2::{Digest, Sha256};
use tower::ServiceExt as _;
use uuid::Uuid;

use wardnet_bridge::config::Config;
use wardnet_bridge::db::DbPools;
use wardnet_bridge::dns_provider::DnsProvider;
use wardnet_bridge::repository::{
    ChallengeRepository, Install, InstallRepository, RegistrationChallenge,
    SqliteChallengeRepository, SqliteInstallRepository,
};
use wardnet_bridge::state::AppState;

// ── Mock DNS provider ────────────────────────────────────────────────────────

/// Records which `DnsProvider` method was called — fields are tracked only for
/// debug output; tests count calls rather than inspecting field values.
#[derive(Debug)]
enum DnsCall {
    UpsertA,
    UpsertTxt,
    DeleteRecord,
}

struct MockDnsProvider {
    calls: Mutex<Vec<DnsCall>>,
    /// If `Some`, every operation returns this error message.
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
        database_url: ":memory:".to_string(),
        cloudflare_api_token: "test-cf-token".to_string(),
        cloudflare_zone_id: "test-cf-zone".to_string(),
        region: "test".to_string(),
        subdomain_parent: "test.wardnet.local".to_string(),
    }
}

async fn make_pools() -> DbPools {
    wardnet_bridge::db::init(":memory:").await.unwrap()
}

/// Build an `AppState` backed by an in-memory database and the given DNS mock.
async fn test_state_with_dns(dns: Arc<MockDnsProvider>) -> AppState {
    let pools = make_pools().await;
    let installs = Arc::new(SqliteInstallRepository::new(pools.write.clone()));
    let challenges = Arc::new(SqliteChallengeRepository::new(pools.write.clone()));
    AppState::new(
        test_config(),
        pools,
        installs as Arc<dyn InstallRepository>,
        challenges as Arc<dyn ChallengeRepository>,
        dns as Arc<dyn DnsProvider>,
    )
}

/// Convenience wrapper: fresh state with a non-failing mock DNS provider.
async fn test_state() -> (AppState, Arc<MockDnsProvider>) {
    let dns = Arc::new(MockDnsProvider::new());
    let state = test_state_with_dns(dns.clone()).await;
    (state, dns)
}

/// Build the Axum router under test with a fixed loopback peer address so
/// every request appears to originate from `127.0.0.1:12345`.
fn test_app(state: AppState) -> axum::Router {
    wardnet_bridge::api::router(state)
        .layer(MockConnectInfo(SocketAddr::from(([127, 0, 0, 1], 12345))))
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

/// Insert a test install row directly into the DB and return the install + raw token.
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
///
/// Signs `"METHOD\npath\ntimestamp\nhex-sha256(body)"` with the test Ed25519 key.
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

/// Like `signed_request` but with a caller-supplied timestamp (for replay / staleness tests).
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
    let (state, _dns) = test_state().await;
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
    let (state, _dns) = test_state().await;
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
    let (state, _dns) = test_state().await;

    // Pre-insert 20 challenges from 127.0.0.1 within the last hour.
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
    let (state, _dns) = test_state().await;
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
    let (state, _dns) = test_state().await;
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
    let (state, _dns) = test_state().await;
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
    let (state, _dns) = test_state().await;
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

/// Helper: build a register request body.
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
    let (state, _dns) = test_state().await;
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
    let (state, _dns) = test_state().await;
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
    let (state, _dns) = test_state().await;
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
    let (state, _dns) = test_state().await;
    let challenge = insert_easy_challenge(&state, "127.0.0.1").await;

    // Non-base64 string
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

    // Valid base64 but wrong length (31 bytes)
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
    let (state, _dns) = test_state().await;
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
    let (state, _dns) = test_state().await;
    let (_, pub_key_b64) = test_pub_key();

    let now = Utc::now();
    let expired = RegistrationChallenge {
        id: Uuid::new_v4().to_string(),
        nonce: hex::encode([1u8; 32]),
        difficulty: 0,
        remote_ip: "127.0.0.1".to_string(),
        created_at: now - chrono::Duration::minutes(10),
        expires_at: now - chrono::Duration::minutes(1), // already expired
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
    let (state, _dns) = test_state().await;
    let (_, pub_key_b64) = test_pub_key();

    // Challenge issued to a different IP than the test client (127.0.0.1)
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
    let (state, _dns) = test_state().await;
    let (_, pub_key_b64) = test_pub_key();

    // Insert challenge with high difficulty — proof=0 won't pass 24 leading-zero bits
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
    let (state, _dns) = test_state().await;
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
    let (state, _dns) = test_state().await;
    let challenge = insert_easy_challenge(&state, "127.0.0.1").await;
    let (_, pub_key_b64) = test_pub_key();

    // Pre-consume the challenge
    state
        .challenges()
        .consume(&challenge.id, &Utc::now().to_rfc3339())
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
    let (state, _dns) = test_state().await;

    // Log 3 registrations from 127.0.0.1 within the last 24 h
    let now = Utc::now().to_rfc3339();
    for _ in 0..3 {
        state
            .installs()
            .log_registration("127.0.0.1", &now)
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
    let (state, _dns) = test_state().await;
    let (install, raw_token) = insert_test_install(&state, "test-node").await;

    let app = test_app(state);
    // Body is 1 MiB + 1 byte — just over the limit
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
    // Unauthenticated endpoints (health, challenge, register, names) must work
    // without an Authorization header even when accessed on paths not under /v1/installs/.
    let (state, _dns) = test_state().await;
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
    let (state, _dns) = test_state().await;
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
            .contains("Authorization header"),
    );
}

#[tokio::test]
async fn auth_returns_401_for_unknown_bearer_token() {
    let (state, _dns) = test_state().await;
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
    let (state, _dns) = test_state().await;
    let (install, raw_token) = insert_test_install(&state, "test-node").await;

    let app = test_app(state);
    let req = Request::builder()
        .method("PUT")
        .uri(format!("/v1/installs/{}/ip", install.id))
        .header("Authorization", format!("Bearer {raw_token}"))
        // No X-Wardnet-Timestamp header
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
    let (state, _dns) = test_state().await;
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
    let (state, _dns) = test_state().await;
    let (install, raw_token) = insert_test_install(&state, "test-node").await;

    let app = test_app(state);
    let past_ts = Utc::now().timestamp() - 120; // 120 s in the past, outside ±60 s window
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
    let (state, _dns) = test_state().await;
    let (install, raw_token) = insert_test_install(&state, "test-node").await;

    let app = test_app(state);
    // Provide a syntactically valid but cryptographically wrong signature
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
    let (state, _dns) = test_state().await;
    let (install, raw_token) = insert_test_install(&state, "test-node").await;

    let app = test_app(state);
    let req = Request::builder()
        .method("PUT")
        .uri(format!("/v1/installs/{}/ip", install.id))
        .header("Authorization", format!("Bearer {raw_token}"))
        .header("X-Wardnet-Timestamp", Utc::now().timestamp().to_string())
        .header("X-Wardnet-Signature", "not!valid!base64!!!") // malformed base64
        .header("Content-Type", "application/json")
        .body(Body::from(r#"{"ip":"203.0.113.1"}"#))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn auth_returns_401_on_replayed_request() {
    let (state, _dns) = test_state().await;
    let (install, raw_token) = insert_test_install(&state, "test-node").await;
    let signing_key = test_signing_key();

    // Fix a single timestamp so both requests share the same replay key.
    let timestamp = Utc::now().timestamp();
    let body = br#"{"ip":"8.8.8.8"}"#;
    let path = format!("/v1/installs/{}/ip", install.id);

    let app = test_app(state);

    // First request — must succeed.
    let req1 = signed_request_at("PUT", &path, body, &raw_token, &signing_key, timestamp);
    let resp1 = app.clone().oneshot(req1).await.unwrap();
    assert_eq!(
        resp1.status(),
        StatusCode::NO_CONTENT,
        "first signed request should succeed"
    );

    // Second request with identical timestamp, body, and signature — must be rejected.
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
    // No Authorization header at all on a /v1/installs/* path.
    // The auth layer skips auth (no header), the handler tries AuthenticatedInstall,
    // which returns 401 since no Install extension was stamped.
    let (state, _dns) = test_state().await;
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
    let (state, dns) = test_state().await;
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
    let (state, _dns) = test_state().await;
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
    let (state, _dns) = test_state().await;
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
    let (state, _dns) = test_state().await;
    let (_, raw_token) = insert_test_install(&state, "test-node").await;
    let signing_key = test_signing_key();

    // A different install ID in the path — token belongs to a different install
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
    let state = test_state_with_dns(dns).await;
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
    let (state, dns) = test_state().await;
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
    let (state, _dns) = test_state().await;
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
    let state = test_state_with_dns(dns).await;
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
    let (state, dns) = test_state().await;
    let (install, raw_token) = insert_test_install(&state, "test-node").await;
    let signing_key = test_signing_key();

    // Pre-populate the ACME record ID
    state
        .installs()
        .update_acme_record(
            &install.id,
            Some("cf-txt-existing"),
            &Utc::now().to_rfc3339(),
        )
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
    let (state, dns) = test_state().await;
    let (install, raw_token) = insert_test_install(&state, "test-node").await;
    let signing_key = test_signing_key();

    // No ACME record set — should still succeed without touching DNS
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
    let (state, _dns) = test_state().await;
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
    let state = test_state_with_dns(dns).await;
    let (install, raw_token) = insert_test_install(&state, "test-node").await;
    let signing_key = test_signing_key();

    // Seed an ACME record so the handler tries to delete it
    state
        .installs()
        .update_acme_record(&install.id, Some("cf-txt-exists"), &Utc::now().to_rfc3339())
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
    let (state, dns) = test_state().await;
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
    let (state, dns) = test_state().await;
    let (install, raw_token) = insert_test_install(&state, "test-node").await;
    let signing_key = test_signing_key();

    // Pre-set an A record and an ACME TXT record
    let now = Utc::now().to_rfc3339();
    state
        .installs()
        .update_ip(&install.id, "8.8.8.8", "cf-a-id", &now)
        .await
        .unwrap();
    state
        .installs()
        .update_acme_record(&install.id, Some("cf-txt-id"), &now)
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
    let (state, dns) = test_state().await;
    let (install, raw_token) = insert_test_install(&state, "test-node").await;
    let signing_key = test_signing_key();

    let now = Utc::now().to_rfc3339();
    state
        .installs()
        .update_ip(&install.id, "8.8.8.8", "cf-a-id", &now)
        .await
        .unwrap();
    // No ACME record

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
    let (state, _dns) = test_state().await;
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
    let state = test_state_with_dns(dns).await;
    let (install, raw_token) = insert_test_install(&state, "test-node").await;
    let signing_key = test_signing_key();

    // Pre-set an A record so the handler tries (and fails) to delete it
    state
        .installs()
        .update_ip(&install.id, "8.8.8.8", "cf-a-id", &Utc::now().to_rfc3339())
        .await
        .unwrap();

    let body = b"";
    let path = format!("/v1/installs/{}", install.id);
    let req = signed_request("DELETE", &path, body, &raw_token, &signing_key);

    let app = test_app(state);
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

// ── DB initialisation ────────────────────────────────────────────────────────

#[tokio::test]
async fn db_init_file_backed_creates_schema() {
    use std::fs;

    let dir = std::env::temp_dir().join(format!("wardnet-bridge-test-{}", Uuid::new_v4().simple()));
    let db_path = dir.join("test.db");
    let path_str = db_path.to_str().unwrap().to_string();

    let pools = wardnet_bridge::db::init(&path_str).await.unwrap();
    // Verify that the schema was applied by querying a known table
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM installs")
        .fetch_one(&pools.write)
        .await
        .unwrap();
    assert_eq!(count, 0);

    drop(pools);
    let _ = fs::remove_dir_all(&dir);
}

// ── State accessors ──────────────────────────────────────────────────────────
// These tests exercise the accessor methods that may not be reachable through
// HTTP tests (e.g. config fields that don't appear in response bodies).

#[tokio::test]
async fn config_accessors_return_expected_values() {
    let (state, _dns) = test_state().await;
    let cfg = state.config();
    assert_eq!(cfg.region, "test");
    assert_eq!(cfg.subdomain_parent, "test.wardnet.local");
    assert_eq!(cfg.install_fqdn("mynode"), "mynode.test.wardnet.local");
    assert_eq!(
        cfg.acme_fqdn("mynode"),
        "_acme-challenge.mynode.test.wardnet.local"
    );
}
