//! Cloudflare provider tests against a `wiremock` Cloudflare API. Verify the
//! list-then-create/update branch selection, bearer auth, request bodies, and
//! that an unsuccessful envelope surfaces as an error.

use serde_json::json;
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::ddns::cloudflare::CloudflareProvider;
use crate::ddns::provider::DnsProvider;

const ZONE: &str = "zone1";
const DOMAIN: &str = "home.example.com";

fn provider(base_url: &str) -> CloudflareProvider {
    CloudflareProvider::new_with_base_url(
        reqwest::Client::new(),
        "cf-token",
        ZONE,
        DOMAIN,
        base_url,
    )
}

#[tokio::test]
async fn upsert_a_creates_when_absent() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(format!("/zones/{ZONE}/dns_records")))
        .and(query_param("type", "A"))
        .and(query_param("name", DOMAIN))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "success": true, "errors": [], "result": []
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(format!("/zones/{ZONE}/dns_records")))
        .and(header("authorization", "Bearer cf-token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "success": true, "errors": [], "result": {"id": "rec-new"}
        })))
        .mount(&server)
        .await;

    provider(&server.uri())
        .upsert_a("1.2.3.4".parse().unwrap())
        .await
        .unwrap();

    let requests = server.received_requests().await.unwrap();
    let post = requests
        .iter()
        .find(|r| r.method.as_str() == "POST")
        .expect("create POST sent");
    let body: serde_json::Value = serde_json::from_slice(&post.body).unwrap();
    assert_eq!(body["type"], "A");
    assert_eq!(body["name"], DOMAIN);
    assert_eq!(body["content"], "1.2.3.4");
}

#[tokio::test]
async fn upsert_a_updates_when_present() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(format!("/zones/{ZONE}/dns_records")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "success": true, "errors": [], "result": [{"id": "rec-1"}]
        })))
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path(format!("/zones/{ZONE}/dns_records/rec-1")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "success": true, "errors": [], "result": {"id": "rec-1"}
        })))
        .expect(1)
        .mount(&server)
        .await;

    provider(&server.uri())
        .upsert_a("5.6.7.8".parse().unwrap())
        .await
        .unwrap();
    // `.expect(1)` on the PUT mock is verified when the server drops.
}

#[tokio::test]
async fn set_txt_creates_acme_record() {
    let server = MockServer::start().await;
    let acme = format!("_acme-challenge.{DOMAIN}");
    Mock::given(method("GET"))
        .and(path(format!("/zones/{ZONE}/dns_records")))
        .and(query_param("type", "TXT"))
        .and(query_param("name", acme.as_str()))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "success": true, "errors": [], "result": []
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(format!("/zones/{ZONE}/dns_records")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "success": true, "errors": [], "result": {"id": "txt-1"}
        })))
        .mount(&server)
        .await;

    provider(&server.uri())
        .set_txt("challenge-value")
        .await
        .unwrap();

    let requests = server.received_requests().await.unwrap();
    let post = requests
        .iter()
        .find(|r| r.method.as_str() == "POST")
        .expect("create POST sent");
    let body: serde_json::Value = serde_json::from_slice(&post.body).unwrap();
    assert_eq!(body["type"], "TXT");
    assert_eq!(body["name"], acme);
    assert_eq!(body["content"], "challenge-value");
}

#[tokio::test]
async fn delete_txt_is_noop_when_absent() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(format!("/zones/{ZONE}/dns_records")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "success": true, "errors": [], "result": []
        })))
        .mount(&server)
        .await;

    provider(&server.uri()).delete_txt().await.unwrap();

    let requests = server.received_requests().await.unwrap();
    assert!(requests.iter().all(|r| r.method.as_str() != "DELETE"));
}

#[tokio::test]
async fn delete_txt_removes_existing_record() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(format!("/zones/{ZONE}/dns_records")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "success": true, "errors": [], "result": [{"id": "txt-9"}]
        })))
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path(format!("/zones/{ZONE}/dns_records/txt-9")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "success": true, "errors": [], "result": {"id": "txt-9"}
        })))
        .expect(1)
        .mount(&server)
        .await;

    provider(&server.uri()).delete_txt().await.unwrap();
}

#[tokio::test]
async fn unsuccessful_envelope_surfaces_as_error() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(format!("/zones/{ZONE}/dns_records")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "success": false,
            "errors": [{"message": "Invalid zone"}],
            "result": null
        })))
        .mount(&server)
        .await;

    let error = provider(&server.uri())
        .upsert_a("1.2.3.4".parse().unwrap())
        .await
        .unwrap_err();
    assert!(error.to_string().contains("Invalid zone"), "got: {error}");
}
