//! Tests for the Network Zone API endpoints
//! (`/api/network/zones` CRUD). Drives the real handlers through an axum
//! router with a canned `NetworkZoneService`, asserting status codes and the
//! response envelopes.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use async_trait::async_trait;
use axum::Router;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use axum::routing::get;
use tower::ServiceExt;
use uuid::Uuid;
use wardnet_common::api::{CreateNetworkZoneRequest, NetworkZoneView, UpdateNetworkZoneRequest};
use wardnet_common::network_zone::{AllowedTargetKind, NetworkZone, ZoneProvenance, ZoneStance};

use crate::state::AppState;
use crate::tests::stubs::{
    StubBackupService, StubDdnsService, StubDeviceService, StubDhcpServer, StubDhcpService,
    StubDiscoveryService, StubDnsFilterService, StubDnsLocalService, StubDnsServer, StubDnsService,
    StubEventPublisher, StubJobService, StubLogService, StubProviderService, StubRoutingService,
    StubRuleRequestService, StubStatsService, StubSystemService, StubTlsService, StubTunnelService,
    StubUpdateService,
};
use wardnetd_services::auth::service::LoginResult;
use wardnetd_services::error::AppError;
use wardnetd_services::{AuthService, LogService, NetworkZoneService};

const TRUSTED: &str = "00000000-0000-0000-0000-000000000201";

/// Mock `AuthService` that always resolves a valid admin session.
struct MockAuthService;

#[async_trait]
impl AuthService for MockAuthService {
    async fn login(&self, _u: &str, _p: &str, _remember_me: bool) -> Result<LoginResult, AppError> {
        unimplemented!()
    }
    async fn validate_session(&self, _token: &str) -> Result<Option<Uuid>, AppError> {
        Ok(Some(Uuid::parse_str(TRUSTED).unwrap()))
    }
    async fn validate_api_key(&self, _key: &str) -> Result<Option<Uuid>, AppError> {
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
    async fn refresh_session(&self, _token: &str) -> Result<LoginResult, AppError> {
        unimplemented!()
    }
}

fn sample_zone() -> NetworkZone {
    let now = "2026-07-01T00:00:00Z".parse().unwrap();
    NetworkZone {
        id: Uuid::parse_str(TRUSTED).unwrap(),
        name: "Trusted".to_owned(),
        provenance: ZoneProvenance::System,
        isolation_stance: ZoneStance::SharedSubnet,
        allowed_targets: vec![AllowedTargetKind::Direct, AllowedTargetKind::Tunnel],
        member_isolation: false,
        subnet: None,
        admin_ui_reachable: true,
        is_default: true,
        is_default_for_new: false,
        created_at: now,
        updated_at: now,
    }
}

/// Canned `NetworkZoneService` — returns fixed data so the handlers'
/// request/response plumbing is exercised end to end.
struct MockNetworkZoneService;

#[async_trait]
impl NetworkZoneService for MockNetworkZoneService {
    async fn list_zones(&self) -> Result<Vec<NetworkZoneView>, AppError> {
        Ok(vec![NetworkZoneView {
            zone: sample_zone(),
            member_count: 3,
        }])
    }
    async fn get_zone(&self, _id: Uuid) -> Result<NetworkZoneView, AppError> {
        Ok(NetworkZoneView {
            zone: sample_zone(),
            member_count: 3,
        })
    }
    async fn create_zone(&self, _req: CreateNetworkZoneRequest) -> Result<NetworkZone, AppError> {
        Ok(sample_zone())
    }
    async fn update_zone(
        &self,
        _id: Uuid,
        _req: UpdateNetworkZoneRequest,
    ) -> Result<NetworkZone, AppError> {
        Ok(sample_zone())
    }
    async fn delete_zone(&self, _id: Uuid) -> Result<(), AppError> {
        Ok(())
    }
    async fn set_default(&self, _id: Uuid) -> Result<(), AppError> {
        Ok(())
    }
    async fn set_default_for_new(&self, _id: Uuid) -> Result<(), AppError> {
        Ok(())
    }
    async fn assign_device(&self, _device_id: Uuid, _zone_id: Uuid) -> Result<(), AppError> {
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
        Arc::new(MockNetworkZoneService),
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

fn zone_router() -> Router {
    Router::new()
        .route(
            "/api/network/zones",
            get(crate::api::network_zone::list_zones).post(crate::api::network_zone::create_zone),
        )
        .route(
            "/api/network/zones/{id}",
            get(crate::api::network_zone::get_zone)
                .put(crate::api::network_zone::update_zone)
                .delete(crate::api::network_zone::delete_zone),
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
    let resp = zone_router().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), 32768).await.unwrap();
    let json = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
    (status, json)
}

#[tokio::test]
async fn list_zones_returns_views_with_member_count() {
    let (status, json) = send("GET", "/api/network/zones", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["zones"][0]["name"], "Trusted");
    assert_eq!(json["zones"][0]["member_count"], 3);
    assert_eq!(json["zones"][0]["is_default"], true);
}

#[tokio::test]
async fn create_zone_returns_201() {
    let body = r#"{"name":"Cameras","isolation_stance":"shared_subnet","allowed_targets":["tunnel"],"member_isolation":false,"admin_ui_reachable":true}"#;
    let (status, json) = send("POST", "/api/network/zones", Some(body)).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(json["zone"]["name"], "Trusted");
}

#[tokio::test]
async fn get_zone_returns_view() {
    let (status, json) = send("GET", &format!("/api/network/zones/{TRUSTED}"), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["zone"]["member_count"], 3);
}

#[tokio::test]
async fn update_zone_returns_updated() {
    let body = r#"{"admin_ui_reachable":false}"#;
    let (status, json) = send("PUT", &format!("/api/network/zones/{TRUSTED}"), Some(body)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["zone"]["name"], "Trusted");
}

#[tokio::test]
async fn delete_zone_returns_deleted_true() {
    let (status, json) = send("DELETE", &format!("/api/network/zones/{TRUSTED}"), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["deleted"], true);
}
