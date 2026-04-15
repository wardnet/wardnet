//! Tests for the device API endpoints (GET /api/devices/me, PUT /api/devices/me/rule,
//! GET /api/devices, GET /api/devices/:id, PUT /api/devices/:id).

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use axum::Router;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use axum::routing::{get, put};
use tower::ServiceExt;
use uuid::Uuid;
use wardnet_types::api::{DeviceMeResponse, SetMyRuleResponse};
use wardnet_types::device::{Device, DeviceType};
use wardnet_types::routing::RoutingTarget;

use crate::config::Config;
use crate::error::AppError;
use crate::log_broadcast::LogBroadcaster;
use crate::recent_errors::RecentErrors;
use crate::service::auth::LoginResult;
use crate::service::{AuthService, DeviceDiscoveryService, DeviceService, DhcpService};
use crate::state::AppState;
use crate::tests::stubs::{
    StubDhcpService, StubDnsService, StubEventPublisher, StubProviderService, StubRoutingService,
    StubSystemService, StubTunnelService,
};

// ---------------------------------------------------------------------------
// Mock services
// ---------------------------------------------------------------------------

/// Mock `AuthService` that always validates sessions.
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
        unimplemented!()
    }
}

/// Mock `DeviceService` returning configurable responses.
struct MockDeviceService {
    device: Option<Device>,
    rule: Option<RoutingTarget>,
    admin_locked: bool,
    set_rule_error: Option<String>,
}

impl MockDeviceService {
    fn found(device: Device, rule: Option<RoutingTarget>) -> Self {
        let admin_locked = device.admin_locked;
        Self {
            device: Some(device),
            rule,
            admin_locked,
            set_rule_error: None,
        }
    }

    fn not_found() -> Self {
        Self {
            device: None,
            rule: None,
            admin_locked: false,
            set_rule_error: Some("not_found".to_owned()),
        }
    }

    fn forbidden(device: Device) -> Self {
        Self {
            device: Some(device),
            rule: None,
            admin_locked: true,
            set_rule_error: Some("forbidden".to_owned()),
        }
    }
}

#[async_trait]
impl DeviceService for MockDeviceService {
    async fn get_device_for_ip(&self, _ip: &str) -> Result<DeviceMeResponse, AppError> {
        Ok(DeviceMeResponse {
            device: self.device.clone(),
            current_rule: self.rule.clone(),
            admin_locked: self.admin_locked,
            available_tunnels: vec![],
        })
    }

    async fn set_rule_for_ip(
        &self,
        _ip: &str,
        target: RoutingTarget,
    ) -> Result<SetMyRuleResponse, AppError> {
        match self.set_rule_error.as_deref() {
            Some("not_found") => Err(AppError::NotFound(
                "device not found for this IP".to_owned(),
            )),
            Some("forbidden") => Err(AppError::Forbidden("routing is locked by admin".to_owned())),
            _ => Ok(SetMyRuleResponse {
                message: "routing rule updated".to_owned(),
                target,
            }),
        }
    }

    async fn set_rule(&self, _id: &str, _t: RoutingTarget) -> Result<(), AppError> {
        Ok(())
    }

    async fn update_admin_locked(&self, _id: &str, _locked: bool) -> Result<(), AppError> {
        Ok(())
    }
}

/// Mock `DhcpService` that returns configurable leases and reservations.
///
/// All mutating methods delegate to `StubDhcpService` (i.e. panic).
/// Only the read methods used by the device list enrichment are implemented.
struct MockDhcpService {
    leases: Vec<wardnet_types::dhcp::DhcpLease>,
    reservations: Vec<wardnet_types::dhcp::DhcpReservation>,
}

impl MockDhcpService {
    fn empty() -> Self {
        Self {
            leases: vec![],
            reservations: vec![],
        }
    }
}

#[async_trait]
impl DhcpService for MockDhcpService {
    async fn get_config(&self) -> Result<wardnet_types::api::DhcpConfigResponse, AppError> {
        unimplemented!()
    }
    async fn update_config(
        &self,
        _r: wardnet_types::api::UpdateDhcpConfigRequest,
    ) -> Result<wardnet_types::api::DhcpConfigResponse, AppError> {
        unimplemented!()
    }
    async fn toggle(
        &self,
        _r: wardnet_types::api::ToggleDhcpRequest,
    ) -> Result<wardnet_types::api::DhcpConfigResponse, AppError> {
        unimplemented!()
    }
    async fn list_leases(&self) -> Result<wardnet_types::api::ListDhcpLeasesResponse, AppError> {
        Ok(wardnet_types::api::ListDhcpLeasesResponse {
            leases: self.leases.clone(),
        })
    }
    async fn revoke_lease(
        &self,
        _id: Uuid,
    ) -> Result<wardnet_types::api::RevokeDhcpLeaseResponse, AppError> {
        unimplemented!()
    }
    async fn list_reservations(
        &self,
    ) -> Result<wardnet_types::api::ListDhcpReservationsResponse, AppError> {
        Ok(wardnet_types::api::ListDhcpReservationsResponse {
            reservations: self.reservations.clone(),
        })
    }
    async fn create_reservation(
        &self,
        _r: wardnet_types::api::CreateDhcpReservationRequest,
    ) -> Result<wardnet_types::api::CreateDhcpReservationResponse, AppError> {
        unimplemented!()
    }
    async fn delete_reservation(
        &self,
        _id: Uuid,
    ) -> Result<wardnet_types::api::DeleteDhcpReservationResponse, AppError> {
        unimplemented!()
    }
    async fn status(&self) -> Result<wardnet_types::api::DhcpStatusResponse, AppError> {
        unimplemented!()
    }
    async fn assign_lease(
        &self,
        _mac: &str,
        _hostname: Option<&str>,
    ) -> Result<wardnet_types::dhcp::DhcpLease, AppError> {
        unimplemented!()
    }
    async fn renew_lease(&self, _mac: &str) -> Result<wardnet_types::dhcp::DhcpLease, AppError> {
        unimplemented!()
    }
    async fn release_lease(&self, _mac: &str) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn cleanup_expired(&self) -> Result<u64, AppError> {
        unimplemented!()
    }
    async fn get_dhcp_config(&self) -> Result<wardnet_types::dhcp::DhcpConfig, AppError> {
        unimplemented!()
    }
}

/// Mock `DeviceDiscoveryService` for admin device endpoints.
struct MockDiscoveryService {
    devices: Vec<Device>,
}

#[async_trait]
impl DeviceDiscoveryService for MockDiscoveryService {
    async fn restore_devices(&self) -> Result<(), AppError> {
        Ok(())
    }
    async fn process_observation(
        &self,
        _obs: &crate::packet_capture::ObservedDevice,
    ) -> Result<crate::service::ObservationResult, AppError> {
        unimplemented!()
    }
    async fn flush_last_seen(&self) -> Result<u64, AppError> {
        Ok(0)
    }
    async fn scan_departures(&self, _timeout_secs: u64) -> Result<Vec<Uuid>, AppError> {
        Ok(vec![])
    }
    async fn resolve_hostname(&self, _device_id: Uuid, _ip: String) -> Result<(), AppError> {
        Ok(())
    }
    async fn get_all_devices(&self) -> Result<Vec<Device>, AppError> {
        Ok(self.devices.clone())
    }
    async fn get_device_by_id(&self, id: Uuid) -> Result<Device, AppError> {
        self.devices
            .iter()
            .find(|d| d.id == id)
            .cloned()
            .ok_or_else(|| AppError::NotFound(format!("device {id} not found")))
    }
    async fn update_device(
        &self,
        id: Uuid,
        name: Option<&str>,
        _device_type: Option<DeviceType>,
    ) -> Result<Device, AppError> {
        let mut device = self.get_device_by_id(id).await?;
        if let Some(n) = name {
            device.name = Some(n.to_owned());
        }
        Ok(device)
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn sample_device() -> Device {
    Device {
        id: Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap(),
        mac: "AA:BB:CC:DD:EE:01".to_owned(),
        name: Some("My Phone".to_owned()),
        hostname: None,
        manufacturer: Some("Apple".to_owned()),
        device_type: DeviceType::Phone,
        first_seen: "2026-03-07T00:00:00Z".parse().unwrap(),
        last_seen: "2026-03-07T00:00:00Z".parse().unwrap(),
        last_ip: "192.168.1.10".to_owned(),
        admin_locked: false,
    }
}

fn connect_info() -> ConnectInfo<SocketAddr> {
    ConnectInfo(SocketAddr::new(
        IpAddr::V4(Ipv4Addr::new(192, 168, 1, 10)),
        12345,
    ))
}

fn build_state(
    device_svc: impl DeviceService + 'static,
    discovery_svc: impl DeviceDiscoveryService + 'static,
) -> AppState {
    build_state_with_dhcp(device_svc, discovery_svc, StubDhcpService)
}

fn build_state_with_dhcp(
    device_svc: impl DeviceService + 'static,
    discovery_svc: impl DeviceDiscoveryService + 'static,
    dhcp_svc: impl DhcpService + 'static,
) -> AppState {
    AppState::new(
        Arc::new(MockAuthService),
        Arc::new(device_svc),
        Arc::new(dhcp_svc),
        Arc::new(StubDnsService),
        Arc::new(discovery_svc),
        Arc::new(StubProviderService),
        Arc::new(StubRoutingService),
        Arc::new(StubSystemService),
        Arc::new(StubTunnelService),
        Arc::new(crate::dhcp::server::NoopDhcpServer),
        Arc::new(crate::dns::server::NoopDnsServer),
        Arc::new(StubEventPublisher),
        LogBroadcaster::new(16),
        RecentErrors::new(),
        Config::default(),
        Instant::now(),
    )
}

fn device_router(state: AppState) -> Router {
    Router::new()
        .route("/api/devices/me", get(crate::api::devices::get_me))
        .route(
            "/api/devices/me/rule",
            put(crate::api::devices::set_my_rule),
        )
        .route("/api/devices", get(crate::api::devices::list_devices))
        .route(
            "/api/devices/{id}",
            get(crate::api::devices::get_device).put(crate::api::devices::update_device),
        )
        .with_state(state)
}

/// Send an authenticated GET request with `ConnectInfo` extension.
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

/// Send an authenticated PUT request with JSON body and `ConnectInfo` extension.
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
// GET /api/devices/me
// ---------------------------------------------------------------------------

#[tokio::test]
async fn get_me_returns_device_when_found() {
    let device = sample_device();
    let state = build_state(
        MockDeviceService::found(device, Some(RoutingTarget::Direct)),
        MockDiscoveryService { devices: vec![] },
    );
    let app = device_router(state);

    let (status, json) = get_json(app, "/api/devices/me").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["device"]["mac"], "AA:BB:CC:DD:EE:01");
    assert_eq!(json["current_rule"]["type"], "direct");
    assert_eq!(json["admin_locked"], false);
}

#[tokio::test]
async fn get_me_returns_null_device_when_unknown_ip() {
    let state = build_state(
        MockDeviceService::not_found(),
        MockDiscoveryService { devices: vec![] },
    );
    let app = device_router(state);

    let (status, json) = get_json(app, "/api/devices/me").await;
    assert_eq!(status, StatusCode::OK);
    assert!(json["device"].is_null());
    assert!(json["current_rule"].is_null());
}

// ---------------------------------------------------------------------------
// PUT /api/devices/me/rule
// ---------------------------------------------------------------------------

#[tokio::test]
async fn set_my_rule_success() {
    let state = build_state(
        MockDeviceService::found(sample_device(), None),
        MockDiscoveryService { devices: vec![] },
    );
    let app = device_router(state);

    let (status, json) = put_json(
        app,
        "/api/devices/me/rule",
        r#"{"target":{"type":"direct"}}"#,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["target"]["type"], "direct");
    assert_eq!(json["message"], "routing rule updated");
}

#[tokio::test]
async fn set_my_rule_with_tunnel_target() {
    let tunnel_id = "00000000-0000-0000-0000-000000000010";
    let state = build_state(
        MockDeviceService::found(sample_device(), None),
        MockDiscoveryService { devices: vec![] },
    );
    let app = device_router(state);

    let body = format!(r#"{{"target":{{"type":"tunnel","tunnel_id":"{tunnel_id}"}}}}"#);
    let (status, json) = put_json(app, "/api/devices/me/rule", &body).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["target"]["type"], "tunnel");
    assert_eq!(json["target"]["tunnel_id"], tunnel_id);
}

#[tokio::test]
async fn set_my_rule_device_not_found() {
    let state = build_state(
        MockDeviceService::not_found(),
        MockDiscoveryService { devices: vec![] },
    );
    let app = device_router(state);

    let (status, json) = put_json(
        app,
        "/api/devices/me/rule",
        r#"{"target":{"type":"direct"}}"#,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(json["error"], "not found");
}

#[tokio::test]
async fn set_my_rule_forbidden_when_locked() {
    let mut device = sample_device();
    device.admin_locked = true;

    let svc = MockDeviceService::forbidden(device);
    let state = build_state(svc, MockDiscoveryService { devices: vec![] });
    let app = device_router(state);

    let (status, json) = put_json(
        app,
        "/api/devices/me/rule",
        r#"{"target":{"type":"direct"}}"#,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(json["error"], "forbidden");
}

#[tokio::test]
async fn set_my_rule_bad_json_returns_error() {
    let state = build_state(
        MockDeviceService::found(sample_device(), None),
        MockDiscoveryService { devices: vec![] },
    );
    let app = device_router(state);

    let resp = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/devices/me/rule")
                .header("Content-Type", "application/json")
                .header("Cookie", "wardnet_session=valid-token")
                .extension(connect_info())
                .body(Body::from("not json"))
                .unwrap(),
        )
        .await
        .unwrap();

    // Axum returns 400 or 422 for deserialization failures depending on version.
    let status = resp.status();
    assert!(
        status == StatusCode::BAD_REQUEST || status == StatusCode::UNPROCESSABLE_ENTITY,
        "expected 400 or 422, got {status}"
    );
}

// ---------------------------------------------------------------------------
// GET /api/devices (admin, list all)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn list_devices_returns_all() {
    let device = sample_device();
    let state = build_state_with_dhcp(
        MockDeviceService::not_found(),
        MockDiscoveryService {
            devices: vec![device],
        },
        MockDhcpService::empty(),
    );
    let app = device_router(state);

    let (status, json) = get_json(app, "/api/devices").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["devices"].as_array().unwrap().len(), 1);
    assert_eq!(json["devices"][0]["mac"], "AA:BB:CC:DD:EE:01");
    // No lease or reservation for this device, so dhcp_status should be "external".
    assert_eq!(json["devices"][0]["dhcp_status"], "external");
}

#[tokio::test]
async fn list_devices_returns_empty() {
    let state = build_state_with_dhcp(
        MockDeviceService::not_found(),
        MockDiscoveryService { devices: vec![] },
        MockDhcpService::empty(),
    );
    let app = device_router(state);

    let (status, json) = get_json(app, "/api/devices").await;
    assert_eq!(status, StatusCode::OK);
    assert!(json["devices"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn list_devices_unauthorized_without_session() {
    let state = build_state_with_dhcp(
        MockDeviceService::not_found(),
        MockDiscoveryService { devices: vec![] },
        MockDhcpService::empty(),
    );
    let app = device_router(state);

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/devices")
                .extension(connect_info())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

// ---------------------------------------------------------------------------
// GET /api/devices/:id (admin, detail)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn get_device_by_id_success() {
    let device = sample_device();
    let state = build_state_with_dhcp(
        MockDeviceService::found(device.clone(), Some(RoutingTarget::Direct)),
        MockDiscoveryService {
            devices: vec![device],
        },
        MockDhcpService::empty(),
    );
    let app = device_router(state);

    let (status, json) = get_json(app, "/api/devices/00000000-0000-0000-0000-000000000001").await;
    assert_eq!(status, StatusCode::OK);
    // DeviceDetailResponse is { device, current_rule } — device fields are
    // nested, current_rule sits at the top level.
    assert_eq!(json["device"]["mac"], "AA:BB:CC:DD:EE:01");
    assert_eq!(json["current_rule"]["type"], "direct");
    assert_eq!(json["device"]["dhcp_status"], "external");
}

#[tokio::test]
async fn get_device_by_id_not_found() {
    let state = build_state_with_dhcp(
        MockDeviceService::not_found(),
        MockDiscoveryService { devices: vec![] },
        MockDhcpService::empty(),
    );
    let app = device_router(state);

    let (status, json) = get_json(app, "/api/devices/00000000-0000-0000-0000-000000000099").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(json["error"], "not found");
}

#[tokio::test]
async fn get_device_by_id_invalid_uuid() {
    let state = build_state_with_dhcp(
        MockDeviceService::not_found(),
        MockDiscoveryService { devices: vec![] },
        MockDhcpService::empty(),
    );
    let app = device_router(state);

    let (status, json) = get_json(app, "/api/devices/not-a-uuid").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["error"], "bad request");
}

// ---------------------------------------------------------------------------
// PUT /api/devices/:id (admin, update)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn update_device_success() {
    let device = sample_device();
    let state = build_state_with_dhcp(
        MockDeviceService::found(device.clone(), None),
        MockDiscoveryService {
            devices: vec![device],
        },
        MockDhcpService::empty(),
    );
    let app = device_router(state);

    let (status, json) = put_json(
        app,
        "/api/devices/00000000-0000-0000-0000-000000000001",
        r#"{"name":"Renamed Device"}"#,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["device"]["name"], "Renamed Device");
}

#[tokio::test]
async fn update_device_with_type() {
    let device = sample_device();
    let state = build_state_with_dhcp(
        MockDeviceService::found(device.clone(), None),
        MockDiscoveryService {
            devices: vec![device],
        },
        MockDhcpService::empty(),
    );
    let app = device_router(state);

    let (status, json) = put_json(
        app,
        "/api/devices/00000000-0000-0000-0000-000000000001",
        r#"{"device_type":"laptop"}"#,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(json["device"]["mac"].is_string());
}

#[tokio::test]
async fn update_device_invalid_uuid() {
    let state = build_state_with_dhcp(
        MockDeviceService::not_found(),
        MockDiscoveryService { devices: vec![] },
        MockDhcpService::empty(),
    );
    let app = device_router(state);

    let (status, json) = put_json(app, "/api/devices/not-a-uuid", r#"{"name":"x"}"#).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["error"], "bad request");
}

#[tokio::test]
async fn update_device_not_found() {
    let state = build_state_with_dhcp(
        MockDeviceService::not_found(),
        MockDiscoveryService { devices: vec![] },
        MockDhcpService::empty(),
    );
    let app = device_router(state);

    let (status, json) = put_json(
        app,
        "/api/devices/00000000-0000-0000-0000-000000000099",
        r#"{"name":"x"}"#,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(json["error"], "not found");
}

// ---------------------------------------------------------------------------
// PUT /api/devices/:id with routing_target and admin_locked
// ---------------------------------------------------------------------------

#[tokio::test]
async fn update_device_with_routing_target() {
    let device = sample_device();
    let state = build_state_with_dhcp(
        MockDeviceService::found(device.clone(), None),
        MockDiscoveryService {
            devices: vec![device],
        },
        MockDhcpService::empty(),
    );
    let app = device_router(state);

    let (status, _json) = put_json(
        app,
        "/api/devices/00000000-0000-0000-0000-000000000001",
        r#"{"routing_target":{"type":"direct"}}"#,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn update_device_with_admin_locked() {
    let device = sample_device();
    let state = build_state_with_dhcp(
        MockDeviceService::found(device.clone(), None),
        MockDiscoveryService {
            devices: vec![device],
        },
        MockDhcpService::empty(),
    );
    let app = device_router(state);

    let (status, _json) = put_json(
        app,
        "/api/devices/00000000-0000-0000-0000-000000000001",
        r#"{"admin_locked":true}"#,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}

// ---------------------------------------------------------------------------
// DHCP status enrichment
// ---------------------------------------------------------------------------

#[tokio::test]
async fn list_devices_with_dhcp_lease_shows_lease_status() {
    let device = sample_device();
    let lease = wardnet_types::dhcp::DhcpLease {
        id: Uuid::new_v4(),
        mac_address: "AA:BB:CC:DD:EE:01".to_owned(),
        ip_address: "192.168.1.10".parse().unwrap(),
        hostname: None,
        lease_start: "2026-04-13T00:00:00Z".parse().unwrap(),
        lease_end: "2026-04-13T01:00:00Z".parse().unwrap(),
        status: wardnet_types::dhcp::DhcpLeaseStatus::Active,
        device_id: Some(device.id),
        created_at: "2026-04-13T00:00:00Z".parse().unwrap(),
        updated_at: "2026-04-13T00:00:00Z".parse().unwrap(),
    };

    let dhcp = MockDhcpService {
        leases: vec![lease],
        reservations: vec![],
    };

    let state = build_state_with_dhcp(
        MockDeviceService::not_found(),
        MockDiscoveryService {
            devices: vec![device],
        },
        dhcp,
    );
    let app = device_router(state);

    let (status, json) = get_json(app, "/api/devices").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["devices"][0]["dhcp_status"], "lease");
}

#[tokio::test]
async fn list_devices_with_dhcp_reservation_shows_reservation_status() {
    let device = sample_device();
    let reservation = wardnet_types::dhcp::DhcpReservation {
        id: Uuid::new_v4(),
        mac_address: "AA:BB:CC:DD:EE:01".to_owned(),
        ip_address: "192.168.1.10".parse().unwrap(),
        hostname: None,
        description: None,
        created_at: "2026-04-13T00:00:00Z".parse().unwrap(),
        updated_at: "2026-04-13T00:00:00Z".parse().unwrap(),
    };

    let dhcp = MockDhcpService {
        leases: vec![],
        reservations: vec![reservation],
    };

    let state = build_state_with_dhcp(
        MockDeviceService::not_found(),
        MockDiscoveryService {
            devices: vec![device],
        },
        dhcp,
    );
    let app = device_router(state);

    let (status, json) = get_json(app, "/api/devices").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["devices"][0]["dhcp_status"], "reservation");
}

#[tokio::test]
async fn list_devices_reservation_overrides_lease() {
    let device = sample_device();
    let lease = wardnet_types::dhcp::DhcpLease {
        id: Uuid::new_v4(),
        mac_address: "AA:BB:CC:DD:EE:01".to_owned(),
        ip_address: "192.168.1.10".parse().unwrap(),
        hostname: None,
        lease_start: "2026-04-13T00:00:00Z".parse().unwrap(),
        lease_end: "2026-04-13T01:00:00Z".parse().unwrap(),
        status: wardnet_types::dhcp::DhcpLeaseStatus::Active,
        device_id: Some(device.id),
        created_at: "2026-04-13T00:00:00Z".parse().unwrap(),
        updated_at: "2026-04-13T00:00:00Z".parse().unwrap(),
    };
    let reservation = wardnet_types::dhcp::DhcpReservation {
        id: Uuid::new_v4(),
        mac_address: "AA:BB:CC:DD:EE:01".to_owned(),
        ip_address: "192.168.1.10".parse().unwrap(),
        hostname: None,
        description: None,
        created_at: "2026-04-13T00:00:00Z".parse().unwrap(),
        updated_at: "2026-04-13T00:00:00Z".parse().unwrap(),
    };

    // Both lease and reservation for the same MAC -- reservation should win.
    let dhcp = MockDhcpService {
        leases: vec![lease],
        reservations: vec![reservation],
    };

    let state = build_state_with_dhcp(
        MockDeviceService::not_found(),
        MockDiscoveryService {
            devices: vec![device],
        },
        dhcp,
    );
    let app = device_router(state);

    let (status, json) = get_json(app, "/api/devices").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        json["devices"][0]["dhcp_status"], "reservation",
        "reservation should override lease"
    );
}

#[tokio::test]
async fn get_device_by_id_includes_dhcp_status() {
    let device = sample_device();
    let lease = wardnet_types::dhcp::DhcpLease {
        id: Uuid::new_v4(),
        mac_address: "aa:bb:cc:dd:ee:01".to_owned(), // lowercase MAC
        ip_address: "192.168.1.10".parse().unwrap(),
        hostname: None,
        lease_start: "2026-04-13T00:00:00Z".parse().unwrap(),
        lease_end: "2026-04-13T01:00:00Z".parse().unwrap(),
        status: wardnet_types::dhcp::DhcpLeaseStatus::Active,
        device_id: Some(device.id),
        created_at: "2026-04-13T00:00:00Z".parse().unwrap(),
        updated_at: "2026-04-13T00:00:00Z".parse().unwrap(),
    };

    let dhcp = MockDhcpService {
        leases: vec![lease],
        reservations: vec![],
    };

    let state = build_state_with_dhcp(
        MockDeviceService::found(device.clone(), Some(RoutingTarget::Direct)),
        MockDiscoveryService {
            devices: vec![device],
        },
        dhcp,
    );
    let app = device_router(state);

    let (status, json) = get_json(app, "/api/devices/00000000-0000-0000-0000-000000000001").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["device"]["dhcp_status"], "lease");
}
