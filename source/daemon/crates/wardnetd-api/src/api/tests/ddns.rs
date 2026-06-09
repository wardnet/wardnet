//! Tests for the remote-access API endpoints (`/api/ddns/*`, `/api/tls/status`).
//!
//! The background provisioning that `POST /api/ddns/register` kicks off is not
//! exercised here (it spawns a detached task); these cover the synchronous,
//! admin-gated read/check surface the wizard polls.

use std::net::Ipv4Addr;
use std::sync::Arc;

use async_trait::async_trait;
use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::{delete, get, post};
use tower::ServiceExt;
use uuid::Uuid;
use wardnet_common::api::{
    DdnsCheckResponse, DdnsRegisterResponse, DdnsResolutionCheckResponse, DdnsResolutionVerdict,
    DdnsStatusResponse, TlsProvisioningPhase, TlsStatusResponse,
};

use crate::state::AppState;
use crate::tests::stubs::{
    StubBackupService, StubDeviceService, StubDhcpServer, StubDhcpService, StubDiscoveryService,
    StubDnsFilterService, StubDnsLocalService, StubDnsServer, StubDnsService, StubEventPublisher,
    StubLogService, StubProviderService, StubRoutingService, StubSystemService, StubTunnelService,
};
use wardnetd_services::auth::service::LoginResult;
use wardnetd_services::ddns::{DdnsRegistration, DdnsService, DdnsStatus};
use wardnetd_services::error::AppError;
use wardnetd_services::tls::{TlsService, TlsStatus};
use wardnetd_services::{AuthService, LogService};

// ── Mocks ───────────────────────────────────────────────────────────────────

/// Authenticates any `wardnet_session` cookie to a stable admin id.
struct AlwaysAdminAuth;
#[async_trait]
impl AuthService for AlwaysAdminAuth {
    async fn login(&self, _u: &str, _p: &str, _r: bool) -> Result<LoginResult, AppError> {
        unimplemented!()
    }
    async fn validate_session(&self, _token: &str) -> Result<Option<Uuid>, AppError> {
        Ok(Some(Uuid::nil()))
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

/// DDNS mock returning canned availability + status.
struct MockDdns {
    available: bool,
}
#[async_trait]
impl DdnsService for MockDdns {
    async fn register_with_bridge(&self, name: String) -> Result<DdnsRegistration, AppError> {
        Ok(DdnsRegistration {
            subdomain: format!("{name}.my.wardnet.services"),
            region: "us".to_owned(),
        })
    }
    async fn check_name_available(&self, _name: String) -> Result<bool, AppError> {
        Ok(self.available)
    }
    async fn configure_cloudflare(
        &self,
        _token: String,
        _domain: String,
    ) -> Result<DdnsRegistration, AppError> {
        unimplemented!()
    }
    // Returns `Ok` (not `unimplemented!`) so the detached provisioning task the
    // register handler spawns can't panic if it races the test's teardown.
    async fn refresh_public_ip(&self) -> Result<Option<Ipv4Addr>, AppError> {
        Ok(None)
    }
    async fn status(&self) -> Result<DdnsStatus, AppError> {
        Ok(DdnsStatus {
            provider: Some("bridge".to_owned()),
            fqdn: Some("happy-einstein.my.wardnet.services".to_owned()),
            last_public_ip: Some("9.9.9.9".to_owned()),
        })
    }
    async fn teardown(&self) -> Result<(), AppError> {
        Ok(())
    }
    async fn resolution_check(&self) -> Result<DdnsResolutionCheckResponse, AppError> {
        Ok(DdnsResolutionCheckResponse {
            verdict: DdnsResolutionVerdict::Match,
            fqdn: Some("happy-einstein.my.wardnet.services".to_owned()),
            expected_ip: Some("9.9.9.9".to_owned()),
            resolved_ips: vec!["9.9.9.9".to_owned()],
        })
    }
    async fn set_acme_challenge(&self, _values: &[String]) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn clear_acme_challenge(&self) -> Result<(), AppError> {
        unimplemented!()
    }
}

/// TLS mock returning a canned `issuing` provisioning status.
struct MockTls;
#[async_trait]
impl TlsService for MockTls {
    // `Ok` (not `unimplemented!`) so the detached provisioning task can't panic.
    async fn ensure_certificate(&self) -> Result<TlsStatus, AppError> {
        Ok(TlsStatus::NotConfigured)
    }
    async fn status(&self) -> Result<TlsStatus, AppError> {
        unimplemented!()
    }
    async fn mark_provisioning_started(&self) -> Result<(), AppError> {
        Ok(())
    }
    async fn provisioning_status(&self) -> Result<TlsStatusResponse, AppError> {
        Ok(TlsStatusResponse {
            phase: TlsProvisioningPhase::Issuing,
            domain: Some("happy-einstein.my.wardnet.services".to_owned()),
            not_after: None,
            error: None,
        })
    }
    async fn teardown(&self) -> Result<(), AppError> {
        Ok(())
    }
}

// ── Harness ───────────────────────────────────────────────────────────────────

fn make_state(ddns: Arc<dyn DdnsService>, tls: Arc<dyn TlsService>) -> AppState {
    AppState::new(
        Arc::new(AlwaysAdminAuth),
        Arc::new(StubBackupService),
        Arc::new(StubDeviceService),
        Arc::new(StubDhcpService),
        Arc::new(StubDnsService),
        Arc::new(StubDnsFilterService),
        Arc::new(StubDnsLocalService),
        ddns,
        tls,
        Arc::new(StubDiscoveryService),
        Arc::new(StubLogService) as Arc<dyn LogService>,
        Arc::new(StubProviderService),
        Arc::new(StubRoutingService),
        Arc::new(StubSystemService),
        Arc::new(StubTunnelService),
        Arc::new(crate::tests::stubs::StubUpdateService),
        Arc::new(StubDhcpServer),
        Arc::new(StubDnsServer),
        Arc::new(StubEventPublisher),
        crate::tests::stubs::StubJobService::new_arc(),
        Arc::new(crate::tests::stubs::StubStatsService),
    )
}

fn app(state: AppState) -> Router {
    Router::new()
        .route("/api/ddns/check", get(crate::api::ddns::ddns_check))
        .route("/api/ddns/register", post(crate::api::ddns::ddns_register))
        .route("/api/ddns/status", get(crate::api::ddns::ddns_status))
        .route(
            "/api/ddns/resolution-check",
            get(crate::api::ddns::ddns_resolution_check),
        )
        .route("/api/ddns", delete(crate::api::ddns::ddns_teardown))
        .route("/api/tls/status", get(crate::api::tls::tls_status))
        .with_state(state)
}

fn admin_get(uri: &str) -> Request<Body> {
    Request::builder()
        .method("GET")
        .uri(uri)
        .header("Cookie", "wardnet_session=test")
        .body(Body::empty())
        .unwrap()
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[tokio::test]
async fn ddns_check_returns_availability() {
    let state = make_state(Arc::new(MockDdns { available: true }), Arc::new(MockTls));
    let resp = app(state)
        .oneshot(admin_get("/api/ddns/check?name=happy-einstein"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
    let json: DdnsCheckResponse = serde_json::from_slice(&body).unwrap();
    assert!(json.available);
}

#[tokio::test]
async fn ddns_register_returns_assigned_fqdn() {
    // The synchronous half of register: persist identity + return the FQDN. The
    // detached provisioning task is fire-and-forget (mocks return Ok, so it can't
    // panic if it races teardown); we assert only the response the wizard reads.
    let state = make_state(Arc::new(MockDdns { available: true }), Arc::new(MockTls));
    let req = Request::builder()
        .method("POST")
        .uri("/api/ddns/register")
        .header("Cookie", "wardnet_session=test")
        .header("Content-Type", "application/json")
        .body(Body::from(
            serde_json::to_vec(&serde_json::json!({ "name": "happy-einstein" })).unwrap(),
        ))
        .unwrap();
    let resp = app(state).oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
    let json: DdnsRegisterResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(json.fqdn, "happy-einstein.my.wardnet.services");
    assert_eq!(json.region.as_deref(), Some("us"));
}

#[tokio::test]
async fn ddns_status_maps_service_status() {
    let state = make_state(Arc::new(MockDdns { available: false }), Arc::new(MockTls));
    let resp = app(state)
        .oneshot(admin_get("/api/ddns/status"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
    let json: DdnsStatusResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(json.provider.as_deref(), Some("bridge"));
    assert_eq!(
        json.fqdn.as_deref(),
        Some("happy-einstein.my.wardnet.services")
    );
}

#[tokio::test]
async fn tls_status_reports_phase() {
    let state = make_state(Arc::new(MockDdns { available: false }), Arc::new(MockTls));
    let resp = app(state)
        .oneshot(admin_get("/api/tls/status"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
    let json: TlsStatusResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(json.phase, TlsProvisioningPhase::Issuing);
    assert_eq!(
        json.domain.as_deref(),
        Some("happy-einstein.my.wardnet.services")
    );
}

#[tokio::test]
async fn tls_status_rejects_unauthenticated() {
    let state = make_state(Arc::new(MockDdns { available: false }), Arc::new(MockTls));
    let req = Request::builder()
        .method("GET")
        .uri("/api/tls/status")
        .body(Body::empty())
        .unwrap();
    let resp = app(state).oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn ddns_resolution_check_returns_verdict() {
    let state = make_state(Arc::new(MockDdns { available: true }), Arc::new(MockTls));
    let resp = app(state)
        .oneshot(admin_get("/api/ddns/resolution-check"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
    let json: DdnsResolutionCheckResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(json.verdict, DdnsResolutionVerdict::Match);
    assert_eq!(json.resolved_ips, vec!["9.9.9.9".to_owned()]);
}

#[tokio::test]
async fn ddns_teardown_returns_no_content() {
    let state = make_state(Arc::new(MockDdns { available: true }), Arc::new(MockTls));
    let req = Request::builder()
        .method("DELETE")
        .uri("/api/ddns")
        .header("Cookie", "wardnet_session=test")
        .body(Body::empty())
        .unwrap();
    let resp = app(state).oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
}
