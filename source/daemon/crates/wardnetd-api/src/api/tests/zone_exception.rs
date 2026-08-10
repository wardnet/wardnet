//! Tests for the cross-zone exception API endpoints
//! (`/api/network/zones/exceptions` CRUD). Drives the real handlers through an
//! axum router with a canned `ZoneExceptionService`, asserting status codes and
//! the response envelopes.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use async_trait::async_trait;
use axum::Router;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use axum::routing::get;
use tower::ServiceExt;
use wardnet_common::api::{CreateZoneExceptionRequest, UpdateZoneExceptionRequest};
use wardnet_common::zone_exception::{
    ExceptionEndpoint, ExceptionEndpointKind, ServiceSet, ServiceSpec, ZoneException,
};

use crate::state::AppState;
use crate::tests::stubs::{
    StubBackupService, StubDdnsService, StubDeviceService, StubDhcpServer, StubDhcpService,
    StubDiscoveryService, StubDnsFilterService, StubDnsLocalService, StubDnsServer, StubDnsService,
    StubEventPublisher, StubJobService, StubLogService, StubNetworkZoneService,
    StubProviderService, StubRoutingService, StubRuleRequestService, StubStatsService,
    StubSystemService, StubTlsService, StubTunnelService, StubUpdateService,
};
use wardnetd_services::auth::service::LoginResult;
use wardnetd_services::error::AppError;
use wardnetd_services::{AuthService, LogService, ZoneExceptionService};
use wardnetd_services::auth::{CurrentUser, LoginAttempt};
use wardnet_common::auth::{AuthenticatedUser, UserRole};
use wardnet_test_support::principal;
use uuid::Uuid;

const IOT: &str = "00000000-0000-0000-0000-000000000202";

/// Mock `AuthService` that always resolves a valid admin session.
struct MockAuthService;

#[async_trait]
impl AuthService for MockAuthService {
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
    async fn validate_session(&self, _token: &str) -> Result<Option<AuthenticatedUser>, AppError> {
        Ok(Some(principal::admin(Uuid::nil())))
    }
    async fn validate_api_key(&self, _key: &str) -> Result<Option<AuthenticatedUser>, AppError> {
        Ok(None)
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
    async fn logout_session(&self, _token: &str) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn refresh_session(&self, _token: &str) -> Result<LoginResult, AppError> {
        unimplemented!()
    }
    async fn cleanup_expired_sessions(&self) -> Result<u64, AppError> {
        unimplemented!()
    }
}

fn sample_exception() -> ZoneException {
    let now = "2026-07-02T00:00:00Z".parse().unwrap();
    ZoneException {
        id: Uuid::nil(),
        from: ExceptionEndpoint {
            kind: ExceptionEndpointKind::Device,
            id: Uuid::nil(),
        },
        to: ExceptionEndpoint {
            kind: ExceptionEndpointKind::Zone,
            id: Uuid::parse_str(IOT).unwrap(),
        },
        service: ServiceSpec::Preset {
            set: ServiceSet::Casting,
        },
        bidirectional: true,
        created_at: now,
        updated_at: now,
    }
}

/// Canned `ZoneExceptionService` — returns fixed data so the handlers'
/// request/response plumbing is exercised end to end.
struct MockZoneExceptionService;

#[async_trait]
impl ZoneExceptionService for MockZoneExceptionService {
    async fn list_exceptions(&self) -> Result<Vec<ZoneException>, AppError> {
        Ok(vec![sample_exception()])
    }
    async fn get_exception(&self, _id: Uuid) -> Result<ZoneException, AppError> {
        Ok(sample_exception())
    }
    async fn create_exception(
        &self,
        _req: CreateZoneExceptionRequest,
    ) -> Result<ZoneException, AppError> {
        Ok(sample_exception())
    }
    async fn update_exception(
        &self,
        _id: Uuid,
        _req: UpdateZoneExceptionRequest,
    ) -> Result<ZoneException, AppError> {
        Ok(sample_exception())
    }
    async fn delete_exception(&self, _id: Uuid) -> Result<(), AppError> {
        Ok(())
    }
}

fn connect_info() -> ConnectInfo<SocketAddr> {
    ConnectInfo(SocketAddr::new(
        IpAddr::V4(Ipv4Addr::new(192, 168, 1, 10)),
        12345,
    ))
}

fn build_state() -> AppState {
    AppState::new(
        Arc::new(MockAuthService),
        Arc::new(StubBackupService),
        Arc::new(StubDeviceService),
        Arc::new(StubDhcpService),
        Arc::new(StubDnsService),
        Arc::new(StubDnsFilterService),
        Arc::new(StubDnsLocalService),
        Arc::new(StubDdnsService),
        Arc::new(StubTlsService),
        Arc::new(StubDiscoveryService),
        Arc::new(StubLogService) as Arc<dyn LogService>,
        Arc::new(StubProviderService),
        Arc::new(StubRoutingService),
        Arc::new(StubNetworkZoneService),
        Arc::new(StubSystemService),
        Arc::new(StubTunnelService),
        Arc::new(StubUpdateService),
        Arc::new(StubDhcpServer),
        Arc::new(StubDnsServer),
        Arc::new(StubEventPublisher),
        StubJobService::new_arc(),
        Arc::new(StubStatsService),
        Arc::new(StubRuleRequestService),
        Arc::new(MockZoneExceptionService),
    )
}

fn exception_router() -> Router {
    Router::new()
        .route(
            "/api/network/zones/exceptions",
            get(crate::api::zone_exception::list_exceptions)
                .post(crate::api::zone_exception::create_exception),
        )
        .route(
            "/api/network/zones/exceptions/{id}",
            get(crate::api::zone_exception::get_exception)
                .put(crate::api::zone_exception::update_exception)
                .delete(crate::api::zone_exception::delete_exception),
        )
        .with_state(build_state())
}

async fn send(method: &str, uri: &str, body: Option<&str>) -> (StatusCode, serde_json::Value) {
    let req = Request::builder()
        .method(method)
        .uri(uri)
        .header("Cookie", "wardnet_session=valid-token")
        .header("Content-Type", "application/json")
        .extension(connect_info())
        .body(body.map_or_else(Body::empty, |b| Body::from(b.to_owned())))
        .unwrap();
    let resp = exception_router().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), 32768).await.unwrap();
    let json = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
    (status, json)
}

#[tokio::test]
async fn list_exceptions_returns_array() {
    let (status, json) = send("GET", "/api/network/zones/exceptions", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["exceptions"][0]["bidirectional"], true);
    assert_eq!(json["exceptions"][0]["service"]["type"], "preset");
    assert_eq!(json["exceptions"][0]["service"]["set"], "casting");
}

#[tokio::test]
async fn create_exception_returns_201() {
    let body = r#"{"from":{"kind":"device","id":"00000000-0000-0000-0000-000000000000"},"to":{"kind":"zone","id":"00000000-0000-0000-0000-000000000202"},"service":{"type":"preset","set":"casting"},"bidirectional":true}"#;
    let (status, json) = send("POST", "/api/network/zones/exceptions", Some(body)).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(json["exception"]["bidirectional"], true);
}

#[tokio::test]
async fn get_exception_returns_exception() {
    let id = Uuid::nil();
    let (status, json) = send("GET", &format!("/api/network/zones/exceptions/{id}"), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["exception"]["to"]["kind"], "zone");
}

#[tokio::test]
async fn update_exception_returns_updated() {
    let id = Uuid::nil();
    let body = r#"{"bidirectional":false}"#;
    let (status, json) = send(
        "PUT",
        &format!("/api/network/zones/exceptions/{id}"),
        Some(body),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(json["exception"].is_object());
}

#[tokio::test]
async fn delete_exception_returns_deleted_true() {
    let id = Uuid::nil();
    let (status, json) = send(
        "DELETE",
        &format!("/api/network/zones/exceptions/{id}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["deleted"], true);
}

// ── Auth gate ──────────────────────────────────────────────────────────────
// The shared `send()` helper always attaches a session cookie, so the tests
// above cannot see the `SessionAuth` guard. These drive each handler with no
// credentials: a handler that lost its `_auth: SessionAuth` extractor would
// answer 200/201 here instead of 401.

/// Same as `send`, but omits the session cookie so the request is anonymous.
async fn send_unauthenticated(method: &str, uri: &str, body: Option<&str>) -> StatusCode {
    let req = Request::builder()
        .method(method)
        .uri(uri)
        .header("Content-Type", "application/json")
        .extension(connect_info())
        .body(body.map_or_else(Body::empty, |b| Body::from(b.to_owned())))
        .unwrap();
    exception_router().oneshot(req).await.unwrap().status()
}

#[tokio::test]
async fn list_exceptions_rejects_unauthenticated() {
    assert_eq!(
        send_unauthenticated("GET", "/api/network/zones/exceptions", None).await,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn create_exception_rejects_unauthenticated() {
    let body = r#"{"from":{"kind":"device","id":"00000000-0000-0000-0000-000000000000"},"to":{"kind":"zone","id":"00000000-0000-0000-0000-000000000202"},"service":{"type":"preset","set":"casting"},"bidirectional":true}"#;
    assert_eq!(
        send_unauthenticated("POST", "/api/network/zones/exceptions", Some(body)).await,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn get_exception_rejects_unauthenticated() {
    let id = Uuid::nil();
    assert_eq!(
        send_unauthenticated("GET", &format!("/api/network/zones/exceptions/{id}"), None).await,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn update_exception_rejects_unauthenticated() {
    let id = Uuid::nil();
    assert_eq!(
        send_unauthenticated(
            "PUT",
            &format!("/api/network/zones/exceptions/{id}"),
            Some(r#"{"bidirectional":false}"#),
        )
        .await,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn delete_exception_rejects_unauthenticated() {
    let id = Uuid::nil();
    assert_eq!(
        send_unauthenticated(
            "DELETE",
            &format!("/api/network/zones/exceptions/{id}"),
            None
        )
        .await,
        StatusCode::UNAUTHORIZED
    );
}
