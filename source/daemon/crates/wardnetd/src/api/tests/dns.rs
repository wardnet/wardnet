//! Tests for the DNS API endpoints.
//!
//! Verifies that routes are wired correctly, that the admin-auth guard
//! rejects unauthenticated requests, and that responses have the expected
//! shape.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use axum::Router;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use axum::routing::{get, post};
use tower::ServiceExt;
use uuid::Uuid;
use wardnet_types::api::{
    DnsCacheFlushResponse, DnsConfigResponse, DnsStatusResponse, ToggleDnsRequest,
    UpdateDnsConfigRequest,
};
use wardnet_types::dns::{DnsConfig, DnsResolutionMode};

use crate::config::Config;
use crate::error::AppError;
use crate::service::auth::LoginResult;
use crate::service::{AuthService, DnsService};
use crate::state::AppState;
use crate::tests::stubs::{
    StubDeviceService, StubDhcpService, StubDiscoveryService, StubEventPublisher,
    StubProviderService, StubRoutingService, StubSystemService, StubTunnelService,
};

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
        Arc::new(StubProviderService),
        Arc::new(StubRoutingService),
        Arc::new(StubSystemService),
        Arc::new(StubTunnelService),
        Arc::new(crate::dhcp::server::NoopDhcpServer),
        Arc::new(crate::dns::server::NoopDnsServer),
        Arc::new(StubEventPublisher),
        crate::log_broadcast::LogBroadcaster::new(16),
        crate::recent_errors::RecentErrors::new(),
        Config::default(),
        Instant::now(),
    )
}

fn dns_router(state: AppState) -> Router {
    Router::new()
        .route(
            "/api/dns/config",
            get(crate::api::dns::get_config).put(crate::api::dns::update_config),
        )
        .route("/api/dns/config/toggle", post(crate::api::dns::toggle))
        .route("/api/dns/status", get(crate::api::dns::status))
        .route("/api/dns/cache/flush", post(crate::api::dns::flush_cache))
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

// ---------------------------------------------------------------------------
// Toggle error-branch coverage
// ---------------------------------------------------------------------------

/// `DnsServer` that always returns error on start/stop — exercises the
/// error branches in the toggle handler.
struct FailingDnsServer;

#[async_trait]
impl crate::dns::server::DnsServer for FailingDnsServer {
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
        Arc::new(StubProviderService),
        Arc::new(StubRoutingService),
        Arc::new(StubSystemService),
        Arc::new(StubTunnelService),
        Arc::new(crate::dhcp::server::NoopDhcpServer),
        Arc::new(FailingDnsServer),
        Arc::new(StubEventPublisher),
        crate::log_broadcast::LogBroadcaster::new(16),
        crate::recent_errors::RecentErrors::new(),
        Config::default(),
        Instant::now(),
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
