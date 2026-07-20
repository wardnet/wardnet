//! Tests for the DNS capture settings API endpoints.
//! GET  /api/devices/{id}/dns-capture
//! PATCH /api/devices/{id}/dns-capture

use std::sync::Arc;

use async_trait::async_trait;
use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::get;
use tower::ServiceExt;
use uuid::Uuid;
use wardnet_common::api::{DeviceMeResponse, DnsCaptureSettingsResponse, SetMyRuleResponse};
use wardnet_common::routing::RoutingTarget;

use crate::state::AppState;
use crate::tests::stubs::{
    StubDhcpServer, StubDhcpService, StubDnsFilterService, StubDnsServer, StubDnsService,
    StubEventPublisher, StubLogService, StubNetworkZoneService, StubProviderService,
    StubRoutingService, StubSystemService, StubTunnelService,
};
use wardnetd_services::DeviceService;
use wardnetd_services::LogService;
use wardnetd_services::auth::service::LoginResult;
use wardnetd_services::error::AppError;

// ---------------------------------------------------------------------------
// MockAuthService — always validates sessions as admin
// ---------------------------------------------------------------------------

struct MockAuthService;

#[async_trait]
impl wardnetd_services::AuthService for MockAuthService {
    async fn current_admin_username(&self) -> Result<String, AppError> {
        Ok("admin".to_owned())
    }
    async fn login(&self, _u: &str, _p: &str, _remember_me: bool) -> Result<LoginResult, AppError> {
        Ok(LoginResult {
            token: "t".to_owned(),
            max_age_seconds: 3600,
        })
    }
    async fn cleanup_expired_sessions(&self) -> Result<u64, AppError> {
        unimplemented!()
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

// ---------------------------------------------------------------------------
// MockDnsDeviceService
// ---------------------------------------------------------------------------

/// Controls what `get_dns_capture_settings` and `update_dns_capture_settings`
/// return. All other `DeviceService` methods are stubbed with `unimplemented!`.
struct MockDnsDeviceService {
    /// Value returned by `get_dns_capture_settings`.
    /// `None` causes the method to return `AppError::NotFound`.
    get_result: Option<DnsCaptureSettingsResponse>,

    /// When `true`, `update_dns_capture_settings` returns `AppError::NotFound`.
    update_not_found: bool,
}

impl MockDnsDeviceService {
    /// Service where both get and update succeed.
    fn found(settings: DnsCaptureSettingsResponse) -> Self {
        Self {
            get_result: Some(settings),
            update_not_found: false,
        }
    }

    /// Service where the device is not found on the get side.
    fn get_not_found() -> Self {
        Self {
            get_result: None,
            update_not_found: false,
        }
    }

    /// Service where the device is not found on the update side (get would
    /// never be reached in the handler, but set to None for safety).
    fn update_not_found() -> Self {
        Self {
            get_result: None,
            update_not_found: true,
        }
    }
}

#[async_trait]
impl DeviceService for MockDnsDeviceService {
    async fn get_device(
        &self,
        _device_id: &str,
    ) -> Result<Option<wardnet_common::device::Device>, AppError> {
        unimplemented!()
    }
    async fn clear_remote_connection_mode(&self, _device_id: &str) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn get_device_for_ip(&self, _ip: &str) -> Result<DeviceMeResponse, AppError> {
        unimplemented!()
    }
    async fn set_rule_for_ip(
        &self,
        _ip: &str,
        _target: RoutingTarget,
    ) -> Result<SetMyRuleResponse, AppError> {
        unimplemented!()
    }
    async fn set_rule(&self, _id: &str, _t: RoutingTarget) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn current_rules(
        &self,
    ) -> Result<std::collections::HashMap<Uuid, RoutingTarget>, AppError> {
        unimplemented!()
    }
    async fn get_rule_for_device(
        &self,
        _device_id: &str,
    ) -> Result<Option<RoutingTarget>, AppError> {
        unimplemented!()
    }
    async fn update_admin_locked(&self, _id: &str, _locked: bool) -> Result<(), AppError> {
        unimplemented!()
    }

    async fn get_dns_capture_settings(
        &self,
        id: &str,
    ) -> Result<DnsCaptureSettingsResponse, AppError> {
        self.get_result
            .clone()
            .ok_or_else(|| AppError::NotFound(format!("device {id} not found")))
    }

    async fn update_dns_capture_settings(
        &self,
        id: &str,
        _enabled: Option<bool>,
        _cap_count: Option<i64>,
        _cap_days: Option<i64>,
    ) -> Result<(), AppError> {
        if self.update_not_found {
            return Err(AppError::NotFound(format!("device {id} not found")));
        }
        Ok(())
    }
    async fn set_my_capture_enabled(
        &self,
        _ip: &str,
        enabled: bool,
    ) -> Result<DnsCaptureSettingsResponse, AppError> {
        let mut settings = self
            .get_result
            .clone()
            .ok_or_else(|| AppError::NotFound("device not found for this IP".to_owned()))?;
        settings.enabled = enabled;
        Ok(settings)
    }
    async fn fetch_pending_dns_events(
        &self,
        _device_id: &str,
        _after_id: i64,
        _limit: i64,
    ) -> Result<Vec<wardnet_common::api::DnsEventItem>, AppError> {
        unimplemented!()
    }
    async fn ack_dns_events(&self, _device_id: &str, _up_to_id: i64) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn list_capture_enabled_device_ids(&self) -> Result<Vec<String>, AppError> {
        unimplemented!()
    }
    async fn get_device_capture_settings(
        &self,
        _device_id: &str,
    ) -> Result<Option<(bool, i64, i64)>, AppError> {
        unimplemented!()
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn sample_settings() -> DnsCaptureSettingsResponse {
    DnsCaptureSettingsResponse {
        enabled: true,
        cap_count: 500,
        cap_days: 14,
        row_count: 42,
        size_bytes: 1024,
    }
}

fn build_state(device_svc: impl DeviceService + 'static) -> AppState {
    AppState::new(
        Arc::new(MockAuthService),
        Arc::new(crate::tests::stubs::StubBackupService),
        Arc::new(device_svc),
        Arc::new(StubDhcpService),
        Arc::new(StubDnsService),
        Arc::new(StubDnsFilterService),
        Arc::new(crate::tests::stubs::StubDnsLocalService),
        Arc::new(crate::tests::stubs::StubDdnsService),
        Arc::new(crate::tests::stubs::StubTlsService),
        Arc::new(crate::tests::stubs::StubDiscoveryService),
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

fn dns_capture_router(state: AppState) -> Router {
    Router::new()
        .route(
            "/api/devices/{id}/dns-capture",
            get(crate::api::dns_capture::get_dns_capture_settings)
                .patch(crate::api::dns_capture::update_dns_capture_settings),
        )
        .route(
            "/api/devices/me/dns-capture",
            axum::routing::patch(crate::api::dns_capture::set_my_dns_capture),
        )
        .with_state(state)
}

fn client_connect_info() -> axum::extract::ConnectInfo<std::net::SocketAddr> {
    axum::extract::ConnectInfo(std::net::SocketAddr::new(
        std::net::IpAddr::V4(std::net::Ipv4Addr::new(192, 168, 1, 100)),
        12345,
    ))
}

/// Self-service (device-IP) PATCH — no auth cookie, `ConnectInfo` extension set.
async fn patch_me(app: Router, json_body: &str) -> (StatusCode, serde_json::Value) {
    let resp = app
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/devices/me/dns-capture")
                .header("Content-Type", "application/json")
                .extension(client_connect_info())
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

/// Authenticated GET request.
async fn get_json(app: Router, uri: &str) -> (StatusCode, serde_json::Value) {
    let resp = app
        .oneshot(
            Request::builder()
                .uri(uri)
                .header("Cookie", "wardnet_session=valid-token")
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

/// Authenticated PATCH request with a JSON body.
async fn patch_json(app: Router, uri: &str, json_body: &str) -> (StatusCode, serde_json::Value) {
    let resp = app
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(uri)
                .header("Content-Type", "application/json")
                .header("Cookie", "wardnet_session=valid-token")
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
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn get_returns_200_with_settings() {
    let settings = sample_settings();
    let state = build_state(MockDnsDeviceService::found(settings));
    let app = dns_capture_router(state);

    let (status, json) = get_json(
        app,
        "/api/devices/00000000-0000-0000-0000-000000000001/dns-capture",
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["enabled"], true);
    assert_eq!(json["cap_count"], 500);
    assert_eq!(json["cap_days"], 14);
    assert_eq!(json["row_count"], 42);
    assert_eq!(json["size_bytes"], 1024);
}

#[tokio::test]
async fn get_returns_404_for_unknown_device() {
    let state = build_state(MockDnsDeviceService::get_not_found());
    let app = dns_capture_router(state);

    let (status, json) = get_json(
        app,
        "/api/devices/00000000-0000-0000-0000-000000000099/dns-capture",
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(json["error"], "not found");
}

#[tokio::test]
async fn patch_updates_and_returns_200() {
    // After update, get returns the "updated" settings.
    let updated_settings = DnsCaptureSettingsResponse {
        enabled: true,
        cap_count: 200,
        cap_days: 30,
        row_count: 0,
        size_bytes: 0,
    };
    let state = build_state(MockDnsDeviceService::found(updated_settings));
    let app = dns_capture_router(state);

    let (status, json) = patch_json(
        app,
        "/api/devices/00000000-0000-0000-0000-000000000001/dns-capture",
        r#"{"enabled":true}"#,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["enabled"], true);
    assert_eq!(json["cap_count"], 200);
    assert_eq!(json["cap_days"], 30);
}

#[tokio::test]
async fn patch_returns_404_for_unknown_device() {
    let state = build_state(MockDnsDeviceService::update_not_found());
    let app = dns_capture_router(state);

    let (status, json) = patch_json(
        app,
        "/api/devices/00000000-0000-0000-0000-000000000099/dns-capture",
        r#"{"enabled":false}"#,
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(json["error"], "not found");
}

#[tokio::test]
async fn set_my_capture_returns_200_and_flips_enabled() {
    // sample_settings has enabled=true; the self-service PATCH flips it off and
    // leaves the admin-owned caps untouched.
    let state = build_state(MockDnsDeviceService::found(sample_settings()));
    let app = dns_capture_router(state);

    let (status, json) = patch_me(app, r#"{"enabled":false}"#).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["enabled"], false);
    assert_eq!(json["cap_count"], 500);
    assert_eq!(json["cap_days"], 14);
}

#[tokio::test]
async fn set_my_capture_returns_404_for_unknown_ip() {
    let state = build_state(MockDnsDeviceService::get_not_found());
    let app = dns_capture_router(state);

    let (status, json) = patch_me(app, r#"{"enabled":true}"#).await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(json["error"], "not found");
}
