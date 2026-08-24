//! Tests for the device access-request API endpoints.
//! POST/GET /api/devices/me/access-requests, GET/PATCH /api/access-requests.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use async_trait::async_trait;
use axum::Router;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use axum::routing::{get, patch};
use tower::ServiceExt;
use wardnet_common::access_request::{
    AccessRequestKind, AccessRequestStatus, ApprovalParams, DeviceAccessRequest,
};

use crate::state::AppState;
use crate::tests::stubs::{
    StubDhcpServer, StubDhcpService, StubDiscoveryService, StubDnsFilterService, StubDnsServer,
    StubDnsService, StubEventPublisher, StubLogService, StubNetworkZoneService,
    StubProviderService, StubRoutingService, StubSystemService, StubTunnelService,
    StubZoneExceptionService,
};
use uuid::Uuid;
use wardnet_common::auth::{AuthenticatedUser, UserRole};
use wardnet_test_support::principal;
use wardnetd_services::AccessRequestService;
use wardnetd_services::LogService;
use wardnetd_services::auth::service::LoginResult;
use wardnetd_services::auth::{CurrentUser, LoginAttempt};
use wardnetd_services::error::AppError;

// --- MockAuthService — validates any session as admin -----------------------

struct MockAuthService;

#[async_trait]
impl wardnetd_services::AuthService for MockAuthService {
    async fn current_user(&self) -> Result<CurrentUser, AppError> {
        Ok(CurrentUser {
            user_id: Uuid::nil(),
            display_name: "admin".to_owned(),
            email: None,
            role: UserRole::Admin,
        })
    }
    async fn login(&self, _attempt: LoginAttempt<'_>) -> Result<LoginResult, AppError> {
        unimplemented!()
    }
    async fn cleanup_expired_sessions(&self) -> Result<u64, AppError> {
        unimplemented!()
    }
    async fn validate_session(&self, _token: &str) -> Result<Option<AuthenticatedUser>, AppError> {
        Ok(Some(principal::admin(
            Uuid::parse_str("00000000-0000-0000-0000-000000000099").unwrap(),
        )))
    }
    async fn validate_api_key(&self, _key: &str) -> Result<Option<AuthenticatedUser>, AppError> {
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
    async fn logout_session(&self, _token: &str) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn refresh_session(&self, _token: &str) -> Result<LoginResult, AppError> {
        unimplemented!()
    }
}

// --- MockAccessRequestService -------------------------------------------------

fn sample_request() -> DeviceAccessRequest {
    DeviceAccessRequest {
        id: "req-1".to_owned(),
        device_id: "00000000-0000-0000-0000-000000000001".to_owned(),
        kind: AccessRequestKind::Block,
        domain: Some("ads.example.com".to_owned()),
        reason: Some("too many ads".to_owned()),
        status: AccessRequestStatus::Pending,
        created_at: "2026-06-18T00:00:00Z".to_owned(),
        decided_at: None,
        decided_by: None,
    }
}

#[derive(Default)]
struct MockAccessRequestService {
    decide_not_found: bool,
}

#[async_trait]
impl AccessRequestService for MockAccessRequestService {
    async fn create_for_ip(
        &self,
        _ip: &str,
        kind: AccessRequestKind,
        domain: Option<String>,
        reason: Option<String>,
    ) -> Result<DeviceAccessRequest, AppError> {
        Ok(DeviceAccessRequest {
            kind,
            domain,
            reason,
            ..sample_request()
        })
    }
    async fn list_for_ip(&self, _ip: &str) -> Result<Vec<DeviceAccessRequest>, AppError> {
        Ok(vec![sample_request()])
    }
    async fn list(
        &self,
        _status: Option<AccessRequestStatus>,
    ) -> Result<Vec<DeviceAccessRequest>, AppError> {
        Ok(vec![sample_request()])
    }
    async fn decide(
        &self,
        _id: &str,
        status: AccessRequestStatus,
        _params: Option<ApprovalParams>,
    ) -> Result<DeviceAccessRequest, AppError> {
        if self.decide_not_found {
            return Err(AppError::NotFound("access request not found".to_owned()));
        }
        Ok(DeviceAccessRequest {
            status,
            ..sample_request()
        })
    }
    async fn resolve_pending(
        &self,
        _device_id: Uuid,
        _kind: AccessRequestKind,
        _status: AccessRequestStatus,
        _decided_by: Option<String>,
    ) -> Result<Option<DeviceAccessRequest>, AppError> {
        unimplemented!("the listener is exercised in the services crate")
    }
}

// --- Helpers ----------------------------------------------------------------

fn connect_info() -> ConnectInfo<SocketAddr> {
    ConnectInfo(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 12345))
}

fn build_state(rule_svc: impl AccessRequestService + 'static) -> AppState {
    AppState::new(
        Arc::new(MockAuthService),
        Arc::new(crate::tests::stubs::StubBackupService),
        Arc::new(crate::tests::stubs::StubDeviceService),
        Arc::new(StubDhcpService),
        Arc::new(StubDnsService),
        Arc::new(StubDnsFilterService),
        Arc::new(crate::tests::stubs::StubDnsLocalService),
        Arc::new(crate::tests::stubs::StubDdnsService),
        Arc::new(crate::tests::stubs::StubTlsService),
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
        Arc::new(rule_svc),
        Arc::new(StubZoneExceptionService),
    )
}

fn router(state: AppState) -> Router {
    use crate::api::access_requests::*;

    Router::new()
        .route(
            "/api/devices/me/access-requests",
            get(list_my_access_requests).post(create_my_access_request),
        )
        .route("/api/access-requests", get(list_access_requests))
        .route("/api/access-requests/{id}", patch(decide_access_request))
        .with_state(state)
}

async fn send(
    app: Router,
    method: &str,
    uri: &str,
    body: Option<&str>,
    cookie: bool,
) -> (StatusCode, serde_json::Value) {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .extension(connect_info());
    if cookie {
        builder = builder.header("Cookie", "wardnet_session=valid-token");
    }
    let req = if let Some(b) = body {
        builder
            .header("Content-Type", "application/json")
            .body(Body::from(b.to_owned()))
            .unwrap()
    } else {
        builder.body(Body::empty()).unwrap()
    };
    let resp = app.oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), 16384).await.unwrap();
    let json = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
    (status, json)
}

// --- Tests ------------------------------------------------------------------

#[tokio::test]
async fn create_my_request_returns_201_and_echoes_fields() {
    let app = router(build_state(MockAccessRequestService {
        decide_not_found: false,
    }));
    let (status, json) = send(
        app,
        "POST",
        "/api/devices/me/access-requests",
        Some(r#"{"kind":"allow","domain":"good.example.com","reason":"need it"}"#),
        false,
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(json["kind"], "allow");
    assert_eq!(json["domain"], "good.example.com");
    assert_eq!(json["reason"], "need it");
    assert_eq!(json["status"], "pending");
}

/// A Private-DNS request carries no domain at all — the wire shape has to
/// accept the field being absent rather than requiring an empty string.
#[tokio::test]
async fn create_my_private_dns_request_needs_no_domain() {
    let app = router(build_state(MockAccessRequestService::default()));
    let (status, json) = send(
        app,
        "POST",
        "/api/devices/me/access-requests",
        Some(r#"{"kind":"private_dns"}"#),
        false,
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(json["kind"], "private_dns");
    assert!(json["domain"].is_null());
}

#[tokio::test]
async fn list_my_requests_returns_200() {
    let app = router(build_state(MockAccessRequestService {
        decide_not_found: false,
    }));
    let (status, json) = send(app, "GET", "/api/devices/me/access-requests", None, false).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json[0]["domain"], "ads.example.com");
}

#[tokio::test]
async fn admin_list_returns_200_with_cookie() {
    let app = router(build_state(MockAccessRequestService {
        decide_not_found: false,
    }));
    let (status, json) = send(
        app,
        "GET",
        "/api/access-requests?status=pending",
        None,
        true,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json[0]["id"], "req-1");
}

#[tokio::test]
async fn admin_list_without_cookie_returns_401() {
    let app = router(build_state(MockAccessRequestService {
        decide_not_found: false,
    }));
    let (status, _json) = send(app, "GET", "/api/access-requests", None, false).await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn admin_decide_returns_200() {
    let app = router(build_state(MockAccessRequestService {
        decide_not_found: false,
    }));
    let (status, json) = send(
        app,
        "PATCH",
        "/api/access-requests/req-1",
        Some(r#"{"status":"approved"}"#),
        true,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["status"], "approved");
}

#[tokio::test]
async fn admin_decide_unknown_returns_404() {
    let app = router(build_state(MockAccessRequestService {
        decide_not_found: true,
    }));
    let (status, _json) = send(
        app,
        "PATCH",
        "/api/access-requests/does-not-exist",
        Some(r#"{"status":"rejected"}"#),
        true,
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
}
