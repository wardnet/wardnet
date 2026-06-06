//! Service-layer tests. Auth gating and the unconfigured short-circuit use
//! mock repositories; registration and IP-refresh run against a `wiremock`
//! bridge so the persistence + change-detection logic is exercised end to end.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;

use async_trait::async_trait;
use uuid::Uuid;
use wardnet_common::auth::AuthContext;
use wardnetd_data::repository::SystemConfigRepository;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::auth_context;
use crate::ddns::{
    DdnsService, DdnsServiceImpl, DdnsSettings, KEY_BRIDGE_BASE_URL, KEY_CF_ZONE_ID, KEY_DOMAIN,
    KEY_INSTALL_ID, KEY_LAST_IP, KEY_PROVIDER, KEY_REGION, KEY_SUBDOMAIN, PROVIDER_BRIDGE,
    PROVIDER_CLOUDFLARE, SECRET_BRIDGE_BEARER, SECRET_BRIDGE_SIGNING_KEY, SECRET_CF_TOKEN,
};
use crate::error::AppError;
use crate::secret_store::SecretStore;

// ── Mock repositories ──────────────────────────────────────────────────────────

#[derive(Default)]
struct MockSystemConfig {
    store: Mutex<HashMap<String, String>>,
}

#[async_trait]
impl SystemConfigRepository for MockSystemConfig {
    async fn get(&self, key: &str) -> anyhow::Result<Option<String>> {
        Ok(self.store.lock().unwrap().get(key).cloned())
    }
    async fn set(&self, key: &str, value: &str) -> anyhow::Result<()> {
        self.store
            .lock()
            .unwrap()
            .insert(key.to_owned(), value.to_owned());
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

#[derive(Default)]
struct MockSecretStore {
    store: Mutex<HashMap<String, Vec<u8>>>,
}

#[async_trait]
impl SecretStore for MockSecretStore {
    async fn put(&self, path: &str, value: &[u8]) -> anyhow::Result<()> {
        self.store
            .lock()
            .unwrap()
            .insert(path.to_owned(), value.to_vec());
        Ok(())
    }
    async fn get(&self, path: &str) -> anyhow::Result<Option<Vec<u8>>> {
        Ok(self.store.lock().unwrap().get(path).cloned())
    }
    async fn delete(&self, path: &str) -> anyhow::Result<()> {
        self.store.lock().unwrap().remove(path);
        Ok(())
    }
    async fn list(&self, prefix: &str) -> anyhow::Result<Vec<String>> {
        Ok(self
            .store
            .lock()
            .unwrap()
            .keys()
            .filter(|k| k.starts_with(prefix))
            .cloned()
            .collect())
    }
}

// ── Helpers ────────────────────────────────────────────────────────────────────

fn admin_ctx() -> AuthContext {
    AuthContext::Admin {
        admin_id: Uuid::nil(),
    }
}

fn settings(bridge_url: Option<String>, echo: Vec<String>) -> DdnsSettings {
    DdnsSettings {
        region_catalog: bridge_url
            .map(|url| vec![("use1".to_owned(), url)])
            .unwrap_or_default(),
        echo_endpoints: echo,
        cf_base_url: String::new(),
    }
}

/// A bridge with a healthy `/v1/health` plus challenge + register endpoints.
async fn registerable_bridge() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/health"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/register/challenge"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "challenge_id": "chal-1", "nonce": "n0nce", "difficulty": 4,
            "expires_at": "2030-01-01T00:00:00Z",
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/register"))
        .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
            "id": "install-9", "bearer_token": "tok-9",
            "subdomain": "happy-einstein.my.us.wardnet.network", "region": "us",
        })))
        .mount(&server)
        .await;
    server
}

async fn echo_server(ip: &str) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/"))
        .respond_with(ResponseTemplate::new(200).set_body_string(ip))
        .mount(&server)
        .await;
    server
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn refresh_and_register_require_admin() {
    let svc = DdnsServiceImpl::new(
        Arc::new(MockSystemConfig::default()),
        Arc::new(MockSecretStore::default()),
    );
    // No auth context set → both reject before any network call.
    assert!(matches!(
        svc.refresh_public_ip().await.unwrap_err(),
        AppError::Forbidden(_)
    ));
    assert!(matches!(
        svc.register_with_bridge("x".to_owned()).await.unwrap_err(),
        AppError::Forbidden(_)
    ));
}

#[tokio::test]
async fn refresh_is_noop_when_unconfigured() {
    let svc = DdnsServiceImpl::new(
        Arc::new(MockSystemConfig::default()),
        Arc::new(MockSecretStore::default()),
    );
    let result = auth_context::with_context(admin_ctx(), svc.refresh_public_ip())
        .await
        .unwrap();
    assert_eq!(result, None);
}

#[tokio::test]
async fn register_with_bridge_persists_identity() {
    let server = registerable_bridge().await;
    let config = Arc::new(MockSystemConfig::default());
    let secrets = Arc::new(MockSecretStore::default());
    let svc = DdnsServiceImpl::with_settings(
        config.clone(),
        secrets.clone(),
        settings(Some(server.uri()), vec![]),
    );

    let registration = auth_context::with_context(
        admin_ctx(),
        svc.register_with_bridge("happy-einstein".to_owned()),
    )
    .await
    .unwrap();
    assert_eq!(
        registration.subdomain,
        "happy-einstein.my.us.wardnet.network"
    );
    assert_eq!(registration.region, "us");

    assert_eq!(
        config.get(KEY_PROVIDER).await.unwrap().as_deref(),
        Some(PROVIDER_BRIDGE)
    );
    assert_eq!(
        config.get(KEY_INSTALL_ID).await.unwrap().as_deref(),
        Some("install-9")
    );
    assert_eq!(
        config.get(KEY_SUBDOMAIN).await.unwrap().as_deref(),
        Some("happy-einstein.my.us.wardnet.network")
    );
    assert_eq!(config.get(KEY_REGION).await.unwrap().as_deref(), Some("us"));
    assert_eq!(
        config.get(KEY_BRIDGE_BASE_URL).await.unwrap(),
        Some(server.uri())
    );

    assert_eq!(
        secrets
            .get(SECRET_BRIDGE_SIGNING_KEY)
            .await
            .unwrap()
            .unwrap()
            .len(),
        32
    );
    assert_eq!(
        secrets.get(SECRET_BRIDGE_BEARER).await.unwrap().unwrap(),
        b"tok-9"
    );
}

#[tokio::test]
async fn register_rejects_invalid_name_before_any_network_call() {
    // A name with a path separator must be rejected locally (BadRequest) so it
    // can never be interpolated into the bridge URL path. No bridge is wired, so
    // a network attempt would itself error differently.
    let svc = DdnsServiceImpl::new(
        Arc::new(MockSystemConfig::default()),
        Arc::new(MockSecretStore::default()),
    );
    let err = auth_context::with_context(
        admin_ctx(),
        svc.register_with_bridge("../installs/x/ip".to_owned()),
    )
    .await
    .unwrap_err();
    assert!(matches!(err, AppError::BadRequest(_)));
}

#[tokio::test]
async fn check_name_available_returns_false_for_invalid_name() {
    // Invalid names short-circuit to `false` with no network call (no bridge
    // wired here), mirroring the bridge's own availability semantics.
    let svc = DdnsServiceImpl::new(
        Arc::new(MockSystemConfig::default()),
        Arc::new(MockSecretStore::default()),
    );
    let available =
        auth_context::with_context(admin_ctx(), svc.check_name_available("bad/name".to_owned()))
            .await
            .unwrap();
    assert!(!available);
}

/// Pre-seed a registered bridge identity into the mock config + secrets.
async fn seed_bridge_identity(
    config: &MockSystemConfig,
    secrets: &MockSecretStore,
    bridge_url: &str,
    last_ip: Option<&str>,
) {
    config.set(KEY_PROVIDER, PROVIDER_BRIDGE).await.unwrap();
    config.set(KEY_INSTALL_ID, "inst-1").await.unwrap();
    config.set(KEY_BRIDGE_BASE_URL, bridge_url).await.unwrap();
    if let Some(ip) = last_ip {
        config.set(KEY_LAST_IP, ip).await.unwrap();
    }
    secrets
        .put(SECRET_BRIDGE_SIGNING_KEY, &[3u8; 32])
        .await
        .unwrap();
    secrets.put(SECRET_BRIDGE_BEARER, b"tok").await.unwrap();
}

#[tokio::test]
async fn refresh_skips_when_ip_unchanged() {
    let bridge = MockServer::start().await;
    Mock::given(method("PUT"))
        .and(path("/v1/installs/inst-1/ip"))
        .respond_with(ResponseTemplate::new(204))
        .expect(0) // must NOT be called when the IP is unchanged
        .mount(&bridge)
        .await;
    let echo = echo_server("1.2.3.4").await;

    let config = Arc::new(MockSystemConfig::default());
    let secrets = Arc::new(MockSecretStore::default());
    seed_bridge_identity(&config, &secrets, &bridge.uri(), Some("1.2.3.4")).await;

    let svc = DdnsServiceImpl::with_settings(
        config.clone(),
        secrets.clone(),
        settings(Some(bridge.uri()), vec![echo.uri()]),
    );

    let result = auth_context::with_context(admin_ctx(), svc.refresh_public_ip())
        .await
        .unwrap();
    assert_eq!(result, None);
    // `.expect(0)` is verified when `bridge` drops here.
}

#[tokio::test]
async fn refresh_publishes_when_ip_changed() {
    let bridge = MockServer::start().await;
    Mock::given(method("PUT"))
        .and(path("/v1/installs/inst-1/ip"))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&bridge)
        .await;
    let echo = echo_server("9.9.9.9").await;

    let config = Arc::new(MockSystemConfig::default());
    let secrets = Arc::new(MockSecretStore::default());
    seed_bridge_identity(&config, &secrets, &bridge.uri(), Some("1.1.1.1")).await;

    let svc = DdnsServiceImpl::with_settings(
        config.clone(),
        secrets.clone(),
        settings(Some(bridge.uri()), vec![echo.uri()]),
    );

    let result = auth_context::with_context(admin_ctx(), svc.refresh_public_ip())
        .await
        .unwrap();
    assert_eq!(result, Some("9.9.9.9".parse().unwrap()));
    assert_eq!(
        config.get(KEY_LAST_IP).await.unwrap().as_deref(),
        Some("9.9.9.9")
    );
}

/// A Cloudflare API mock that serves the zone list for `lookup_zone_id`.
async fn cf_zones_server() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/zones"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "success": true,
            "errors": [],
            "result": [
                { "id": "zone-eg", "name": "example.com" },
                { "id": "zone-other", "name": "other.net" },
            ],
        })))
        .mount(&server)
        .await;
    server
}

#[tokio::test]
async fn configure_cloudflare_requires_admin() {
    let svc = DdnsServiceImpl::new(
        Arc::new(MockSystemConfig::default()),
        Arc::new(MockSecretStore::default()),
    );
    assert!(matches!(
        svc.configure_cloudflare("tok".to_owned(), "home.example.com".to_owned())
            .await
            .unwrap_err(),
        AppError::Forbidden(_)
    ));
}

#[tokio::test]
async fn configure_cloudflare_resolves_zone_and_persists_identity() {
    let cf = cf_zones_server().await;
    let config = Arc::new(MockSystemConfig::default());
    let secrets = Arc::new(MockSecretStore::default());
    let svc = DdnsServiceImpl::with_settings(
        config.clone(),
        secrets.clone(),
        DdnsSettings {
            region_catalog: vec![],
            echo_endpoints: vec![],
            cf_base_url: cf.uri(),
        },
    );

    // A subdomain resolves to its registrable apex zone (longest suffix match).
    let registration = auth_context::with_context(
        admin_ctx(),
        svc.configure_cloudflare("cf-token".to_owned(), "home.example.com".to_owned()),
    )
    .await
    .unwrap();
    assert_eq!(registration.subdomain, "home.example.com");

    assert_eq!(
        config.get(KEY_PROVIDER).await.unwrap().as_deref(),
        Some(PROVIDER_CLOUDFLARE)
    );
    assert_eq!(
        config.get(KEY_DOMAIN).await.unwrap().as_deref(),
        Some("home.example.com")
    );
    assert_eq!(
        config.get(KEY_CF_ZONE_ID).await.unwrap().as_deref(),
        Some("zone-eg")
    );
    assert_eq!(
        secrets.get(SECRET_CF_TOKEN).await.unwrap().unwrap(),
        b"cf-token"
    );
}

#[tokio::test]
async fn configure_cloudflare_errors_when_no_zone_covers_domain() {
    let cf = cf_zones_server().await;
    let config = Arc::new(MockSystemConfig::default());
    let secrets = Arc::new(MockSecretStore::default());
    let svc = DdnsServiceImpl::with_settings(
        config.clone(),
        secrets.clone(),
        DdnsSettings {
            region_catalog: vec![],
            echo_endpoints: vec![],
            cf_base_url: cf.uri(),
        },
    );

    // No accessible zone is a suffix of `unrelated.org` — a user-fixable input
    // error (wrong domain / under-scoped token), so BadRequest, not an outage.
    let err = auth_context::with_context(
        admin_ctx(),
        svc.configure_cloudflare("cf-token".to_owned(), "unrelated.org".to_owned()),
    )
    .await
    .unwrap_err();
    assert!(matches!(err, AppError::BadRequest(_)));
    // Nothing should have been persisted on failure.
    assert_eq!(config.get(KEY_PROVIDER).await.unwrap(), None);
    assert_eq!(secrets.get(SECRET_CF_TOKEN).await.unwrap(), None);
}

#[tokio::test]
async fn configure_cloudflare_rejected_token_is_bad_request() {
    // A 403 from Cloudflare (bad/under-scoped token) is a user-fixable error, so
    // it must surface as BadRequest (keep the operator on the form) rather than
    // UpstreamUnavailable (which the wizard treats as a retryable outage).
    let cf = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/zones"))
        .respond_with(ResponseTemplate::new(403).set_body_json(serde_json::json!({
            "success": false,
            "errors": [{ "message": "Invalid API token" }],
            "result": null,
        })))
        .mount(&cf)
        .await;

    let config = Arc::new(MockSystemConfig::default());
    let secrets = Arc::new(MockSecretStore::default());
    let svc = DdnsServiceImpl::with_settings(
        config.clone(),
        secrets.clone(),
        DdnsSettings {
            region_catalog: vec![],
            echo_endpoints: vec![],
            cf_base_url: cf.uri(),
        },
    );

    let err = auth_context::with_context(
        admin_ctx(),
        svc.configure_cloudflare("bad-token".to_owned(), "home.example.com".to_owned()),
    )
    .await
    .unwrap_err();
    assert!(matches!(err, AppError::BadRequest(_)));
    assert_eq!(config.get(KEY_PROVIDER).await.unwrap(), None);
    assert_eq!(secrets.get(SECRET_CF_TOKEN).await.unwrap(), None);
}

#[tokio::test]
async fn acme_challenge_requires_admin() {
    let svc = DdnsServiceImpl::new(
        Arc::new(MockSystemConfig::default()),
        Arc::new(MockSecretStore::default()),
    );
    assert!(matches!(
        svc.set_acme_challenge("token").await.unwrap_err(),
        AppError::Forbidden(_)
    ));
    assert!(matches!(
        svc.clear_acme_challenge().await.unwrap_err(),
        AppError::Forbidden(_)
    ));
}

#[tokio::test]
async fn acme_challenge_errors_when_unconfigured() {
    let svc = DdnsServiceImpl::new(
        Arc::new(MockSystemConfig::default()),
        Arc::new(MockSecretStore::default()),
    );
    // Unconfigured DDNS → no provider → Conflict (can't publish a challenge).
    assert!(matches!(
        auth_context::with_context(admin_ctx(), svc.set_acme_challenge("token"))
            .await
            .unwrap_err(),
        AppError::Conflict(_)
    ));
    assert!(matches!(
        auth_context::with_context(admin_ctx(), svc.clear_acme_challenge())
            .await
            .unwrap_err(),
        AppError::Conflict(_)
    ));
}

#[tokio::test]
async fn set_acme_challenge_publishes_txt_via_bridge() {
    let bridge = MockServer::start().await;
    Mock::given(method("PUT"))
        .and(path("/v1/installs/inst-1/acme-challenge"))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&bridge)
        .await;

    let config = Arc::new(MockSystemConfig::default());
    let secrets = Arc::new(MockSecretStore::default());
    seed_bridge_identity(&config, &secrets, &bridge.uri(), None).await;

    let svc = DdnsServiceImpl::with_settings(
        config.clone(),
        secrets.clone(),
        settings(Some(bridge.uri()), vec![]),
    );

    auth_context::with_context(admin_ctx(), svc.set_acme_challenge("challenge-value"))
        .await
        .unwrap();
    // `.expect(1)` is verified when `bridge` drops here.
}

#[tokio::test]
async fn clear_acme_challenge_deletes_txt_via_bridge() {
    let bridge = MockServer::start().await;
    Mock::given(method("DELETE"))
        .and(path("/v1/installs/inst-1/acme-challenge"))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&bridge)
        .await;

    let config = Arc::new(MockSystemConfig::default());
    let secrets = Arc::new(MockSecretStore::default());
    seed_bridge_identity(&config, &secrets, &bridge.uri(), None).await;

    let svc = DdnsServiceImpl::with_settings(
        config.clone(),
        secrets.clone(),
        settings(Some(bridge.uri()), vec![]),
    );

    auth_context::with_context(admin_ctx(), svc.clear_acme_challenge())
        .await
        .unwrap();
    // `.expect(1)` is verified when `bridge` drops here.
}
