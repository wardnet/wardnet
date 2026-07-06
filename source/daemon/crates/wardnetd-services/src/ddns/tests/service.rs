//! Service-layer tests. Auth gating and the unconfigured short-circuit use mock
//! repositories; enrollment, registration, IP-refresh, ACME and teardown run
//! against `wiremock` tenants + ddns servers so the persistence + auth flow is
//! exercised end to end.
//!
//! Every JWT + `PoP` call first mints a token via the tenants server's
//! `POST /tenants/v1/token`; the [`mount_token`] helper answers it with a JWT-shaped
//! body (see `crate::cloud::tests` for the canonical pattern).

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;

use async_trait::async_trait;
use base64::Engine as _;
use serde_json::json;
use uuid::Uuid;
use wardnet_common::api::DdnsResolutionVerdict;
use wardnet_common::auth::AuthContext;
use wardnetd_data::repository::SystemConfigRepository;
use wiremock::matchers::{body_json, header_exists, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::auth_context;
use crate::ddns::region::RegionEndpoint;
use crate::ddns::{
    DdnsService, DdnsServiceImpl, DdnsSettings, KEY_CF_ZONE_ID, KEY_DOMAIN, KEY_LAST_IP,
    KEY_NETWORK_ID, KEY_PROVIDER, KEY_REGION, KEY_SLUG, KEY_SUBDOMAIN, KEY_TENANT_ID,
    PROVIDER_CLOUDFLARE, PROVIDER_WARDNET, SECRET_CF_TOKEN, SECRET_DAEMON_KEY,
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
    async fn delete(&self, key: &str) -> anyhow::Result<()> {
        self.store.lock().unwrap().remove(key);
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

/// A JWT-shaped string whose payload decodes to `{"exp": exp}` (mirrors the
/// canonical helper in `crate::cloud::tests`).
fn fake_jwt(exp: i64) -> String {
    let payload =
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(format!("{{\"exp\":{exp}}}"));
    format!("header.{payload}.sig")
}

/// Mount `POST /tenants/v1/token` on a tenants server, answering with a fresh,
/// far-from-expiry JWT. Required by every JWT + `PoP` call the service makes.
async fn mount_token(server: &MockServer) {
    let exp = chrono::Utc::now().timestamp() + 3600;
    Mock::given(method("POST"))
        .and(path("/tenants/v1/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "token": fake_jwt(exp) })))
        .mount(server)
        .await;
}

/// Build settings pointed at a wiremock tenants server, with an explicit region
/// catalog and echo endpoints. Cloudflare/DoH are left empty (their tests build
/// `DdnsSettings` directly).
fn settings(
    region_catalog: Vec<RegionEndpoint>,
    tenants_url: String,
    echo: Vec<String>,
) -> DdnsSettings {
    DdnsSettings {
        region_catalog,
        global_gateway_url: tenants_url,
        echo_endpoints: echo,
        cf_base_url: String::new(),
        doh_resolvers: vec![],
    }
}

/// A one-region catalog whose health probe + control base both target `ddns`.
fn region_catalog(slug: &str, ddns: &MockServer) -> Vec<RegionEndpoint> {
    vec![RegionEndpoint {
        slug: slug.to_owned(),
        gateway_base_url: ddns.uri(),
        health_url: format!("{}/ddns/v1/health", ddns.uri()),
    }]
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

/// Pre-seed a fully registered wardnet identity (provider + network + key) into
/// the mock config + secrets.
async fn seed_wardnet_identity(
    config: &MockSystemConfig,
    secrets: &MockSecretStore,
    region_slug: &str,
    last_ip: Option<&str>,
) {
    config.set(KEY_PROVIDER, PROVIDER_WARDNET).await.unwrap();
    config.set(KEY_TENANT_ID, "t-1").await.unwrap();
    config.set(KEY_NETWORK_ID, "net-1").await.unwrap();
    config.set(KEY_SLUG, "happy").await.unwrap();
    config
        .set(KEY_SUBDOMAIN, "happy.my.wardnet.services")
        .await
        .unwrap();
    config.set(KEY_REGION, region_slug).await.unwrap();
    if let Some(ip) = last_ip {
        config.set(KEY_LAST_IP, ip).await.unwrap();
    }
    secrets.put(SECRET_DAEMON_KEY, &[3u8; 32]).await.unwrap();
}

/// Pre-seed only the enrolled identity (key, no network/provider) — the state
/// after [`DdnsService::enroll`] but before [`DdnsService::register_network`].
async fn seed_enrolled(secrets: &MockSecretStore) {
    secrets.put(SECRET_DAEMON_KEY, &[5u8; 32]).await.unwrap();
}

// ── Auth gating + unconfigured short-circuits ───────────────────────────────────

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
        svc.register_network("alice".to_owned(), None)
            .await
            .unwrap_err(),
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

// ── Enrollment ──────────────────────────────────────────────────────────────────

#[tokio::test]
async fn request_enrollment_code_requires_admin() {
    let svc = DdnsServiceImpl::new(
        Arc::new(MockSystemConfig::default()),
        Arc::new(MockSecretStore::default()),
    );
    assert!(matches!(
        svc.request_enrollment_code("a@b.com".to_owned())
            .await
            .unwrap_err(),
        AppError::Forbidden(_)
    ));
}

#[tokio::test]
async fn request_enrollment_code_posts_email_and_purpose() {
    let tenants = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/tenants/v1/verification-codes"))
        .and(body_json(
            json!({ "email": "a@b.com", "purpose": "enrollment" }),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "code": null })))
        .expect(1)
        .mount(&tenants)
        .await;

    let svc = DdnsServiceImpl::with_settings(
        Arc::new(MockSystemConfig::default()),
        Arc::new(MockSecretStore::default()),
        settings(vec![], tenants.uri(), vec![]),
    );
    auth_context::with_context(
        admin_ctx(),
        svc.request_enrollment_code("a@b.com".to_owned()),
    )
    .await
    .unwrap();
    // `.expect(1)` is verified when `tenants` drops here.
}

/// The FULL chain the e2e harness relies on: a real TOML file (exact same
/// `[ddns_wardnet]` shape `compose.ui.yaml` injects) parsed by
/// `ApplicationConfiguration::load`, then wired through the same two calls
/// `lib.rs::create_services` makes (`DdnsSettings::with_wardnet_overrides`
/// then `DdnsServiceImpl::with_settings`), then a real HTTP call — proving
/// nothing is lost in the glue between "config parses" and "settings struct
/// has the right value" (each already covered by their own tests).
#[tokio::test]
async fn ddns_wardnet_toml_section_is_wired_through_to_a_real_call() {
    let tenants = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/tenants/v1/verification-codes"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "code": null })))
        .expect(1)
        .mount(&tenants)
        .await;

    let dir = std::env::temp_dir().join("wardnet-ddns-wardnet-e2e-chain-test");
    let _ = std::fs::create_dir_all(&dir);
    let path_toml = dir.join("wardnet-ddns-wardnet-chain.toml");
    std::fs::write(
        &path_toml,
        format!(
            "\n[ddns_wardnet]\ngateway_url = \"{}\"\nregion_gateway_url = \"{}\"\nregion_health_url = \"{}/ddns/v1/health\"\n",
            tenants.uri(),
            tenants.uri(),
            tenants.uri(),
        ),
    )
    .unwrap();
    let config = wardnet_common::config::ApplicationConfiguration::load(&path_toml).unwrap();
    let _ = std::fs::remove_file(&path_toml);

    let ddns_settings = DdnsSettings::with_wardnet_overrides(
        config.ddns_wardnet.gateway_url.as_deref(),
        config.ddns_wardnet.region_gateway_url.as_deref(),
        config.ddns_wardnet.region_health_url.as_deref(),
    );
    let svc = DdnsServiceImpl::with_settings(
        Arc::new(MockSystemConfig::default()),
        Arc::new(MockSecretStore::default()),
        ddns_settings,
    );
    auth_context::with_context(
        admin_ctx(),
        svc.request_enrollment_code("a@b.com".to_owned()),
    )
    .await
    .unwrap();
    // `.expect(1)` is verified when `tenants` drops here.
}

/// End-to-end through `DdnsSettings::with_wardnet_overrides` (not the
/// hand-built `settings()` helper the other enrollment tests use) — proves
/// the config-driven override path (used in production by `lib.rs` and by
/// the e2e web-ui harness's `[ddns_wardnet]` toml section) produces a
/// `TenantsClient` that still hits the gateway-prefixed path, not just that
/// the override sets the right string on `DdnsSettings`.
#[tokio::test]
async fn wardnet_overrides_gateway_url_is_wired_through_to_a_real_call() {
    let tenants = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/tenants/v1/verification-codes"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "code": null })))
        .expect(1)
        .mount(&tenants)
        .await;

    let svc = DdnsServiceImpl::with_settings(
        Arc::new(MockSystemConfig::default()),
        Arc::new(MockSecretStore::default()),
        DdnsSettings::with_wardnet_overrides(Some(&tenants.uri()), None, None),
    );
    auth_context::with_context(
        admin_ctx(),
        svc.request_enrollment_code("a@b.com".to_owned()),
    )
    .await
    .unwrap();
    // `.expect(1)` is verified when `tenants` drops here.
}

#[tokio::test]
async fn request_enrollment_code_rejects_invalid_email_locally() {
    let svc = DdnsServiceImpl::new(
        Arc::new(MockSystemConfig::default()),
        Arc::new(MockSecretStore::default()),
    );
    // No `@` → rejected before any network call.
    let err =
        auth_context::with_context(admin_ctx(), svc.request_enrollment_code("nope".to_owned()))
            .await
            .unwrap_err();
    assert!(matches!(err, AppError::BadRequest(_)));
}

#[tokio::test]
async fn enroll_persists_key_and_tenant_id() {
    let tenants = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/tenants/v1/enroll"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "tenant_id": "t-1" })))
        .expect(1)
        .mount(&tenants)
        .await;

    let config = Arc::new(MockSystemConfig::default());
    let secrets = Arc::new(MockSecretStore::default());
    let svc = DdnsServiceImpl::with_settings(
        config.clone(),
        secrets.clone(),
        settings(vec![], tenants.uri(), vec![]),
    );

    auth_context::with_context(admin_ctx(), svc.enroll("ABC123".to_owned()))
        .await
        .unwrap();

    // The fresh Ed25519 seed and the tenant binding are persisted…
    assert_eq!(
        secrets.get(SECRET_DAEMON_KEY).await.unwrap().unwrap().len(),
        32
    );
    assert_eq!(
        config.get(KEY_TENANT_ID).await.unwrap().as_deref(),
        Some("t-1")
    );
    // …but the provider is NOT yet wardnet — there is no network until
    // `register_network` runs.
    assert_eq!(config.get(KEY_PROVIDER).await.unwrap(), None);
}

#[tokio::test]
async fn enroll_rejects_empty_code_locally() {
    let svc = DdnsServiceImpl::new(
        Arc::new(MockSystemConfig::default()),
        Arc::new(MockSecretStore::default()),
    );
    let err = auth_context::with_context(admin_ctx(), svc.enroll("   ".to_owned()))
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::BadRequest(_)));
}

// ── Slug availability ───────────────────────────────────────────────────────────

#[tokio::test]
async fn check_slug_returns_available() {
    let tenants = MockServer::start().await;
    mount_token(&tenants).await;
    Mock::given(method("GET"))
        .and(path("/tenants/v1/availability"))
        .and(query_param("slug", "alice"))
        .and(header_exists("authorization"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "available": true })))
        .mount(&tenants)
        .await;

    let secrets = Arc::new(MockSecretStore::default());
    seed_enrolled(&secrets).await;
    let svc = DdnsServiceImpl::with_settings(
        Arc::new(MockSystemConfig::default()),
        secrets,
        settings(vec![], tenants.uri(), vec![]),
    );

    let available = auth_context::with_context(admin_ctx(), svc.check_slug("alice".to_owned()))
        .await
        .unwrap();
    assert!(available);
}

#[tokio::test]
async fn check_slug_returns_taken() {
    let tenants = MockServer::start().await;
    mount_token(&tenants).await;
    Mock::given(method("GET"))
        .and(path("/tenants/v1/availability"))
        .and(query_param("slug", "taken"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "available": false })))
        .mount(&tenants)
        .await;

    let secrets = Arc::new(MockSecretStore::default());
    seed_enrolled(&secrets).await;
    let svc = DdnsServiceImpl::with_settings(
        Arc::new(MockSystemConfig::default()),
        secrets,
        settings(vec![], tenants.uri(), vec![]),
    );

    let available = auth_context::with_context(admin_ctx(), svc.check_slug("taken".to_owned()))
        .await
        .unwrap();
    assert!(!available);
}

#[tokio::test]
async fn check_slug_rejects_invalid_locally() {
    // A slug with a path separator short-circuits to `false` with no network
    // call (no server wired) — both a path-injection guard and the cloud's own
    // "invalid ⇒ unavailable" semantics.
    let svc = DdnsServiceImpl::new(
        Arc::new(MockSystemConfig::default()),
        Arc::new(MockSecretStore::default()),
    );
    let available = auth_context::with_context(admin_ctx(), svc.check_slug("bad/name".to_owned()))
        .await
        .unwrap();
    assert!(!available);
}

#[tokio::test]
async fn check_slug_not_enrolled_is_conflict() {
    // A valid slug but no enrolled identity → availability is tenant-scoped, so
    // it fails fast with Conflict before any network call.
    let svc = DdnsServiceImpl::new(
        Arc::new(MockSystemConfig::default()),
        Arc::new(MockSecretStore::default()),
    );
    let err = auth_context::with_context(admin_ctx(), svc.check_slug("alice".to_owned()))
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::Conflict(_)));
}

// ── Network registration ────────────────────────────────────────────────────────

#[tokio::test]
async fn register_network_persists_wardnet_provider() {
    let tenants = MockServer::start().await;
    mount_token(&tenants).await;
    Mock::given(method("POST"))
        .and(path("/tenants/v1/networks"))
        .and(body_json(json!({ "slug": "alice", "region": "use1" })))
        .and(header_exists("authorization"))
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
        .expect(1)
        .mount(&tenants)
        .await;

    // The region health probe (`select_best`) must see a reachable region.
    let ddns = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/ddns/v1/health"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&ddns)
        .await;

    let config = Arc::new(MockSystemConfig::default());
    let secrets = Arc::new(MockSecretStore::default());
    seed_enrolled(&secrets).await;
    let svc = DdnsServiceImpl::with_settings(
        config.clone(),
        secrets.clone(),
        settings(region_catalog("use1", &ddns), tenants.uri(), vec![]),
    );

    let registration =
        auth_context::with_context(admin_ctx(), svc.register_network("alice".to_owned(), None))
            .await
            .unwrap();
    assert_eq!(registration.subdomain, "alice.my.wardnet.services");
    assert_eq!(registration.region, "use1");

    assert_eq!(
        config.get(KEY_PROVIDER).await.unwrap().as_deref(),
        Some(PROVIDER_WARDNET)
    );
    assert_eq!(
        config.get(KEY_NETWORK_ID).await.unwrap().as_deref(),
        Some("n-1")
    );
    assert_eq!(
        config.get(KEY_SLUG).await.unwrap().as_deref(),
        Some("alice")
    );
    assert_eq!(
        config.get(KEY_SUBDOMAIN).await.unwrap().as_deref(),
        Some("alice.my.wardnet.services")
    );
    assert_eq!(
        config.get(KEY_REGION).await.unwrap().as_deref(),
        Some("use1")
    );
    assert!(
        svc.entitlement().is_entitled(),
        "registering the wardnet provider should flip the box to premium-entitled"
    );
}

#[tokio::test]
async fn register_network_rejects_invalid_slug_before_any_network_call() {
    // A slug with a path separator must be rejected locally (BadRequest) so it
    // can never be interpolated into the tenants request URL.
    let svc = DdnsServiceImpl::new(
        Arc::new(MockSystemConfig::default()),
        Arc::new(MockSecretStore::default()),
    );
    let err = auth_context::with_context(
        admin_ctx(),
        svc.register_network("../networks/x".to_owned(), None),
    )
    .await
    .unwrap_err();
    assert!(matches!(err, AppError::BadRequest(_)));
}

// ── Public-IP refresh ───────────────────────────────────────────────────────────

#[tokio::test]
async fn refresh_skips_when_ip_unchanged() {
    let tenants = MockServer::start().await;
    let ddns = MockServer::start().await;
    Mock::given(method("PUT"))
        .and(path("/ddns/v1/ip"))
        .respond_with(ResponseTemplate::new(204))
        .expect(0) // must NOT be called when the IP is unchanged
        .mount(&ddns)
        .await;
    let echo = echo_server("1.2.3.4").await;

    let config = Arc::new(MockSystemConfig::default());
    let secrets = Arc::new(MockSecretStore::default());
    seed_wardnet_identity(&config, &secrets, "use1", Some("1.2.3.4")).await;

    let svc = DdnsServiceImpl::with_settings(
        config.clone(),
        secrets.clone(),
        settings(
            region_catalog("use1", &ddns),
            tenants.uri(),
            vec![echo.uri()],
        ),
    );

    let result = auth_context::with_context(admin_ctx(), svc.refresh_public_ip())
        .await
        .unwrap();
    assert_eq!(result, None);
    // `.expect(0)` is verified when `ddns` drops here.
}

#[tokio::test]
async fn refresh_publishes_when_ip_changed() {
    let tenants = MockServer::start().await;
    mount_token(&tenants).await;
    let ddns = MockServer::start().await;
    Mock::given(method("PUT"))
        .and(path("/ddns/v1/ip"))
        .and(body_json(json!({ "ip": "9.9.9.9" })))
        .and(header_exists("authorization"))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&ddns)
        .await;
    let echo = echo_server("9.9.9.9").await;

    let config = Arc::new(MockSystemConfig::default());
    let secrets = Arc::new(MockSecretStore::default());
    seed_wardnet_identity(&config, &secrets, "use1", Some("1.1.1.1")).await;

    let svc = DdnsServiceImpl::with_settings(
        config.clone(),
        secrets.clone(),
        settings(
            region_catalog("use1", &ddns),
            tenants.uri(),
            vec![echo.uri()],
        ),
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

// ── ACME challenge ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn acme_challenge_requires_admin() {
    let svc = DdnsServiceImpl::new(
        Arc::new(MockSystemConfig::default()),
        Arc::new(MockSecretStore::default()),
    );
    assert!(matches!(
        svc.set_acme_challenge(&["token".to_string()])
            .await
            .unwrap_err(),
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
        auth_context::with_context(admin_ctx(), svc.set_acme_challenge(&["token".to_string()]))
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
async fn set_acme_challenge_publishes_txt_via_ddns() {
    let tenants = MockServer::start().await;
    mount_token(&tenants).await;
    let ddns = MockServer::start().await;
    Mock::given(method("PUT"))
        .and(path("/ddns/v1/acme-challenge"))
        .and(body_json(
            json!({ "values": ["apex-value", "wildcard-value"] }),
        ))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&ddns)
        .await;

    let config = Arc::new(MockSystemConfig::default());
    let secrets = Arc::new(MockSecretStore::default());
    seed_wardnet_identity(&config, &secrets, "use1", None).await;

    let svc = DdnsServiceImpl::with_settings(
        config.clone(),
        secrets.clone(),
        settings(region_catalog("use1", &ddns), tenants.uri(), vec![]),
    );

    auth_context::with_context(
        admin_ctx(),
        svc.set_acme_challenge(&["apex-value".to_string(), "wildcard-value".to_string()]),
    )
    .await
    .unwrap();
    // `.expect(1)` is verified when `ddns` drops here.
}

#[tokio::test]
async fn clear_acme_challenge_deletes_txt_via_ddns() {
    let tenants = MockServer::start().await;
    mount_token(&tenants).await;
    let ddns = MockServer::start().await;
    Mock::given(method("DELETE"))
        .and(path("/ddns/v1/acme-challenge"))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&ddns)
        .await;

    let config = Arc::new(MockSystemConfig::default());
    let secrets = Arc::new(MockSecretStore::default());
    seed_wardnet_identity(&config, &secrets, "use1", None).await;

    let svc = DdnsServiceImpl::with_settings(
        config.clone(),
        secrets.clone(),
        settings(region_catalog("use1", &ddns), tenants.uri(), vec![]),
    );

    auth_context::with_context(admin_ctx(), svc.clear_acme_challenge())
        .await
        .unwrap();
    // `.expect(1)` is verified when `ddns` drops here.
}

// ── Entitlement ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn token_403_surfaces_as_forbidden() {
    // A 403 from token minting (lapsed subscription) must surface as Forbidden so
    // the wizard can drive the Suspended state, not a generic outage.
    let tenants = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/tenants/v1/token"))
        .respond_with(ResponseTemplate::new(403).set_body_string("subscription is not active"))
        .mount(&tenants)
        .await;

    let secrets = Arc::new(MockSecretStore::default());
    seed_enrolled(&secrets).await;
    let svc = DdnsServiceImpl::with_settings(
        Arc::new(MockSystemConfig::default()),
        secrets,
        settings(vec![], tenants.uri(), vec![]),
    );

    let err = auth_context::with_context(admin_ctx(), svc.check_slug("alice".to_owned()))
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::Forbidden(_)));
}

// ── Cloudflare (BYOD) ───────────────────────────────────────────────────────────

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

/// Cloudflare-only settings: an empty region catalog + tenants URL (unused on the
/// BYOD path), with the CF API base pointed at `cf`.
fn cloudflare_settings(cf: &MockServer) -> DdnsSettings {
    DdnsSettings {
        region_catalog: vec![],
        global_gateway_url: String::new(),
        echo_endpoints: vec![],
        cf_base_url: cf.uri(),
        doh_resolvers: vec![],
    }
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
    let svc =
        DdnsServiceImpl::with_settings(config.clone(), secrets.clone(), cloudflare_settings(&cf));

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
    assert!(
        !svc.entitlement().is_entitled(),
        "BYOD-Cloudflare is a free provider — it must not flip premium entitlement"
    );
}

#[tokio::test]
async fn configure_cloudflare_errors_when_no_zone_covers_domain() {
    let cf = cf_zones_server().await;
    let config = Arc::new(MockSystemConfig::default());
    let secrets = Arc::new(MockSecretStore::default());
    let svc =
        DdnsServiceImpl::with_settings(config.clone(), secrets.clone(), cloudflare_settings(&cf));

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
    let svc =
        DdnsServiceImpl::with_settings(config.clone(), secrets.clone(), cloudflare_settings(&cf));

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

// ── Status ──────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn status_reports_configured_wardnet_provider() {
    let config = Arc::new(MockSystemConfig::default());
    let secrets = Arc::new(MockSecretStore::default());
    seed_wardnet_identity(&config, &secrets, "use1", Some("9.9.9.9")).await;
    let svc =
        DdnsServiceImpl::with_settings(config, secrets, settings(vec![], String::new(), vec![]));

    let status = auth_context::with_context(admin_ctx(), svc.status())
        .await
        .unwrap();
    assert_eq!(status.provider.as_deref(), Some(PROVIDER_WARDNET));
    assert_eq!(status.fqdn.as_deref(), Some("happy.my.wardnet.services"));
    assert_eq!(status.last_public_ip.as_deref(), Some("9.9.9.9"));
}

#[tokio::test]
async fn status_reports_unconfigured() {
    let svc = DdnsServiceImpl::new(
        Arc::new(MockSystemConfig::default()),
        Arc::new(MockSecretStore::default()),
    );
    let status = auth_context::with_context(admin_ctx(), svc.status())
        .await
        .unwrap();
    assert_eq!(status.provider, None);
    assert_eq!(status.fqdn, None);
}

// ── Premium entitlement sync ─────────────────────────────────────────────────────

#[tokio::test]
async fn sync_premium_primes_entitled_from_persisted_wardnet_provider() {
    let config = Arc::new(MockSystemConfig::default());
    let secrets = Arc::new(MockSecretStore::default());
    seed_wardnet_identity(&config, &secrets, "use1", Some("9.9.9.9")).await;
    let svc =
        DdnsServiceImpl::with_settings(config, secrets, settings(vec![], String::new(), vec![]));

    // A fresh handle starts not-entitled, before any sync/mint has run.
    assert!(!svc.entitlement().is_entitled());

    svc.sync_premium().await.unwrap();
    assert!(
        svc.entitlement().is_entitled(),
        "a persisted wardnet provider should prime the box as premium-entitled"
    );
}

#[tokio::test]
async fn sync_premium_leaves_free_provider_not_entitled() {
    let svc = DdnsServiceImpl::new(
        Arc::new(MockSystemConfig::default()),
        Arc::new(MockSecretStore::default()),
    );
    svc.entitlement().set_premium(true); // simulate a stale/incorrect prior state

    svc.sync_premium().await.unwrap();
    assert!(
        !svc.entitlement().is_entitled(),
        "an unconfigured provider must sync to not-entitled"
    );
}

// ── Teardown ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn teardown_removes_daemon_and_wipes_state() {
    let tenants = MockServer::start().await;
    mount_token(&tenants).await;
    // The per-daemon removal endpoint must be hit exactly once.
    Mock::given(method("DELETE"))
        .and(path("/tenants/v1/networks/net-1/daemons/self"))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&tenants)
        .await;
    let ddns = MockServer::start().await;

    let config = Arc::new(MockSystemConfig::default());
    let secrets = Arc::new(MockSecretStore::default());
    seed_wardnet_identity(&config, &secrets, "use1", Some("9.9.9.9")).await;

    let svc = DdnsServiceImpl::with_settings(
        config.clone(),
        secrets.clone(),
        settings(region_catalog("use1", &ddns), tenants.uri(), vec![]),
    );
    // Simulate a box that was already premium-entitled (as it would be after
    // `register_network`/`sync_premium`) before tearing the provider down.
    svc.entitlement().set_premium(true);

    auth_context::with_context(admin_ctx(), svc.teardown())
        .await
        .unwrap();

    // Config + secrets are wiped → `status()` reports unconfigured.
    assert_eq!(config.get(KEY_PROVIDER).await.unwrap(), None);
    assert_eq!(config.get(KEY_TENANT_ID).await.unwrap(), None);
    assert_eq!(config.get(KEY_NETWORK_ID).await.unwrap(), None);
    assert_eq!(config.get(KEY_SLUG).await.unwrap(), None);
    assert_eq!(config.get(KEY_SUBDOMAIN).await.unwrap(), None);
    assert_eq!(config.get(KEY_REGION).await.unwrap(), None);
    assert_eq!(config.get(KEY_LAST_IP).await.unwrap(), None);
    assert_eq!(secrets.get(SECRET_DAEMON_KEY).await.unwrap(), None);
    assert!(
        !svc.entitlement().is_entitled(),
        "tearing down the wardnet provider should revoke premium entitlement"
    );
    // `.expect(1)` on the DELETE is verified when `tenants` drops here.
}

#[tokio::test]
async fn teardown_is_idempotent_when_unconfigured() {
    let svc = DdnsServiceImpl::new(
        Arc::new(MockSystemConfig::default()),
        Arc::new(MockSecretStore::default()),
    );
    // No provider configured → no network call, returns Ok.
    auth_context::with_context(admin_ctx(), svc.teardown())
        .await
        .unwrap();
}

#[tokio::test]
async fn teardown_deletes_cloudflare_records_and_wipes_state() {
    let cf = MockServer::start().await;
    // Record lookup: both the A and the ACME TXT queries resolve to a record id.
    Mock::given(method("GET"))
        .and(path("/zones/zone-eg/dns_records"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "success": true, "errors": [], "result": [{ "id": "rec-1" }],
        })))
        .mount(&cf)
        .await;
    // CloudflareProvider::teardown deletes the A record, then the TXT — two DELETEs.
    Mock::given(method("DELETE"))
        .and(path("/zones/zone-eg/dns_records/rec-1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "success": true, "errors": [], "result": { "id": "rec-1" },
        })))
        .expect(2)
        .mount(&cf)
        .await;

    let config = Arc::new(MockSystemConfig::default());
    let secrets = Arc::new(MockSecretStore::default());
    config.set(KEY_PROVIDER, PROVIDER_CLOUDFLARE).await.unwrap();
    config.set(KEY_DOMAIN, "home.example.com").await.unwrap();
    config.set(KEY_CF_ZONE_ID, "zone-eg").await.unwrap();
    secrets.put(SECRET_CF_TOKEN, b"cf-token").await.unwrap();

    let svc =
        DdnsServiceImpl::with_settings(config.clone(), secrets.clone(), cloudflare_settings(&cf));

    auth_context::with_context(admin_ctx(), svc.teardown())
        .await
        .unwrap();

    assert_eq!(config.get(KEY_PROVIDER).await.unwrap(), None);
    assert_eq!(config.get(KEY_DOMAIN).await.unwrap(), None);
    assert_eq!(config.get(KEY_CF_ZONE_ID).await.unwrap(), None);
    assert_eq!(secrets.get(SECRET_CF_TOKEN).await.unwrap(), None);
    // `.expect(2)` (A + TXT delete) is verified when `cf` drops here.
}

#[tokio::test]
async fn switching_to_cloudflare_deregisters_old_wardnet() {
    // Old wardnet network: its per-daemon removal must be called once by the switch.
    let tenants = MockServer::start().await;
    mount_token(&tenants).await;
    Mock::given(method("DELETE"))
        .and(path("/tenants/v1/networks/net-1/daemons/self"))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&tenants)
        .await;
    let cf = cf_zones_server().await;
    let ddns = MockServer::start().await;

    let config = Arc::new(MockSystemConfig::default());
    let secrets = Arc::new(MockSecretStore::default());
    seed_wardnet_identity(&config, &secrets, "use1", None).await;

    let svc = DdnsServiceImpl::with_settings(
        config.clone(),
        secrets.clone(),
        DdnsSettings {
            region_catalog: region_catalog("use1", &ddns),
            global_gateway_url: tenants.uri(),
            echo_endpoints: vec![],
            cf_base_url: cf.uri(),
            doh_resolvers: vec![],
        },
    );

    auth_context::with_context(
        admin_ctx(),
        svc.configure_cloudflare("cf-token".to_owned(), "home.example.com".to_owned()),
    )
    .await
    .unwrap();

    // New provider is committed; the old wardnet identity + secret are cleared.
    assert_eq!(
        config.get(KEY_PROVIDER).await.unwrap().as_deref(),
        Some(PROVIDER_CLOUDFLARE)
    );
    assert_eq!(config.get(KEY_NETWORK_ID).await.unwrap(), None);
    assert_eq!(secrets.get(SECRET_DAEMON_KEY).await.unwrap(), None);
    // `.expect(1)` on the old-wardnet DELETE is verified when `tenants` drops here.
}

// ── Resolution check ────────────────────────────────────────────────────────────

/// Build a DoH-JSON resolver mock returning `answer_ips` as A records with the
/// given `status` code (0 = NOERROR, 3 = NXDOMAIN).
async fn doh_server(status: u32, answer_ips: &[&str]) -> MockServer {
    let server = MockServer::start().await;
    let answer: Vec<_> = answer_ips
        .iter()
        .map(|ip| serde_json::json!({ "type": 1, "data": ip }))
        .collect();
    Mock::given(method("GET"))
        .and(path("/dns-query"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({ "Status": status, "Answer": answer })),
        )
        .mount(&server)
        .await;
    server
}

fn resolution_settings(doh: &MockServer) -> DdnsSettings {
    DdnsSettings {
        region_catalog: vec![],
        global_gateway_url: String::new(),
        echo_endpoints: vec![],
        cf_base_url: String::new(),
        doh_resolvers: vec![format!("{}/dns-query", doh.uri())],
    }
}

/// Seed a configured wardnet identity with a known FQDN + last-published IP, for
/// the resolution-check tests (which only read `status()` + query `DoH`).
async fn seed_for_resolution(config: &MockSystemConfig, last_ip: &str) {
    config.set(KEY_PROVIDER, PROVIDER_WARDNET).await.unwrap();
    config
        .set(KEY_SUBDOMAIN, "happy.my.wardnet.services")
        .await
        .unwrap();
    config.set(KEY_LAST_IP, last_ip).await.unwrap();
}

#[tokio::test]
async fn resolution_check_not_configured() {
    let svc = DdnsServiceImpl::new(
        Arc::new(MockSystemConfig::default()),
        Arc::new(MockSecretStore::default()),
    );
    let result = auth_context::with_context(admin_ctx(), svc.resolution_check())
        .await
        .unwrap();
    assert_eq!(result.verdict, DdnsResolutionVerdict::NotConfigured);
    assert_eq!(result.fqdn, None);
}

#[tokio::test]
async fn resolution_check_reports_match() {
    let doh = doh_server(0, &["9.9.9.9"]).await;
    let config = Arc::new(MockSystemConfig::default());
    seed_for_resolution(&config, "9.9.9.9").await;
    let svc = DdnsServiceImpl::with_settings(
        config,
        Arc::new(MockSecretStore::default()),
        resolution_settings(&doh),
    );

    let result = auth_context::with_context(admin_ctx(), svc.resolution_check())
        .await
        .unwrap();
    assert_eq!(result.verdict, DdnsResolutionVerdict::Match);
    assert_eq!(result.fqdn.as_deref(), Some("happy.my.wardnet.services"));
    assert_eq!(result.resolved_ips, vec!["9.9.9.9".to_owned()]);
}

#[tokio::test]
async fn resolution_check_reports_mismatch() {
    let doh = doh_server(0, &["1.2.3.4"]).await;
    let config = Arc::new(MockSystemConfig::default());
    seed_for_resolution(&config, "9.9.9.9").await;
    let svc = DdnsServiceImpl::with_settings(
        config,
        Arc::new(MockSecretStore::default()),
        resolution_settings(&doh),
    );

    let result = auth_context::with_context(admin_ctx(), svc.resolution_check())
        .await
        .unwrap();
    assert_eq!(result.verdict, DdnsResolutionVerdict::Mismatch);
}

#[tokio::test]
async fn resolution_check_reports_pending_on_nxdomain() {
    let doh = doh_server(3, &[]).await;
    let config = Arc::new(MockSystemConfig::default());
    seed_for_resolution(&config, "9.9.9.9").await;
    let svc = DdnsServiceImpl::with_settings(
        config,
        Arc::new(MockSecretStore::default()),
        resolution_settings(&doh),
    );

    let result = auth_context::with_context(admin_ctx(), svc.resolution_check())
        .await
        .unwrap();
    assert_eq!(result.verdict, DdnsResolutionVerdict::Pending);
}
