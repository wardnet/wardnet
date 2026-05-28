//! Unit tests for [`CloudflareDnsProvider`].
//!
//! Uses [`wiremock`] to spin up a local HTTP server so no real Cloudflare
//! calls are made. The test constructor `new_for_test` overrides `base_url`
//! to point at the mock server and sets `initial_backoff = Duration::ZERO`
//! so retry paths run without sleeping.

use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::provider::CloudflareDnsProvider;
use crate::dns_provider::DnsProvider;

// ── Helpers ───────────────────────────────────────────────────────────────────

const ZONE: &str = "test-zone-id";
const TOKEN: &str = "test-api-token";

/// Standard success response body for a create/update operation.
fn cf_ok(record_id: &str) -> serde_json::Value {
    json!({
        "success": true,
        "errors": [],
        "result": { "id": record_id }
    })
}

/// Build a provider that points at the given mock server.
fn provider(server: &MockServer) -> CloudflareDnsProvider {
    CloudflareDnsProvider::new_for_test(TOKEN, ZONE, &server.uri()).unwrap()
}

// ── Constructor ───────────────────────────────────────────────────────────────

#[test]
fn new_rejects_token_with_invalid_header_chars() {
    // ASCII control characters are invalid in HTTP header values.
    let result = CloudflareDnsProvider::new("\x00bad", "zone");
    let err = result.expect_err("expected error for invalid token chars");
    let msg = err.to_string();
    assert!(msg.contains("invalid header"), "unexpected error: {msg}");
}

#[test]
fn new_accepts_valid_token() {
    let result = CloudflareDnsProvider::new("valid-token-abc123", "zone-id");
    assert!(result.is_ok());
}

// ── upsert_a_record ───────────────────────────────────────────────────────────

#[tokio::test]
async fn upsert_a_record_create_posts_to_records_endpoint() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path(format!("/zones/{ZONE}/dns_records")))
        .respond_with(ResponseTemplate::new(200).set_body_json(cf_ok("new-a-record")))
        .expect(1)
        .mount(&server)
        .await;

    let record_id = provider(&server)
        .upsert_a_record("myhost.example.com", "1.2.3.4", None)
        .await
        .unwrap();

    assert_eq!(record_id, "new-a-record");
    server.verify().await;
}

#[tokio::test]
async fn upsert_a_record_update_puts_to_record_endpoint() {
    let server = MockServer::start().await;

    Mock::given(method("PUT"))
        .and(path(format!("/zones/{ZONE}/dns_records/existing-id")))
        .respond_with(ResponseTemplate::new(200).set_body_json(cf_ok("existing-id")))
        .expect(1)
        .mount(&server)
        .await;

    let record_id = provider(&server)
        .upsert_a_record("myhost.example.com", "1.2.3.4", Some("existing-id"))
        .await
        .unwrap();

    assert_eq!(record_id, "existing-id");
    server.verify().await;
}

// ── upsert_txt_record ─────────────────────────────────────────────────────────

#[tokio::test]
async fn upsert_txt_record_create_posts_to_records_endpoint() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path(format!("/zones/{ZONE}/dns_records")))
        .respond_with(ResponseTemplate::new(200).set_body_json(cf_ok("new-txt-record")))
        .expect(1)
        .mount(&server)
        .await;

    let record_id = provider(&server)
        .upsert_txt_record("_acme.example.com", "challenge-value", None)
        .await
        .unwrap();

    assert_eq!(record_id, "new-txt-record");
    server.verify().await;
}

#[tokio::test]
async fn upsert_txt_record_update_puts_to_record_endpoint() {
    let server = MockServer::start().await;

    Mock::given(method("PUT"))
        .and(path(format!("/zones/{ZONE}/dns_records/txt-id")))
        .respond_with(ResponseTemplate::new(200).set_body_json(cf_ok("txt-id")))
        .expect(1)
        .mount(&server)
        .await;

    let record_id = provider(&server)
        .upsert_txt_record("_acme.example.com", "new-value", Some("txt-id"))
        .await
        .unwrap();

    assert_eq!(record_id, "txt-id");
    server.verify().await;
}

// ── delete_record ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn delete_record_success_on_200() {
    let server = MockServer::start().await;

    Mock::given(method("DELETE"))
        .and(path(format!("/zones/{ZONE}/dns_records/del-id")))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&server)
        .await;

    provider(&server).delete_record("del-id").await.unwrap();
    server.verify().await;
}

#[tokio::test]
async fn delete_record_returns_ok_on_404_idempotency() {
    let server = MockServer::start().await;

    Mock::given(method("DELETE"))
        .and(path(format!("/zones/{ZONE}/dns_records/gone-id")))
        .respond_with(ResponseTemplate::new(404))
        .expect(1)
        .mount(&server)
        .await;

    // 404 must be treated as a success (idempotent delete).
    provider(&server).delete_record("gone-id").await.unwrap();
    server.verify().await;
}

#[tokio::test]
async fn delete_record_returns_error_on_4xx_other_than_404() {
    let server = MockServer::start().await;

    Mock::given(method("DELETE"))
        .and(path(format!("/zones/{ZONE}/dns_records/no-perm")))
        .respond_with(ResponseTemplate::new(403))
        .expect(1)
        .mount(&server)
        .await;

    let result = provider(&server).delete_record("no-perm").await;
    assert!(result.is_err(), "expected error for 403");
    server.verify().await;
}

// ── parse_cf_response error paths ────────────────────────────────────────────

#[tokio::test]
async fn returns_error_when_response_body_is_not_json() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path(format!("/zones/{ZONE}/dns_records")))
        .respond_with(ResponseTemplate::new(200).set_body_string("not json at all"))
        .mount(&server)
        .await;

    let result = provider(&server)
        .upsert_a_record("host.example.com", "1.2.3.4", None)
        .await;
    assert!(result.is_err());
    let msg = result.unwrap_err().to_string();
    assert!(msg.contains("non-JSON"), "unexpected error: {msg}");
}

#[tokio::test]
async fn returns_cloudflare_error_messages_when_success_is_false() {
    let server = MockServer::start().await;

    let body = json!({
        "success": false,
        "errors": [
            { "message": "Record already exists" },
            { "message": "Quota exceeded" }
        ],
        "result": null
    });
    Mock::given(method("POST"))
        .and(path(format!("/zones/{ZONE}/dns_records")))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(&server)
        .await;

    let result = provider(&server)
        .upsert_a_record("host.example.com", "1.2.3.4", None)
        .await;
    assert!(result.is_err());
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("Record already exists"),
        "unexpected error: {msg}"
    );
    assert!(msg.contains("Quota exceeded"), "unexpected error: {msg}");
}

#[tokio::test]
async fn returns_error_when_success_is_true_but_result_id_is_missing() {
    let server = MockServer::start().await;

    let body = json!({
        "success": true,
        "errors": [],
        "result": null   // success=true but no result object
    });
    Mock::given(method("POST"))
        .and(path(format!("/zones/{ZONE}/dns_records")))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(&server)
        .await;

    let result = provider(&server)
        .upsert_a_record("host.example.com", "1.2.3.4", None)
        .await;
    assert!(result.is_err());
    let msg = result.unwrap_err().to_string();
    assert!(msg.contains("no result.id"), "unexpected error: {msg}");
}

// ── Retry logic ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn retries_on_429_and_succeeds_on_next_attempt() {
    let server = MockServer::start().await;

    // First call: 429. Second call: 200 OK.
    Mock::given(method("POST"))
        .and(path(format!("/zones/{ZONE}/dns_records")))
        .respond_with(ResponseTemplate::new(429))
        .up_to_n_times(1)
        .mount(&server)
        .await;

    Mock::given(method("POST"))
        .and(path(format!("/zones/{ZONE}/dns_records")))
        .respond_with(ResponseTemplate::new(200).set_body_json(cf_ok("rec-after-429")))
        .mount(&server)
        .await;

    let record_id = provider(&server)
        .upsert_a_record("host.example.com", "1.2.3.4", None)
        .await
        .unwrap();

    assert_eq!(record_id, "rec-after-429");
}

#[tokio::test]
async fn retries_on_5xx_and_succeeds_on_next_attempt() {
    let server = MockServer::start().await;

    // First call: 503. Second call: 200 OK.
    Mock::given(method("POST"))
        .and(path(format!("/zones/{ZONE}/dns_records")))
        .respond_with(ResponseTemplate::new(503))
        .up_to_n_times(1)
        .mount(&server)
        .await;

    Mock::given(method("POST"))
        .and(path(format!("/zones/{ZONE}/dns_records")))
        .respond_with(ResponseTemplate::new(200).set_body_json(cf_ok("rec-after-503")))
        .mount(&server)
        .await;

    let record_id = provider(&server)
        .upsert_a_record("host.example.com", "1.2.3.4", None)
        .await
        .unwrap();

    assert_eq!(record_id, "rec-after-503");
}

#[tokio::test]
async fn exhausts_retries_on_persistent_5xx_and_returns_error() {
    let server = MockServer::start().await;

    // All 4 attempts (initial + 3 retries) return 500.
    Mock::given(method("POST"))
        .and(path(format!("/zones/{ZONE}/dns_records")))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;

    let result = provider(&server)
        .upsert_a_record("host.example.com", "1.2.3.4", None)
        .await;

    assert!(result.is_err());
    let msg = result.unwrap_err().to_string();
    assert!(msg.contains("server error"), "unexpected error: {msg}");
}

#[tokio::test]
async fn does_not_retry_on_4xx_client_error() {
    let server = MockServer::start().await;

    // 422 Unprocessable Entity — a client error, must not be retried.
    Mock::given(method("POST"))
        .and(path(format!("/zones/{ZONE}/dns_records")))
        .respond_with(ResponseTemplate::new(422).set_body_json(
            json!({"success": false, "errors": [{"message": "bad input"}], "result": null}),
        ))
        .expect(1) // exactly one call — no retries
        .mount(&server)
        .await;

    let result = provider(&server)
        .upsert_a_record("host.example.com", "1.2.3.4", None)
        .await;

    assert!(result.is_err());
    server.verify().await; // would fail if more than 1 request was made
}
