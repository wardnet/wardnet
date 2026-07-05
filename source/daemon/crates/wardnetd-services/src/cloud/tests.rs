//! Unit tests for the cloud clients, against `wiremock` tenants/ddns servers.
//!
//! The `PoP` test pins the canonical payload format (the cloud is a separate
//! workspace; this guards against drift) by reconstructing and cryptographically
//! verifying a signature. The client tests assert the exact routes, bodies, and
//! the `403`→`EntitlementLost` and JWT-caching behaviours.

use std::sync::Arc;

use base64::Engine as _;
use ed25519_dalek::{Signer, SigningKey, Verifier};
use serde_json::json;
use sha2::{Digest, Sha256};
use wiremock::matchers::{body_json, header_exists, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::identity::DaemonIdentity;
use super::{CloudError, DdnsClient, TenantsClient, pop};

/// Deterministic identity bound to a wiremock tenants server.
fn identity_for(server: &MockServer) -> Arc<DaemonIdentity> {
    let tenants = Arc::new(TenantsClient::new(reqwest::Client::new(), server.uri()));
    DaemonIdentity::from_seed(
        [7u8; 32],
        tenants,
        crate::entitlement::Entitlement::shared(),
    )
}

/// A JWT-shaped string whose payload decodes to `{"exp": exp}`.
fn fake_jwt(exp: i64) -> String {
    let payload =
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(format!("{{\"exp\":{exp}}}"));
    format!("header.{payload}.sig")
}

#[test]
fn pop_canonical_payload_is_stable_and_verifies() {
    let key = SigningKey::from_bytes(&[7u8; 32]);
    let timestamp = 1_700_000_000i64;
    let body = br#"{"ip":"203.0.113.5"}"#;

    // The signed path is the prefix-free gateway-facing path (`/v1/...`, cloud
    // ADR-0015) — the daemon signs exactly what it dials.
    let payload = pop::canonical_payload("PUT", "/v1/ip", timestamp, body);
    let expected = format!(
        "PUT\n/v1/ip\n{timestamp}\n{}",
        hex::encode(Sha256::digest(body))
    );
    assert_eq!(payload, expected, "canonical payload format must not drift");

    let signature_b64 = pop::sign(&key, "PUT", "/v1/ip", timestamp, body);
    let signature_bytes = base64::engine::general_purpose::STANDARD
        .decode(signature_b64)
        .expect("standard base64");
    let signature = ed25519_dalek::Signature::from_slice(&signature_bytes).unwrap();
    key.verifying_key()
        .verify(payload.as_bytes(), &signature)
        .expect("signature verifies the canonical payload");

    // Cross-check the signer matches a hand-rolled signature over the same bytes.
    assert_eq!(signature, key.sign(expected.as_bytes()));

    // The query string participates in the signed path: the cloud verifies the
    // full request path including `?…`, so a payload that drops the query must
    // not equal one that carries it.
    let with_query =
        pop::canonical_payload("GET", "/v1/availability?slug=alice", timestamp, b"");
    let expected_with_query = format!(
        "GET\n/v1/availability?slug=alice\n{timestamp}\n{}",
        hex::encode(Sha256::digest(b""))
    );
    assert_eq!(with_query, expected_with_query);
    assert_ne!(
        with_query,
        pop::canonical_payload("GET", "/v1/availability", timestamp, b""),
        "dropping the query string must change the signed payload"
    );
}

#[tokio::test]
async fn request_enrollment_code_posts_email_and_purpose() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/verification-codes"))
        .and(body_json(
            json!({ "email": "a@b.com", "purpose": "enrollment" }),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "code": null })))
        .expect(1)
        .mount(&server)
        .await;

    let tenants = TenantsClient::new(reqwest::Client::new(), server.uri());
    tenants.request_enrollment_code("a@b.com").await.unwrap();
}

#[tokio::test]
async fn enroll_returns_tenant_id() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/enroll"))
        .and(body_json(json!({ "code": "ABC123", "public_key": "pk" })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "tenant_id": "t-1" })))
        .mount(&server)
        .await;

    let tenants = TenantsClient::new(reqwest::Client::new(), server.uri());
    assert_eq!(tenants.enroll("ABC123", "pk").await.unwrap(), "t-1");
}

#[tokio::test]
async fn mint_token_403_flags_entitlement_lost() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/token"))
        .and(header_exists(pop::SIGNATURE_HEADER))
        .respond_with(ResponseTemplate::new(403).set_body_string("subscription is not active"))
        .mount(&server)
        .await;

    let identity = identity_for(&server);
    let err = identity.token().await.unwrap_err();
    assert!(matches!(err, CloudError::EntitlementLost));
    assert!(
        !identity.is_entitled(),
        "403 mint flips the entitlement flag"
    );
}

#[tokio::test]
async fn token_is_cached_and_minted_once() {
    let server = MockServer::start().await;
    let exp = chrono::Utc::now().timestamp() + 3600;
    Mock::given(method("POST"))
        .and(path("/v1/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "token": fake_jwt(exp) })))
        .expect(1) // second call must hit the cache, not the server
        .mount(&server)
        .await;

    let identity = identity_for(&server);
    let first = identity.token().await.unwrap();
    let second = identity.token().await.unwrap();
    assert_eq!(first, second);
    assert!(identity.is_entitled());
}

#[tokio::test]
async fn availability_sends_jwt_and_pop() {
    let server = MockServer::start().await;
    let exp = chrono::Utc::now().timestamp() + 3600;
    Mock::given(method("POST"))
        .and(path("/v1/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "token": fake_jwt(exp) })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/availability"))
        .and(query_param("slug", "alice"))
        .and(header_exists("authorization"))
        .and(header_exists(pop::SIGNATURE_HEADER))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "available": true })))
        .mount(&server)
        .await;

    let identity = identity_for(&server);
    assert!(
        identity_tenants(&server)
            .availability(&identity, "alice")
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn register_network_maps_view() {
    let server = MockServer::start().await;
    let exp = chrono::Utc::now().timestamp() + 3600;
    Mock::given(method("POST"))
        .and(path("/v1/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "token": fake_jwt(exp) })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/networks"))
        .and(body_json(json!({ "slug": "alice", "region": "use1" })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "n-1",
            "tenant_id": "t-1",
            "slug": "alice",
            "display_name": "alice",
            "region": "use1",
            "provisioning_state": "provisioning",
            "created_at": "2026-06-29T00:00:00Z",
            "updated_at": "2026-06-29T00:00:00Z"
        })))
        .mount(&server)
        .await;

    let identity = identity_for(&server);
    let reg = identity_tenants(&server)
        .register_network(&identity, "alice", None, "use1")
        .await
        .unwrap();
    assert_eq!(reg.network_id, "n-1");
    assert_eq!(reg.slug, "alice");
    assert_eq!(reg.provisioning_state, "provisioning");
}

#[tokio::test]
async fn ddns_report_ip_and_clear_acme() {
    let server = MockServer::start().await;
    let exp = chrono::Utc::now().timestamp() + 3600;
    Mock::given(method("POST"))
        .and(path("/v1/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "token": fake_jwt(exp) })))
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/v1/ip"))
        .and(body_json(json!({ "ip": "203.0.113.7" })))
        .and(header_exists(pop::SIGNATURE_HEADER))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path("/v1/acme-challenge"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;

    let identity = identity_for(&server);
    let ddns = DdnsClient::new(reqwest::Client::new(), server.uri());
    ddns.report_ip(&identity, "203.0.113.7".parse().unwrap())
        .await
        .unwrap();
    ddns.clear_acme_challenge(&identity).await.unwrap();
}

/// A tenants client sharing the wiremock server URI (the identity already holds
/// one for minting; tests that also call tenants endpoints want a handle).
fn identity_tenants(server: &MockServer) -> TenantsClient {
    TenantsClient::new(reqwest::Client::new(), server.uri())
}
