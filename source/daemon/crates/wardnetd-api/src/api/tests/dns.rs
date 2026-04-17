//! Tests for the DNS API endpoints.
//!
//! Verifies that routes are wired correctly, that the admin-auth guard
//! rejects unauthenticated requests, and that responses have the expected
//! shape.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use async_trait::async_trait;
use axum::Router;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use axum::routing::{get, post};
use tower::ServiceExt;
use uuid::Uuid;
use wardnet_common::api::{
    CreateAllowlistRequest, CreateAllowlistResponse, CreateBlocklistRequest,
    CreateBlocklistResponse, CreateFilterRuleRequest, CreateFilterRuleResponse,
    DeleteAllowlistResponse, DeleteBlocklistResponse, DeleteFilterRuleResponse,
    DnsCacheFlushResponse, DnsConfigResponse, DnsStatusResponse, ListAllowlistResponse,
    ListBlocklistsResponse, ListFilterRulesResponse, ToggleDnsRequest, UpdateBlocklistNowResponse,
    UpdateBlocklistRequest, UpdateBlocklistResponse, UpdateDnsConfigRequest,
    UpdateFilterRuleRequest, UpdateFilterRuleResponse,
};
use wardnet_common::dns::{DnsConfig, DnsResolutionMode};

use crate::state::AppState;
use crate::tests::stubs::{
    StubDeviceService, StubDhcpServer, StubDhcpService, StubDiscoveryService, StubDnsServer,
    StubEventPublisher, StubLogService, StubProviderService, StubRoutingService, StubSystemService,
    StubTunnelService,
};
use wardnetd_services::LogService;
use wardnetd_services::auth::service::LoginResult;
use wardnetd_services::error::AppError;
use wardnetd_services::{AuthService, DnsService};

// ---------------------------------------------------------------------------
// Mock services
// ---------------------------------------------------------------------------

/// Mock auth service that always validates sessions as admin.
struct MockAuthService;

#[async_trait]
impl AuthService for MockAuthService {
    async fn login(&self, _u: &str, _p: &str) -> Result<LoginResult, AppError> {
        Ok(LoginResult {
            token: "t".to_owned(),
            max_age_seconds: 3600,
        })
    }
    async fn validate_session(&self, _token: &str) -> Result<Option<Uuid>, AppError> {
        Ok(Some(
            Uuid::parse_str("00000000-0000-0000-0000-000000000099").unwrap(),
        ))
    }
    async fn validate_api_key(&self, _key: &str) -> Result<Option<Uuid>, AppError> {
        Ok(None)
    }
    async fn setup_admin(&self, _u: &str, _p: &str) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn is_setup_completed(&self) -> Result<bool, AppError> {
        Ok(true)
    }
}

/// Mock DNS service that returns sensible defaults for all operations.
struct MockDnsService;

impl MockDnsService {
    fn default_config() -> DnsConfig {
        DnsConfig {
            enabled: false,
            resolution_mode: DnsResolutionMode::Forwarding,
            upstream_servers: vec![],
            cache_size: 10_000,
            cache_ttl_min_secs: 0,
            cache_ttl_max_secs: 86_400,
            dnssec_enabled: false,
            rebinding_protection: true,
            rate_limit_per_second: 0,
            ad_blocking_enabled: true,
            query_log_enabled: true,
            query_log_retention_days: 7,
        }
    }
}

#[async_trait]
impl DnsService for MockDnsService {
    async fn get_config(&self) -> Result<DnsConfigResponse, AppError> {
        Ok(DnsConfigResponse {
            config: Self::default_config(),
        })
    }

    async fn update_config(
        &self,
        _req: UpdateDnsConfigRequest,
    ) -> Result<DnsConfigResponse, AppError> {
        // Return the default config regardless of input (mock behaviour).
        Ok(DnsConfigResponse {
            config: Self::default_config(),
        })
    }

    async fn toggle(&self, req: ToggleDnsRequest) -> Result<DnsConfigResponse, AppError> {
        let mut config = Self::default_config();
        config.enabled = req.enabled;
        Ok(DnsConfigResponse { config })
    }

    async fn status(&self) -> Result<DnsStatusResponse, AppError> {
        Ok(DnsStatusResponse {
            enabled: false,
            running: false,
            cache_size: 0,
            cache_capacity: 10_000,
            cache_hit_rate: 0.0,
        })
    }

    async fn flush_cache(&self) -> Result<DnsCacheFlushResponse, AppError> {
        Ok(DnsCacheFlushResponse {
            message: "Cache flushed".to_owned(),
            entries_cleared: 0,
        })
    }

    async fn get_dns_config(&self) -> Result<DnsConfig, AppError> {
        Ok(Self::default_config())
    }

    async fn list_blocklists(&self) -> Result<ListBlocklistsResponse, AppError> {
        Ok(ListBlocklistsResponse { blocklists: vec![] })
    }
    async fn create_blocklist(
        &self,
        req: CreateBlocklistRequest,
    ) -> Result<CreateBlocklistResponse, AppError> {
        let now = chrono::Utc::now();
        Ok(CreateBlocklistResponse {
            blocklist: wardnet_common::dns::Blocklist {
                id: Uuid::new_v4(),
                name: req.name,
                url: req.url,
                enabled: req.enabled,
                entry_count: 0,
                last_updated: None,
                cron_schedule: req.cron_schedule,
                last_error: None,
                last_error_at: None,
                created_at: now,
                updated_at: now,
            },
            message: "blocklist created".to_owned(),
        })
    }
    async fn update_blocklist(
        &self,
        id: Uuid,
        _req: UpdateBlocklistRequest,
    ) -> Result<UpdateBlocklistResponse, AppError> {
        let now = chrono::Utc::now();
        Ok(UpdateBlocklistResponse {
            blocklist: wardnet_common::dns::Blocklist {
                id,
                name: "Updated".to_owned(),
                url: "https://example.com/list.txt".to_owned(),
                enabled: true,
                entry_count: 0,
                last_updated: None,
                cron_schedule: "0 0 3 * * *".to_owned(),
                last_error: None,
                last_error_at: None,
                created_at: now,
                updated_at: now,
            },
            message: "blocklist updated".to_owned(),
        })
    }
    async fn delete_blocklist(&self, id: Uuid) -> Result<DeleteBlocklistResponse, AppError> {
        Ok(DeleteBlocklistResponse {
            message: format!("blocklist {id} deleted"),
        })
    }
    async fn update_blocklist_now(&self, id: Uuid) -> Result<UpdateBlocklistNowResponse, AppError> {
        let now = chrono::Utc::now();
        let blocklist = wardnet_common::dns::Blocklist {
            id,
            name: "Test".to_owned(),
            url: "https://example.com/list.txt".to_owned(),
            enabled: true,
            entry_count: 42,
            last_updated: None,
            cron_schedule: "0 0 3 * * *".to_owned(),
            last_error: None,
            last_error_at: None,
            created_at: now,
            updated_at: now,
        };
        Ok(UpdateBlocklistNowResponse {
            entry_count: blocklist.entry_count,
            blocklist,
            message: "blocklist refresh triggered".to_owned(),
        })
    }
    async fn list_allowlist(&self) -> Result<ListAllowlistResponse, AppError> {
        Ok(ListAllowlistResponse { entries: vec![] })
    }
    async fn create_allowlist_entry(
        &self,
        req: CreateAllowlistRequest,
    ) -> Result<CreateAllowlistResponse, AppError> {
        Ok(CreateAllowlistResponse {
            entry: wardnet_common::dns::AllowlistEntry {
                id: Uuid::new_v4(),
                domain: req.domain,
                reason: req.reason,
                created_at: chrono::Utc::now(),
            },
            message: "allowlist entry created".to_owned(),
        })
    }
    async fn delete_allowlist_entry(&self, id: Uuid) -> Result<DeleteAllowlistResponse, AppError> {
        Ok(DeleteAllowlistResponse {
            message: format!("allowlist entry {id} deleted"),
        })
    }
    async fn list_filter_rules(&self) -> Result<ListFilterRulesResponse, AppError> {
        Ok(ListFilterRulesResponse { rules: vec![] })
    }
    async fn create_filter_rule(
        &self,
        req: CreateFilterRuleRequest,
    ) -> Result<CreateFilterRuleResponse, AppError> {
        let now = chrono::Utc::now();
        Ok(CreateFilterRuleResponse {
            rule: wardnet_common::dns::CustomFilterRule {
                id: Uuid::new_v4(),
                rule_text: req.rule_text,
                enabled: req.enabled,
                comment: req.comment,
                created_at: now,
                updated_at: now,
            },
            message: "filter rule created".to_owned(),
        })
    }
    async fn update_filter_rule(
        &self,
        id: Uuid,
        _req: UpdateFilterRuleRequest,
    ) -> Result<UpdateFilterRuleResponse, AppError> {
        let now = chrono::Utc::now();
        Ok(UpdateFilterRuleResponse {
            rule: wardnet_common::dns::CustomFilterRule {
                id,
                rule_text: "||updated.com^".to_owned(),
                enabled: true,
                comment: None,
                created_at: now,
                updated_at: now,
            },
            message: "filter rule updated".to_owned(),
        })
    }
    async fn delete_filter_rule(&self, id: Uuid) -> Result<DeleteFilterRuleResponse, AppError> {
        Ok(DeleteFilterRuleResponse {
            message: format!("filter rule {id} deleted"),
        })
    }
    async fn load_filter_inputs(
        &self,
    ) -> Result<wardnetd_services::dns::filter::FilterInputs, AppError> {
        Ok(wardnetd_services::dns::filter::FilterInputs::default())
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn connect_info() -> ConnectInfo<SocketAddr> {
    ConnectInfo(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 12345))
}

fn build_state(dns_svc: impl DnsService + 'static) -> AppState {
    AppState::new(
        Arc::new(MockAuthService),
        Arc::new(StubDeviceService),
        Arc::new(StubDhcpService),
        Arc::new(dns_svc),
        Arc::new(StubDiscoveryService),
        Arc::new(StubLogService) as Arc<dyn LogService>,
        Arc::new(StubProviderService),
        Arc::new(StubRoutingService),
        Arc::new(StubSystemService),
        Arc::new(StubTunnelService),
        Arc::new(StubDhcpServer),
        Arc::new(StubDnsServer),
        Arc::new(StubEventPublisher),
    )
}

fn dns_router(state: AppState) -> Router {
    use axum::routing::{delete, put};

    Router::new()
        .route(
            "/api/dns/config",
            get(crate::api::dns::get_config).put(crate::api::dns::update_config),
        )
        .route("/api/dns/config/toggle", post(crate::api::dns::toggle))
        .route("/api/dns/status", get(crate::api::dns::status))
        .route("/api/dns/cache/flush", post(crate::api::dns::flush_cache))
        .route(
            "/api/dns/blocklists",
            get(crate::api::dns::list_blocklists).post(crate::api::dns::create_blocklist),
        )
        .route(
            "/api/dns/blocklists/{id}",
            put(crate::api::dns::update_blocklist).delete(crate::api::dns::delete_blocklist),
        )
        .route(
            "/api/dns/blocklists/{id}/update",
            post(crate::api::dns::update_blocklist_now),
        )
        .route(
            "/api/dns/allowlist",
            get(crate::api::dns::list_allowlist).post(crate::api::dns::create_allowlist_entry),
        )
        .route(
            "/api/dns/allowlist/{id}",
            delete(crate::api::dns::delete_allowlist_entry),
        )
        .route(
            "/api/dns/rules",
            get(crate::api::dns::list_filter_rules).post(crate::api::dns::create_filter_rule),
        )
        .route(
            "/api/dns/rules/{id}",
            put(crate::api::dns::update_filter_rule).delete(crate::api::dns::delete_filter_rule),
        )
        .with_state(state)
}

/// Send an authenticated GET request.
async fn get_json(app: Router, uri: &str) -> (StatusCode, serde_json::Value) {
    let resp = app
        .oneshot(
            Request::builder()
                .uri(uri)
                .header("Cookie", "wardnet_session=valid-token")
                .extension(connect_info())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let status = resp.status();
    let body = axum::body::to_bytes(resp.into_body(), 16384).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap_or(serde_json::Value::Null);
    (status, json)
}

/// Send an authenticated POST request with a JSON body.
async fn post_json(app: Router, uri: &str, json_body: &str) -> (StatusCode, serde_json::Value) {
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(uri)
                .header("Content-Type", "application/json")
                .header("Cookie", "wardnet_session=valid-token")
                .extension(connect_info())
                .body(Body::from(json_body.to_owned()))
                .unwrap(),
        )
        .await
        .unwrap();

    let status = resp.status();
    let body = axum::body::to_bytes(resp.into_body(), 16384).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap_or(serde_json::Value::Null);
    (status, json)
}

/// Send an authenticated PUT request with a JSON body.
async fn put_json(app: Router, uri: &str, json_body: &str) -> (StatusCode, serde_json::Value) {
    let resp = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(uri)
                .header("Content-Type", "application/json")
                .header("Cookie", "wardnet_session=valid-token")
                .extension(connect_info())
                .body(Body::from(json_body.to_owned()))
                .unwrap(),
        )
        .await
        .unwrap();

    let status = resp.status();
    let body = axum::body::to_bytes(resp.into_body(), 16384).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap_or(serde_json::Value::Null);
    (status, json)
}

// ---------------------------------------------------------------------------
// Tests — GET /api/dns/config
// ---------------------------------------------------------------------------

#[tokio::test]
async fn get_config_authenticated() {
    let state = build_state(MockDnsService);
    let app = dns_router(state);

    let (status, json) = get_json(app, "/api/dns/config").await;
    assert_eq!(status, StatusCode::OK);
    assert!(!json["config"]["enabled"].as_bool().unwrap());
    assert_eq!(json["config"]["resolution_mode"], "forwarding");
    assert_eq!(json["config"]["cache_size"], 10_000);
    assert!(json["config"]["rebinding_protection"].as_bool().unwrap());
    assert!(json["config"]["ad_blocking_enabled"].as_bool().unwrap());
    assert!(json["config"]["query_log_enabled"].as_bool().unwrap());
    assert_eq!(json["config"]["query_log_retention_days"], 7);
}

#[tokio::test]
async fn get_config_unauthenticated() {
    let state = build_state(MockDnsService);
    let app = dns_router(state);

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/dns/config")
                .extension(connect_info())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

// ---------------------------------------------------------------------------
// Tests — PUT /api/dns/config
// ---------------------------------------------------------------------------

#[tokio::test]
async fn update_config_authenticated() {
    let state = build_state(MockDnsService);
    let app = dns_router(state);

    let body = serde_json::json!({
        "cache_size": 5000,
        "dnssec_enabled": true,
        "ad_blocking_enabled": false,
    });

    let (status, json) = put_json(app, "/api/dns/config", &body.to_string()).await;
    assert_eq!(status, StatusCode::OK);
    // The mock returns default config regardless of input.
    assert!(json["config"].is_object());
    assert_eq!(json["config"]["cache_size"], 10_000);
    assert!(!json["config"]["enabled"].as_bool().unwrap());
}

// ---------------------------------------------------------------------------
// Tests — POST /api/dns/config/toggle
// ---------------------------------------------------------------------------

#[tokio::test]
async fn toggle_enable() {
    let state = build_state(MockDnsService);
    let app = dns_router(state);

    let body = serde_json::json!({"enabled": true});

    let (status, json) = post_json(app, "/api/dns/config/toggle", &body.to_string()).await;
    assert_eq!(status, StatusCode::OK);
    assert!(json["config"].is_object());
    // The mock toggle respects the requested enabled value.
    assert!(json["config"]["enabled"].as_bool().unwrap());
}

#[tokio::test]
async fn toggle_disable() {
    let state = build_state(MockDnsService);
    let app = dns_router(state);

    let body = serde_json::json!({"enabled": false});

    let (status, json) = post_json(app, "/api/dns/config/toggle", &body.to_string()).await;
    assert_eq!(status, StatusCode::OK);
    assert!(json["config"].is_object());
    assert!(!json["config"]["enabled"].as_bool().unwrap());
}

// ---------------------------------------------------------------------------
// Tests — GET /api/dns/status
// ---------------------------------------------------------------------------

#[tokio::test]
async fn status_authenticated() {
    let state = build_state(MockDnsService);
    let app = dns_router(state);

    let (status, json) = get_json(app, "/api/dns/status").await;
    assert_eq!(status, StatusCode::OK);
    assert!(!json["enabled"].as_bool().unwrap());
    assert!(!json["running"].as_bool().unwrap());
    assert_eq!(json["cache_size"], 0);
    assert_eq!(json["cache_capacity"], 10_000);
    assert_eq!(json["cache_hit_rate"], 0.0);
}

// ---------------------------------------------------------------------------
// Tests — POST /api/dns/cache/flush
// ---------------------------------------------------------------------------

#[tokio::test]
async fn flush_cache_authenticated() {
    let state = build_state(MockDnsService);
    let app = dns_router(state);

    let (status, json) = post_json(app, "/api/dns/cache/flush", "{}").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["message"], "Cache flushed");
    assert_eq!(json["entries_cleared"], 0);
}

#[tokio::test]
async fn flush_cache_unauthenticated() {
    let state = build_state(MockDnsService);
    let app = dns_router(state);

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/dns/cache/flush")
                .header("Content-Type", "application/json")
                .extension(connect_info())
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

/// Send an authenticated DELETE request.
async fn delete_json(app: Router, uri: &str) -> (StatusCode, serde_json::Value) {
    let resp = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(uri)
                .header("Cookie", "wardnet_session=valid-token")
                .extension(connect_info())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let status = resp.status();
    let body = axum::body::to_bytes(resp.into_body(), 16384).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap_or(serde_json::Value::Null);
    (status, json)
}

// ---------------------------------------------------------------------------
// Tests — Blocklists CRUD
// ---------------------------------------------------------------------------

#[tokio::test]
async fn list_blocklists_returns_empty() {
    let state = build_state(MockDnsService);
    let app = dns_router(state);

    let (status, json) = get_json(app, "/api/dns/blocklists").await;
    assert_eq!(status, StatusCode::OK);
    assert!(json["blocklists"].is_array());
    assert_eq!(json["blocklists"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn list_allowlist_returns_empty() {
    let state = build_state(MockDnsService);
    let app = dns_router(state);

    let (status, json) = get_json(app, "/api/dns/allowlist").await;
    assert_eq!(status, StatusCode::OK);
    assert!(json["entries"].is_array());
    assert_eq!(json["entries"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn list_filter_rules_returns_empty() {
    let state = build_state(MockDnsService);
    let app = dns_router(state);

    let (status, json) = get_json(app, "/api/dns/rules").await;
    assert_eq!(status, StatusCode::OK);
    assert!(json["rules"].is_array());
    assert_eq!(json["rules"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn create_blocklist_returns_created() {
    let state = build_state(MockDnsService);
    let app = dns_router(state);

    let body = serde_json::json!({
        "name": "Test blocklist",
        "url": "https://example.com/list.txt",
        "cron_schedule": "0 0 3 * * *",
        "enabled": true
    });

    let (status, json) = post_json(app, "/api/dns/blocklists", &body.to_string()).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(json["message"], "blocklist created");
    assert!(json["blocklist"]["id"].is_string());
}

#[tokio::test]
async fn create_allowlist_entry_returns_created() {
    let state = build_state(MockDnsService);
    let app = dns_router(state);

    let body = serde_json::json!({
        "domain": "safe.example.com",
        "reason": "testing"
    });

    let (status, json) = post_json(app, "/api/dns/allowlist", &body.to_string()).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(json["message"], "allowlist entry created");
    assert_eq!(json["entry"]["domain"], "safe.example.com");
}

#[tokio::test]
async fn create_filter_rule_returns_created() {
    let state = build_state(MockDnsService);
    let app = dns_router(state);

    let body = serde_json::json!({
        "rule_text": "||ads.example.com^",
        "comment": "block ads",
        "enabled": true
    });

    let (status, json) = post_json(app, "/api/dns/rules", &body.to_string()).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(json["message"], "filter rule created");
    assert_eq!(json["rule"]["rule_text"], "||ads.example.com^");
}

#[tokio::test]
async fn update_blocklist_returns_ok() {
    let state = build_state(MockDnsService);
    let app = dns_router(state);
    let id = Uuid::new_v4();

    let body = serde_json::json!({"name": "Renamed"});

    let (status, json) =
        put_json(app, &format!("/api/dns/blocklists/{id}"), &body.to_string()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["message"], "blocklist updated");
}

#[tokio::test]
async fn delete_blocklist_returns_ok() {
    let state = build_state(MockDnsService);
    let app = dns_router(state);
    let id = Uuid::new_v4();

    let (status, json) = delete_json(app, &format!("/api/dns/blocklists/{id}")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(json["message"].as_str().unwrap().contains("deleted"));
}

#[tokio::test]
async fn update_blocklist_now_returns_ok() {
    let state = build_state(MockDnsService);
    let app = dns_router(state);
    let id = Uuid::new_v4();

    let (status, json) = post_json(app, &format!("/api/dns/blocklists/{id}/update"), "{}").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["message"], "blocklist refresh triggered");
}

#[tokio::test]
async fn delete_allowlist_entry_returns_ok() {
    let state = build_state(MockDnsService);
    let app = dns_router(state);
    let id = Uuid::new_v4();

    let (status, json) = delete_json(app, &format!("/api/dns/allowlist/{id}")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(json["message"].as_str().unwrap().contains("deleted"));
}

#[tokio::test]
async fn update_filter_rule_returns_ok() {
    let state = build_state(MockDnsService);
    let app = dns_router(state);
    let id = Uuid::new_v4();

    let body = serde_json::json!({"rule_text": "||updated.com^"});

    let (status, json) = put_json(app, &format!("/api/dns/rules/{id}"), &body.to_string()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["message"], "filter rule updated");
}

#[tokio::test]
async fn delete_filter_rule_returns_ok() {
    let state = build_state(MockDnsService);
    let app = dns_router(state);
    let id = Uuid::new_v4();

    let (status, json) = delete_json(app, &format!("/api/dns/rules/{id}")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(json["message"].as_str().unwrap().contains("deleted"));
}

// ---------------------------------------------------------------------------
// Toggle error-branch coverage
// ---------------------------------------------------------------------------

/// `DnsServer` that always returns error on start/stop — exercises the
/// error branches in the toggle handler.
struct FailingDnsServer;

#[async_trait]
impl wardnetd_services::dns::server::DnsServer for FailingDnsServer {
    async fn start(&self) -> anyhow::Result<()> {
        Err(anyhow::anyhow!("start failed"))
    }
    async fn stop(&self) -> anyhow::Result<()> {
        Err(anyhow::anyhow!("stop failed"))
    }
    fn is_running(&self) -> bool {
        false
    }
    async fn flush_cache(&self) -> u64 {
        0
    }
    async fn cache_size(&self) -> u64 {
        0
    }
    async fn cache_hit_rate(&self) -> f64 {
        0.0
    }
    async fn update_config(&self, _config: DnsConfig) {}
}

fn build_state_with_failing_server(dns_svc: impl DnsService + 'static) -> AppState {
    AppState::new(
        Arc::new(MockAuthService),
        Arc::new(StubDeviceService),
        Arc::new(StubDhcpService),
        Arc::new(dns_svc),
        Arc::new(StubDiscoveryService),
        Arc::new(StubLogService) as Arc<dyn LogService>,
        Arc::new(StubProviderService),
        Arc::new(StubRoutingService),
        Arc::new(StubSystemService),
        Arc::new(StubTunnelService),
        Arc::new(StubDhcpServer),
        Arc::new(FailingDnsServer),
        Arc::new(StubEventPublisher),
    )
}

#[tokio::test]
async fn toggle_enable_logs_error_when_server_start_fails() {
    // The handler should return 200 OK even if the underlying server
    // fails to start — the error is logged but not surfaced to the caller.
    let state = build_state_with_failing_server(MockDnsService);
    let app = dns_router(state);

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/dns/config/toggle")
                .header("Cookie", "wardnet_session=valid-token")
                .header("Content-Type", "application/json")
                .extension(connect_info())
                .body(Body::from(r#"{"enabled": true}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn toggle_disable_logs_error_when_server_stop_fails() {
    let state = build_state_with_failing_server(MockDnsService);
    let app = dns_router(state);

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/dns/config/toggle")
                .header("Cookie", "wardnet_session=valid-token")
                .header("Content-Type", "application/json")
                .extension(connect_info())
                .body(Body::from(r#"{"enabled": false}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
}
