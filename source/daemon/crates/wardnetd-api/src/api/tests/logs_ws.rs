//! Tests for the `logs_ws` WebSocket handler (`GET /api/system/logs/stream`).

use std::sync::Arc;

use async_trait::async_trait;
use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::get;
use tower::ServiceExt;
use uuid::Uuid;

use wardnetd_services::AuthService;
use wardnetd_services::LogService;
use wardnetd_services::auth::service::LoginResult;
use wardnetd_services::error::AppError;

use crate::state::AppState;
use crate::tests::stubs::{
    StubBackupService, StubDeviceService, StubDhcpServer, StubDhcpService, StubDiscoveryService,
    StubDnsFilterService, StubDnsLocalService, StubDnsServer, StubDnsService, StubEventPublisher,
    StubJobService, StubLogService, StubProviderService, StubRoutingService,
    StubRuleRequestService, StubStatsService, StubSystemService, StubTunnelService,
    StubUpdateService,
};

// ---------------------------------------------------------------------------
// Mock auth that approves every token
// ---------------------------------------------------------------------------

struct AlwaysAuthService;

#[async_trait]
impl AuthService for AlwaysAuthService {
    async fn login(&self, _u: &str, _p: &str, _remember_me: bool) -> Result<LoginResult, AppError> {
        unimplemented!()
    }
    async fn validate_session(&self, _token: &str) -> Result<Option<Uuid>, AppError> {
        Ok(Some(Uuid::new_v4()))
    }
    async fn validate_api_key(&self, _key: &str) -> Result<Option<Uuid>, AppError> {
        Ok(Some(Uuid::new_v4()))
    }
    async fn setup_admin(&self, _u: &str, _p: &str) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn is_setup_completed(&self) -> Result<bool, AppError> {
        unimplemented!()
    }
    async fn wizard_state(
        &self,
    ) -> Result<wardnetd_services::auth::service::WizardState, AppError> {
        unimplemented!()
    }
    async fn advance_wizard(
        &self,
        _to_step: wardnet_common::api::WizardStep,
        _mode: Option<wardnet_common::api::WizardMode>,
    ) -> Result<wardnetd_services::auth::service::WizardState, AppError> {
        unimplemented!()
    }
    async fn refresh_session(&self, _token: &str) -> Result<LoginResult, AppError> {
        unimplemented!()
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn logs_ws_app(state: AppState) -> Router {
    Router::new()
        .route("/api/system/logs/stream", get(crate::api::logs_ws::logs_ws))
        .with_state(state)
}

fn make_state() -> AppState {
    AppState::new(
        Arc::new(AlwaysAuthService),
        Arc::new(StubBackupService),
        Arc::new(StubDeviceService),
        Arc::new(StubDhcpService),
        Arc::new(StubDnsService),
        Arc::new(StubDnsFilterService),
        Arc::new(StubDnsLocalService),
        Arc::new(crate::tests::stubs::StubDdnsService),
        Arc::new(crate::tests::stubs::StubTlsService),
        Arc::new(StubDiscoveryService),
        Arc::new(StubLogService) as Arc<dyn LogService>,
        Arc::new(StubProviderService),
        Arc::new(StubRoutingService),
        Arc::new(StubSystemService),
        Arc::new(StubTunnelService),
        Arc::new(StubUpdateService),
        Arc::new(StubDhcpServer),
        Arc::new(StubDnsServer),
        Arc::new(StubEventPublisher),
        StubJobService::new_arc(),
        Arc::new(StubStatsService),
        Arc::new(StubRuleRequestService),
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// An authenticated WS upgrade request must reach the handler body and
/// get `101 Switching Protocols`.
///
/// This cannot use `tower::ServiceExt::oneshot` because axum's
/// `WebSocketUpgrade` extractor requires hyper's `OnUpgrade` extension,
/// which is only present on a real TCP connection. We bind a listener,
/// spawn axum's server, and send the raw HTTP handshake so that every
/// line of the handler body is reachable by the coverage tool.
#[tokio::test]
async fn logs_ws_upgrade_returns_101_when_authenticated() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind test listener");
    let addr = listener.local_addr().expect("local addr");

    tokio::spawn(async move {
        let _ = axum::serve(listener, logs_ws_app(make_state())).await;
    });

    let mut stream = tokio::net::TcpStream::connect(addr)
        .await
        .expect("connect to test server");

    let req = format!(
        "GET /api/system/logs/stream HTTP/1.1\r\n\
         Host: 127.0.0.1:{port}\r\n\
         Authorization: Bearer test-token\r\n\
         Connection: Upgrade\r\n\
         Upgrade: websocket\r\n\
         Sec-WebSocket-Version: 13\r\n\
         Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\
         \r\n",
        port = addr.port(),
    );

    stream
        .write_all(req.as_bytes())
        .await
        .expect("write request");

    let mut buf = vec![0u8; 512];
    let n = stream.read(&mut buf).await.expect("read response");
    let response = std::str::from_utf8(&buf[..n]).expect("utf-8 response");

    assert!(
        response.starts_with("HTTP/1.1 101"),
        "expected 101 Switching Protocols, got: {}",
        response.lines().next().unwrap_or(response)
    );
}

/// Without credentials the extractor must reject before the upgrade happens.
/// Kept here alongside the positive test so both auth paths are visible
/// together (the router-level test in `router.rs` provides the same
/// assertion from a full-router angle).
#[tokio::test]
async fn logs_ws_upgrade_returns_401_without_credentials() {
    // StubAuthService always returns Ok(None) — no session/key is valid.
    let app = logs_ws_app(crate::tests::stubs::test_app_state());

    let req = Request::builder()
        .method("GET")
        .uri("/api/system/logs/stream")
        .header("Connection", "Upgrade")
        .header("Upgrade", "websocket")
        .header("Sec-WebSocket-Version", "13")
        .header("Sec-WebSocket-Key", "dGhlIHNhbXBsZSBub25jZQ==")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}
