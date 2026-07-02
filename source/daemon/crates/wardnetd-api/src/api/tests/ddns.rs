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
    StubLogService, StubNetworkZoneService, StubProviderService, StubRoutingService,
    StubSystemService, StubTunnelService,
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
    async fn cleanup_expired_sessions(&self) -> Result<u64, AppError> {
        unimplemented!()
    }
}

/// DDNS mock returning canned availability + status.
struct MockDdns {
    available: bool,
}
#[async_trait]
impl DdnsService for MockDdns {
    async fn request_enrollment_code(&self, _email: String) -> Result<(), AppError> {
        Ok(())
    }
    async fn enroll(&self, _code: String) -> Result<(), AppError> {
        Ok(())
    }
    async fn check_slug(&self, _slug: String) -> Result<bool, AppError> {
        Ok(self.available)
    }
    async fn register_network(
        &self,
        slug: String,
        _display_name: Option<String>,
    ) -> Result<DdnsRegistration, AppError> {
        Ok(DdnsRegistration {
            subdomain: format!("{slug}.my.wardnet.services"),
            region: "use1".to_owned(),
        })
    }
    async fn configure_cloudflare(
        &self,
        _token: String,
        domain: String,
    ) -> Result<DdnsRegistration, AppError> {
        // BYOD: the configured domain becomes the serving FQDN; no bridge region.
        Ok(DdnsRegistration {
            subdomain: domain,
            region: "us".to_owned(),
        })
    }
    // Returns `Ok` (not `unimplemented!`) so the detached provisioning task the
    // register handler spawns can't panic if it races the test's teardown.
    async fn refresh_public_ip(&self) -> Result<Option<Ipv4Addr>, AppError> {
        Ok(None)
    }
    async fn status(&self) -> Result<DdnsStatus, AppError> {
        Ok(DdnsStatus {
            provider: Some("wardnet".to_owned()),
            fqdn: Some("happy-einstein.my.wardnet.services".to_owned()),
            last_public_ip: Some("9.9.9.9".to_owned()),
            suspended: false,
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
        Arc::new(StubNetworkZoneService),
        Arc::new(StubSystemService),
        Arc::new(StubTunnelService),
        Arc::new(crate::tests::stubs::StubUpdateService),
        Arc::new(StubDhcpServer),
        Arc::new(StubDnsServer),
        Arc::new(StubEventPublisher),
        crate::tests::stubs::StubJobService::new_arc(),
        Arc::new(crate::tests::stubs::StubStatsService),
        Arc::new(crate::tests::stubs::StubRuleRequestService),
        Arc::new(crate::tests::stubs::StubZoneExceptionService),
    )
}

fn app(state: AppState) -> Router {
    Router::new()
        .route(
            "/api/ddns/enrollment-code",
            post(crate::api::ddns::ddns_enrollment_code),
        )
        .route("/api/ddns/enroll", post(crate::api::ddns::ddns_enroll))
        .route("/api/ddns/check", get(crate::api::ddns::ddns_check))
        .route("/api/ddns/register", post(crate::api::ddns::ddns_register))
        .route(
            "/api/ddns/cloudflare",
            post(crate::api::ddns::ddns_cloudflare),
        )
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
        .oneshot(admin_get("/api/ddns/check?slug=happy-einstein"))
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
            serde_json::to_vec(&serde_json::json!({ "slug": "happy-einstein" })).unwrap(),
        ))
        .unwrap();
    let resp = app(state).oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
    let json: DdnsRegisterResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(json.fqdn, "happy-einstein.my.wardnet.services");
    assert_eq!(json.region.as_deref(), Some("use1"));
}

#[tokio::test]
async fn ddns_enrollment_code_returns_no_content() {
    let state = make_state(Arc::new(MockDdns { available: true }), Arc::new(MockTls));
    let req = Request::builder()
        .method("POST")
        .uri("/api/ddns/enrollment-code")
        .header("Cookie", "wardnet_session=test")
        .header("Content-Type", "application/json")
        .body(Body::from(
            serde_json::to_vec(&serde_json::json!({ "email": "a@b.com" })).unwrap(),
        ))
        .unwrap();
    let resp = app(state).oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn ddns_enroll_returns_no_content() {
    let state = make_state(Arc::new(MockDdns { available: true }), Arc::new(MockTls));
    let req = Request::builder()
        .method("POST")
        .uri("/api/ddns/enroll")
        .header("Cookie", "wardnet_session=test")
        .header("Content-Type", "application/json")
        .body(Body::from(
            serde_json::to_vec(&serde_json::json!({ "code": "ABC123" })).unwrap(),
        ))
        .unwrap();
    let resp = app(state).oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
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
    assert_eq!(json.provider.as_deref(), Some("wardnet"));
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

#[tokio::test]
async fn ddns_cloudflare_returns_configured_domain() {
    // BYOD-Cloudflare: the handler returns the configured domain as the FQDN
    // with no bridge region, and kicks off provisioning (mocks return Ok).
    let state = make_state(Arc::new(MockDdns { available: true }), Arc::new(MockTls));
    let req = Request::builder()
        .method("POST")
        .uri("/api/ddns/cloudflare")
        .header("Cookie", "wardnet_session=test")
        .header("Content-Type", "application/json")
        .body(Body::from(
            serde_json::to_vec(&serde_json::json!({
                "token": "cf-token",
                "domain": "home.example.com",
            }))
            .unwrap(),
        ))
        .unwrap();
    let resp = app(state).oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
    let json: DdnsRegisterResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(json.fqdn, "home.example.com");
    assert_eq!(json.region, None);
}
