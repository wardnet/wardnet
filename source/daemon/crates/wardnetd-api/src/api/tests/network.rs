//! Tests for the network API endpoints
//! (`GET /api/network/status`,
//! `POST /api/network/discover-gateway-mac`,
//! `POST /api/network/dhcp-self-probe`).

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::sync::Mutex;

use async_trait::async_trait;
use axum::Router;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use axum::routing::{get, post};
use tower::ServiceExt;
use uuid::Uuid;
use wardnet_common::api::{
    DhcpSelfProbeResponse, DhcpSource, DiscoverGatewayMacRequest, DiscoverGatewayMacResponse,
    NetworkStatusResponse, RouterMacSource, SystemStatusResponse,
};

use crate::state::AppState;
use crate::tests::stubs::{
    StubDeviceService, StubDhcpServer, StubDhcpService, StubDiscoveryService, StubDnsFilterService,
    StubDnsLocalService, StubDnsServer, StubDnsService, StubEventPublisher, StubLogService,
    StubNetworkZoneService, StubProviderService, StubRoutingService, StubTunnelService,
};
use wardnetd_services::LogService;
use wardnetd_services::auth::service::LoginResult;
use wardnetd_services::error::AppError;
use wardnetd_services::{AuthService, SystemService};

// ---------------------------------------------------------------------------
// Auth stubs
// ---------------------------------------------------------------------------

struct AlwaysAuthService {
    admin_id: Uuid,
}

#[async_trait]
impl AuthService for AlwaysAuthService {
    async fn login(&self, _u: &str, _p: &str, _remember_me: bool) -> Result<LoginResult, AppError> {
        unimplemented!()
    }
    async fn validate_session(&self, _token: &str) -> Result<Option<Uuid>, AppError> {
        Ok(Some(self.admin_id))
    }
    async fn validate_api_key(&self, _key: &str) -> Result<Option<Uuid>, AppError> {
        Ok(Some(self.admin_id))
    }
    async fn setup_admin(&self, _username: &str, _password: &str) -> Result<(), AppError> {
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

struct NeverAuthService;
#[async_trait]
impl AuthService for NeverAuthService {
    async fn login(&self, _u: &str, _p: &str, _remember_me: bool) -> Result<LoginResult, AppError> {
        unimplemented!()
    }
    async fn validate_session(&self, _token: &str) -> Result<Option<Uuid>, AppError> {
        Ok(None)
    }
    async fn validate_api_key(&self, _key: &str) -> Result<Option<Uuid>, AppError> {
        Ok(None)
    }
    async fn setup_admin(&self, _username: &str, _password: &str) -> Result<(), AppError> {
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
// Mock SystemService — only the network-side methods matter here.
// Requests captured into `last_discover_request` so a test can assert
// the body was deserialised correctly.
// ---------------------------------------------------------------------------

#[allow(clippy::type_complexity)]
struct MockSystemService {
    network_status: Result<NetworkStatusResponse, AppError>,
    discover: Result<DiscoverGatewayMacResponse, AppError>,
    self_probe: Result<DhcpSelfProbeResponse, AppError>,
    last_discover_request: Mutex<Option<DiscoverGatewayMacRequest>>,
}

impl MockSystemService {
    fn new(
        network_status: Result<NetworkStatusResponse, AppError>,
        discover: Result<DiscoverGatewayMacResponse, AppError>,
        self_probe: Result<DhcpSelfProbeResponse, AppError>,
    ) -> Self {
        Self {
            network_status,
            discover,
            self_probe,
            last_discover_request: Mutex::new(None),
        }
    }
}

/// Re-create an [`AppError`] of the same variant as `err` so a mock
/// can return a fresh `Err` from a borrowed source. Internal/Database
/// errors collapse into a generic `Internal` because [`anyhow::Error`]
/// is not cloneable; the variant alone determines the HTTP status.
fn clone_app_error(err: &AppError) -> AppError {
    match err {
        AppError::BadRequest(s) => AppError::BadRequest(s.clone()),
        AppError::NotFound(s) => AppError::NotFound(s.clone()),
        AppError::Conflict(s) => AppError::Conflict(s.clone()),
        AppError::Forbidden(s) => AppError::Forbidden(s.clone()),
        AppError::Unauthorized(s) => AppError::Unauthorized(s.clone()),
        AppError::UpstreamUnavailable(s) => AppError::UpstreamUnavailable(s.clone()),
        AppError::Internal(_) | AppError::Database(_) => {
            AppError::Internal(anyhow::anyhow!("mock internal"))
        }
    }
}

#[async_trait]
impl SystemService for MockSystemService {
    fn version(&self) -> &'static str {
        "0.1.0-test"
    }
    fn uptime(&self) -> std::time::Duration {
        std::time::Duration::ZERO
    }
    async fn status(&self) -> Result<SystemStatusResponse, AppError> {
        unimplemented!()
    }
    async fn request_restart(&self) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn request_reboot(&self) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn request_shutdown(&self) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn network_status(&self) -> Result<NetworkStatusResponse, AppError> {
        match &self.network_status {
            Ok(r) => Ok(NetworkStatusResponse {
                interface: r.interface.clone(),
                ip: r.ip,
                gateway: r.gateway,
                dhcp_source: r.dhcp_source,
            }),
            Err(e) => Err(clone_app_error(e)),
        }
    }
    async fn discover_gateway_mac(
        &self,
        request: DiscoverGatewayMacRequest,
    ) -> Result<DiscoverGatewayMacResponse, AppError> {
        *self.last_discover_request.lock().unwrap() = Some(DiscoverGatewayMacRequest {
            mac: request.mac.clone(),
            target_ip: request.target_ip,
        });
        match &self.discover {
            Ok(r) => Ok(DiscoverGatewayMacResponse {
                mac: r.mac.clone(),
                source: r.source,
            }),
            Err(e) => Err(clone_app_error(e)),
        }
    }
    async fn dhcp_self_probe(&self) -> Result<DhcpSelfProbeResponse, AppError> {
        match &self.self_probe {
            Ok(r) => Ok(DhcpSelfProbeResponse {
                wardnet_responded: r.wardnet_responded,
                foreign_responded: r.foreign_responded,
                foreign_server_ip: r.foreign_server_ip,
            }),
            Err(e) => Err(clone_app_error(e)),
        }
    }
    async fn record_heartbeat(&self) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn record_graceful_shutdown(&self) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn acknowledge_last_shutdown(&self) -> Result<(), AppError> {
        unimplemented!()
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_state(auth: impl AuthService + 'static, system: Arc<MockSystemService>) -> AppState {
    AppState::new(
        Arc::new(auth),
        Arc::new(crate::tests::stubs::StubBackupService),
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
        Arc::new(StubNetworkZoneService),
        system,
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

fn network_app(state: AppState) -> Router {
    Router::new()
        .route("/api/network/status", get(crate::api::network::status))
        .route(
            "/api/network/discover-gateway-mac",
            post(crate::api::network::discover_gateway_mac),
        )
        .route(
            "/api/network/dhcp-self-probe",
            post(crate::api::network::dhcp_self_probe),
        )
        .with_state(state)
}

fn connect_info_ext() -> ConnectInfo<SocketAddr> {
    ConnectInfo(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 1234))
}

fn ok_status() -> NetworkStatusResponse {
    NetworkStatusResponse {
        interface: "eth0".to_owned(),
        ip: Ipv4Addr::new(192, 168, 1, 2),
        gateway: Some(Ipv4Addr::new(192, 168, 1, 1)),
        dhcp_source: DhcpSource::Static,
    }
}

fn ok_discover() -> DiscoverGatewayMacResponse {
    DiscoverGatewayMacResponse {
        mac: "AA:BB:CC:DD:EE:FF".to_owned(),
        source: RouterMacSource::Arp,
    }
}

fn ok_probe() -> DhcpSelfProbeResponse {
    DhcpSelfProbeResponse {
        wardnet_responded: true,
        foreign_responded: false,
        foreign_server_ip: None,
    }
}

fn err_internal() -> AppError {
    AppError::Internal(anyhow::anyhow!("mock"))
}

// ---------------------------------------------------------------------------
// GET /api/network/status
// ---------------------------------------------------------------------------

#[tokio::test]
async fn status_returns_200_and_payload() {
    let admin_id = Uuid::new_v4();
    let svc = Arc::new(MockSystemService::new(
        Ok(ok_status()),
        Ok(ok_discover()),
        Ok(ok_probe()),
    ));
    let state = make_state(AlwaysAuthService { admin_id }, svc);

    let app = network_app(state);
    let req = Request::builder()
        .uri("/api/network/status")
        .header("Cookie", "wardnet_session=valid-token")
        .extension(connect_info_ext())
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["interface"], "eth0");
    assert_eq!(json["ip"], "192.168.1.2");
    assert_eq!(json["gateway"], "192.168.1.1");
    assert_eq!(json["dhcp_source"], "static");
}

#[tokio::test]
async fn status_requires_authentication() {
    let svc = Arc::new(MockSystemService::new(
        Ok(ok_status()),
        Ok(ok_discover()),
        Ok(ok_probe()),
    ));
    let state = make_state(NeverAuthService, svc);

    let app = network_app(state);
    let req = Request::builder()
        .uri("/api/network/status")
        .extension(connect_info_ext())
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn status_surfaces_service_error_as_500() {
    let admin_id = Uuid::new_v4();
    let svc = Arc::new(MockSystemService::new(
        Err(err_internal()),
        Ok(ok_discover()),
        Ok(ok_probe()),
    ));
    let state = make_state(AlwaysAuthService { admin_id }, svc);

    let app = network_app(state);
    let req = Request::builder()
        .uri("/api/network/status")
        .header("Authorization", "Bearer key")
        .extension(connect_info_ext())
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

// ---------------------------------------------------------------------------
// POST /api/network/discover-gateway-mac
// ---------------------------------------------------------------------------

#[tokio::test]
async fn discover_gateway_mac_returns_arp_result() {
    let admin_id = Uuid::new_v4();
    let svc = Arc::new(MockSystemService::new(
        Ok(ok_status()),
        Ok(ok_discover()),
        Ok(ok_probe()),
    ));
    let state = make_state(AlwaysAuthService { admin_id }, svc.clone());

    let body = serde_json::to_vec(&DiscoverGatewayMacRequest::default()).unwrap();

    let app = network_app(state);
    let req = Request::builder()
        .method("POST")
        .uri("/api/network/discover-gateway-mac")
        .header("Cookie", "wardnet_session=valid-token")
        .header("Content-Type", "application/json")
        .extension(connect_info_ext())
        .body(Body::from(body))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["mac"], "AA:BB:CC:DD:EE:FF");
    assert_eq!(json["source"], "arp");

    let guard = svc.last_discover_request.lock().unwrap();
    let captured = guard.as_ref().expect("handler captured the request");
    assert!(captured.mac.is_none());
    assert!(captured.target_ip.is_none());
}

#[tokio::test]
async fn discover_gateway_mac_forwards_request_body() {
    let admin_id = Uuid::new_v4();
    let svc = Arc::new(MockSystemService::new(
        Ok(ok_status()),
        Ok(DiscoverGatewayMacResponse {
            mac: "11:22:33:44:55:66".to_owned(),
            source: RouterMacSource::Manual,
        }),
        Ok(ok_probe()),
    ));
    let state = make_state(AlwaysAuthService { admin_id }, svc.clone());

    let body = serde_json::to_vec(&DiscoverGatewayMacRequest {
        mac: Some("11:22:33:44:55:66".to_owned()),
        target_ip: Some(Ipv4Addr::new(10, 0, 0, 1)),
    })
    .unwrap();

    let app = network_app(state);
    let req = Request::builder()
        .method("POST")
        .uri("/api/network/discover-gateway-mac")
        .header("Authorization", "Bearer key")
        .header("Content-Type", "application/json")
        .extension(connect_info_ext())
        .body(Body::from(body))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let guard = svc.last_discover_request.lock().unwrap();
    let captured = guard.as_ref().expect("handler captured the request");
    assert_eq!(captured.mac.as_deref(), Some("11:22:33:44:55:66"));
    assert_eq!(captured.target_ip, Some(Ipv4Addr::new(10, 0, 0, 1)));
}

#[tokio::test]
async fn discover_gateway_mac_bad_request_for_invalid_mac() {
    let admin_id = Uuid::new_v4();
    let svc = Arc::new(MockSystemService::new(
        Ok(ok_status()),
        Err(AppError::BadRequest("not a mac".to_owned())),
        Ok(ok_probe()),
    ));
    let state = make_state(AlwaysAuthService { admin_id }, svc);

    let body = serde_json::to_vec(&DiscoverGatewayMacRequest {
        mac: Some("not-a-mac".to_owned()),
        target_ip: None,
    })
    .unwrap();

    let app = network_app(state);
    let req = Request::builder()
        .method("POST")
        .uri("/api/network/discover-gateway-mac")
        .header("Cookie", "wardnet_session=valid-token")
        .header("Content-Type", "application/json")
        .extension(connect_info_ext())
        .body(Body::from(body))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn discover_gateway_mac_not_found_when_probe_silent() {
    let admin_id = Uuid::new_v4();
    let svc = Arc::new(MockSystemService::new(
        Ok(ok_status()),
        Err(AppError::NotFound("no arp reply".to_owned())),
        Ok(ok_probe()),
    ));
    let state = make_state(AlwaysAuthService { admin_id }, svc);

    let body = serde_json::to_vec(&DiscoverGatewayMacRequest::default()).unwrap();

    let app = network_app(state);
    let req = Request::builder()
        .method("POST")
        .uri("/api/network/discover-gateway-mac")
        .header("Cookie", "wardnet_session=valid-token")
        .header("Content-Type", "application/json")
        .extension(connect_info_ext())
        .body(Body::from(body))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn discover_gateway_mac_requires_authentication() {
    let svc = Arc::new(MockSystemService::new(
        Ok(ok_status()),
        Ok(ok_discover()),
        Ok(ok_probe()),
    ));
    let state = make_state(NeverAuthService, svc);

    let body = serde_json::to_vec(&DiscoverGatewayMacRequest::default()).unwrap();

    let app = network_app(state);
    let req = Request::builder()
        .method("POST")
        .uri("/api/network/discover-gateway-mac")
        .header("Content-Type", "application/json")
        .extension(connect_info_ext())
        .body(Body::from(body))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

// ---------------------------------------------------------------------------
// POST /api/network/dhcp-self-probe
// ---------------------------------------------------------------------------

#[tokio::test]
async fn dhcp_self_probe_returns_results() {
    let admin_id = Uuid::new_v4();
    let svc = Arc::new(MockSystemService::new(
        Ok(ok_status()),
        Ok(ok_discover()),
        Ok(DhcpSelfProbeResponse {
            wardnet_responded: true,
            foreign_responded: true,
            foreign_server_ip: Some(Ipv4Addr::new(192, 168, 1, 254)),
        }),
    ));
    let state = make_state(AlwaysAuthService { admin_id }, svc);

    let app = network_app(state);
    let req = Request::builder()
        .method("POST")
        .uri("/api/network/dhcp-self-probe")
        .header("Cookie", "wardnet_session=valid-token")
        .extension(connect_info_ext())
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["wardnet_responded"], true);
    assert_eq!(json["foreign_responded"], true);
    assert_eq!(json["foreign_server_ip"], "192.168.1.254");
}

#[tokio::test]
async fn dhcp_self_probe_requires_authentication() {
    let svc = Arc::new(MockSystemService::new(
        Ok(ok_status()),
        Ok(ok_discover()),
        Ok(ok_probe()),
    ));
    let state = make_state(NeverAuthService, svc);

    let app = network_app(state);
    let req = Request::builder()
        .method("POST")
        .uri("/api/network/dhcp-self-probe")
        .extension(connect_info_ext())
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn dhcp_self_probe_surfaces_service_error_as_500() {
    let admin_id = Uuid::new_v4();
    let svc = Arc::new(MockSystemService::new(
        Ok(ok_status()),
        Ok(ok_discover()),
        Err(err_internal()),
    ));
    let state = make_state(AlwaysAuthService { admin_id }, svc);

    let app = network_app(state);
    let req = Request::builder()
        .method("POST")
        .uri("/api/network/dhcp-self-probe")
        .header("Cookie", "wardnet_session=valid-token")
        .extension(connect_info_ext())
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
}
