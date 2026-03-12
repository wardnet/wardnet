//! Tests for the DHCP API endpoints.
//!
//! DHCP API handler tests will be expanded when the full DHCP runner is
//! implemented. For now we verify that routes are wired correctly and that
//! the admin-auth guard rejects unauthenticated requests.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use axum::Router;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use axum::routing::{delete, get, post};
use tower::ServiceExt;
use uuid::Uuid;
use wardnet_types::api::{
    CreateDhcpReservationRequest, CreateDhcpReservationResponse, DeleteDhcpReservationResponse,
    DhcpConfigResponse, DhcpStatusResponse, ListDhcpLeasesResponse, ListDhcpReservationsResponse,
    RevokeDhcpLeaseResponse, ToggleDhcpRequest, UpdateDhcpConfigRequest,
};
use wardnet_types::dhcp::{DhcpConfig, DhcpLease, DhcpReservation};

use crate::config::Config;
use crate::error::AppError;
use crate::service::auth::LoginResult;
use crate::service::{AuthService, DhcpService};
use crate::state::AppState;
use crate::tests::stubs::{
    StubDeviceService, StubDiscoveryService, StubEventPublisher, StubProviderService,
    StubRoutingService, StubSystemService, StubTunnelService,
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

/// Mock DHCP service that returns default config and empty collections.
struct MockDhcpService;

#[async_trait]
impl DhcpService for MockDhcpService {
    async fn get_config(&self) -> Result<DhcpConfigResponse, AppError> {
        Ok(DhcpConfigResponse {
            config: DhcpConfig {
                enabled: false,
                pool_start: "192.168.1.100".parse().unwrap(),
                pool_end: "192.168.1.200".parse().unwrap(),
                subnet_mask: "255.255.255.0".parse().unwrap(),
                upstream_dns: vec!["1.1.1.1".parse().unwrap(), "8.8.8.8".parse().unwrap()],
                lease_duration_secs: 86400,
                router_ip: None,
            },
        })
    }

    async fn update_config(
        &self,
        _req: UpdateDhcpConfigRequest,
    ) -> Result<DhcpConfigResponse, AppError> {
        self.get_config().await
    }

    async fn toggle(&self, _req: ToggleDhcpRequest) -> Result<DhcpConfigResponse, AppError> {
        self.get_config().await
    }

    async fn list_leases(&self) -> Result<ListDhcpLeasesResponse, AppError> {
        Ok(ListDhcpLeasesResponse { leases: vec![] })
    }

    async fn revoke_lease(&self, id: Uuid) -> Result<RevokeDhcpLeaseResponse, AppError> {
        Err(AppError::NotFound(format!("lease {id} not found")))
    }

    async fn list_reservations(&self) -> Result<ListDhcpReservationsResponse, AppError> {
        Ok(ListDhcpReservationsResponse {
            reservations: vec![],
        })
    }

    async fn create_reservation(
        &self,
        req: CreateDhcpReservationRequest,
    ) -> Result<CreateDhcpReservationResponse, AppError> {
        Ok(CreateDhcpReservationResponse {
            reservation: DhcpReservation {
                id: Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap(),
                mac_address: req.mac_address,
                ip_address: req.ip_address.parse().unwrap(),
                hostname: req.hostname,
                description: req.description,
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            },
            message: "reservation created".to_owned(),
        })
    }

    async fn delete_reservation(
        &self,
        id: Uuid,
    ) -> Result<DeleteDhcpReservationResponse, AppError> {
        Err(AppError::NotFound(format!("reservation {id} not found")))
    }

    async fn status(&self) -> Result<DhcpStatusResponse, AppError> {
        Ok(DhcpStatusResponse {
            enabled: false,
            running: false,
            active_lease_count: 0,
            pool_total: 101,
            pool_used: 0,
        })
    }

    async fn assign_lease(
        &self,
        _mac: &str,
        _hostname: Option<&str>,
    ) -> Result<DhcpLease, AppError> {
        unimplemented!()
    }
    async fn renew_lease(&self, _mac: &str) -> Result<DhcpLease, AppError> {
        unimplemented!()
    }
    async fn release_lease(&self, _mac: &str) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn cleanup_expired(&self) -> Result<u64, AppError> {
        Ok(0)
    }
    async fn get_dhcp_config(&self) -> Result<DhcpConfig, AppError> {
        Ok(DhcpConfig {
            enabled: false,
            pool_start: "192.168.1.100".parse().unwrap(),
            pool_end: "192.168.1.200".parse().unwrap(),
            subnet_mask: "255.255.255.0".parse().unwrap(),
            upstream_dns: vec!["1.1.1.1".parse().unwrap()],
            lease_duration_secs: 86400,
            router_ip: None,
        })
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn connect_info() -> ConnectInfo<SocketAddr> {
    ConnectInfo(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 12345))
}

fn build_state(dhcp_svc: impl DhcpService + 'static) -> AppState {
    AppState::new(
        Arc::new(MockAuthService),
        Arc::new(StubDeviceService),
        Arc::new(dhcp_svc),
        Arc::new(StubDiscoveryService),
        Arc::new(StubProviderService),
        Arc::new(StubRoutingService),
        Arc::new(StubSystemService),
        Arc::new(StubTunnelService),
        Arc::new(StubEventPublisher),
        Config::default(),
        Instant::now(),
    )
}

fn dhcp_router(state: AppState) -> Router {
    Router::new()
        .route(
            "/api/dhcp/config",
            get(crate::api::dhcp::get_config).put(crate::api::dhcp::update_config),
        )
        .route("/api/dhcp/config/toggle", post(crate::api::dhcp::toggle))
        .route("/api/dhcp/leases", get(crate::api::dhcp::list_leases))
        .route(
            "/api/dhcp/leases/{id}",
            delete(crate::api::dhcp::revoke_lease),
        )
        .route(
            "/api/dhcp/reservations",
            get(crate::api::dhcp::list_reservations).post(crate::api::dhcp::create_reservation),
        )
        .route(
            "/api/dhcp/reservations/{id}",
            delete(crate::api::dhcp::delete_reservation),
        )
        .route("/api/dhcp/status", get(crate::api::dhcp::status))
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

/// Send an authenticated DELETE request.
async fn delete_json(app: Router, uri: &str) -> (StatusCode, serde_json::Value) {
    let resp = app
        .oneshot(
            Request::builder()
                .method("DELETE")
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn get_config_returns_ok() {
    let state = build_state(MockDhcpService);
    let app = dhcp_router(state);

    let (status, json) = get_json(app, "/api/dhcp/config").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["config"]["pool_start"], "192.168.1.100");
    assert_eq!(json["config"]["pool_end"], "192.168.1.200");
    assert!(!json["config"]["enabled"].as_bool().unwrap());
}

#[tokio::test]
async fn get_config_unauthorized_without_session() {
    let state = build_state(MockDhcpService);
    let app = dhcp_router(state);

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/dhcp/config")
                .extension(connect_info())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn status_returns_pool_info() {
    let state = build_state(MockDhcpService);
    let app = dhcp_router(state);

    let (status, json) = get_json(app, "/api/dhcp/status").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["pool_total"], 101);
    assert_eq!(json["active_lease_count"], 0);
}

#[tokio::test]
async fn list_leases_returns_empty() {
    let state = build_state(MockDhcpService);
    let app = dhcp_router(state);

    let (status, json) = get_json(app, "/api/dhcp/leases").await;
    assert_eq!(status, StatusCode::OK);
    assert!(json["leases"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn list_reservations_returns_empty() {
    let state = build_state(MockDhcpService);
    let app = dhcp_router(state);

    let (status, json) = get_json(app, "/api/dhcp/reservations").await;
    assert_eq!(status, StatusCode::OK);
    assert!(json["reservations"].as_array().unwrap().is_empty());
}

// ---------------------------------------------------------------------------
// PUT /api/dhcp/config
// ---------------------------------------------------------------------------

#[tokio::test]
async fn update_config_returns_ok() {
    let state = build_state(MockDhcpService);
    let app = dhcp_router(state);

    let body = serde_json::json!({
        "pool_start": "10.0.0.100",
        "pool_end": "10.0.0.200",
        "subnet_mask": "255.255.255.0",
        "upstream_dns": ["1.1.1.1"],
        "lease_duration_secs": 3600,
        "router_ip": null
    });

    let (status, json) = put_json(app, "/api/dhcp/config", &body.to_string()).await;
    assert_eq!(status, StatusCode::OK);
    // The mock returns the default config regardless of input.
    assert_eq!(json["config"]["pool_start"], "192.168.1.100");
    assert!(!json["config"]["enabled"].as_bool().unwrap());
}

#[tokio::test]
async fn update_config_unauthorized_without_session() {
    let state = build_state(MockDhcpService);
    let app = dhcp_router(state);

    let resp = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/dhcp/config")
                .header("Content-Type", "application/json")
                .extension(connect_info())
                .body(Body::from(r#"{"pool_start":"10.0.0.1","pool_end":"10.0.0.200","subnet_mask":"255.255.255.0","upstream_dns":[],"lease_duration_secs":3600}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn update_config_bad_json_returns_error() {
    let state = build_state(MockDhcpService);
    let app = dhcp_router(state);

    let resp = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/dhcp/config")
                .header("Content-Type", "application/json")
                .header("Cookie", "wardnet_session=valid-token")
                .extension(connect_info())
                .body(Body::from("not json"))
                .unwrap(),
        )
        .await
        .unwrap();

    let status = resp.status();
    assert!(
        status == StatusCode::BAD_REQUEST || status == StatusCode::UNPROCESSABLE_ENTITY,
        "expected 400 or 422, got {status}"
    );
}

#[tokio::test]
async fn update_config_missing_fields_returns_error() {
    let state = build_state(MockDhcpService);
    let app = dhcp_router(state);

    // Missing required fields like pool_end, subnet_mask, etc.
    let body = serde_json::json!({"pool_start": "10.0.0.1"});

    let resp = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/dhcp/config")
                .header("Content-Type", "application/json")
                .header("Cookie", "wardnet_session=valid-token")
                .extension(connect_info())
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    let status = resp.status();
    assert!(
        status == StatusCode::BAD_REQUEST || status == StatusCode::UNPROCESSABLE_ENTITY,
        "expected 400 or 422, got {status}"
    );
}

// ---------------------------------------------------------------------------
// POST /api/dhcp/config/toggle
// ---------------------------------------------------------------------------

#[tokio::test]
async fn toggle_enable_returns_ok() {
    let state = build_state(MockDhcpService);
    let app = dhcp_router(state);

    let body = serde_json::json!({"enabled": true});

    let (status, json) = post_json(app, "/api/dhcp/config/toggle", &body.to_string()).await;
    assert_eq!(status, StatusCode::OK);
    // Mock always returns the same config; we just verify the handler wired correctly.
    assert!(json["config"].is_object());
}

#[tokio::test]
async fn toggle_disable_returns_ok() {
    let state = build_state(MockDhcpService);
    let app = dhcp_router(state);

    let body = serde_json::json!({"enabled": false});

    let (status, json) = post_json(app, "/api/dhcp/config/toggle", &body.to_string()).await;
    assert_eq!(status, StatusCode::OK);
    assert!(json["config"].is_object());
}

#[tokio::test]
async fn toggle_unauthorized_without_session() {
    let state = build_state(MockDhcpService);
    let app = dhcp_router(state);

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/dhcp/config/toggle")
                .header("Content-Type", "application/json")
                .extension(connect_info())
                .body(Body::from(r#"{"enabled":true}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn toggle_bad_json_returns_error() {
    let state = build_state(MockDhcpService);
    let app = dhcp_router(state);

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/dhcp/config/toggle")
                .header("Content-Type", "application/json")
                .header("Cookie", "wardnet_session=valid-token")
                .extension(connect_info())
                .body(Body::from("not json"))
                .unwrap(),
        )
        .await
        .unwrap();

    let status = resp.status();
    assert!(
        status == StatusCode::BAD_REQUEST || status == StatusCode::UNPROCESSABLE_ENTITY,
        "expected 400 or 422, got {status}"
    );
}

// ---------------------------------------------------------------------------
// DELETE /api/dhcp/leases/:id
// ---------------------------------------------------------------------------

#[tokio::test]
async fn revoke_lease_not_found() {
    let state = build_state(MockDhcpService);
    let app = dhcp_router(state);

    let (status, json) =
        delete_json(app, "/api/dhcp/leases/00000000-0000-0000-0000-000000000042").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(json["error"], "not found");
}

#[tokio::test]
async fn revoke_lease_unauthorized_without_session() {
    let state = build_state(MockDhcpService);
    let app = dhcp_router(state);

    let resp = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/dhcp/leases/00000000-0000-0000-0000-000000000042")
                .extension(connect_info())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

// ---------------------------------------------------------------------------
// POST /api/dhcp/reservations
// ---------------------------------------------------------------------------

#[tokio::test]
async fn create_reservation_returns_created() {
    let state = build_state(MockDhcpService);
    let app = dhcp_router(state);

    let body = serde_json::json!({
        "mac_address": "AA:BB:CC:DD:EE:FF",
        "ip_address": "192.168.1.50",
        "hostname": "my-server",
        "description": "Home server"
    });

    let (status, json) = post_json(app, "/api/dhcp/reservations", &body.to_string()).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(json["reservation"]["mac_address"], "AA:BB:CC:DD:EE:FF");
    assert_eq!(json["reservation"]["ip_address"], "192.168.1.50");
    assert_eq!(json["reservation"]["hostname"], "my-server");
    assert_eq!(json["reservation"]["description"], "Home server");
    assert_eq!(json["message"], "reservation created");
}

#[tokio::test]
async fn create_reservation_without_optional_fields() {
    let state = build_state(MockDhcpService);
    let app = dhcp_router(state);

    let body = serde_json::json!({
        "mac_address": "11:22:33:44:55:66",
        "ip_address": "192.168.1.51"
    });

    let (status, json) = post_json(app, "/api/dhcp/reservations", &body.to_string()).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(json["reservation"]["mac_address"], "11:22:33:44:55:66");
    assert!(json["reservation"]["hostname"].is_null());
    assert!(json["reservation"]["description"].is_null());
}

#[tokio::test]
async fn create_reservation_unauthorized_without_session() {
    let state = build_state(MockDhcpService);
    let app = dhcp_router(state);

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/dhcp/reservations")
                .header("Content-Type", "application/json")
                .extension(connect_info())
                .body(Body::from(
                    r#"{"mac_address":"AA:BB:CC:DD:EE:FF","ip_address":"192.168.1.50"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn create_reservation_bad_json_returns_error() {
    let state = build_state(MockDhcpService);
    let app = dhcp_router(state);

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/dhcp/reservations")
                .header("Content-Type", "application/json")
                .header("Cookie", "wardnet_session=valid-token")
                .extension(connect_info())
                .body(Body::from("not json"))
                .unwrap(),
        )
        .await
        .unwrap();

    let status = resp.status();
    assert!(
        status == StatusCode::BAD_REQUEST || status == StatusCode::UNPROCESSABLE_ENTITY,
        "expected 400 or 422, got {status}"
    );
}

#[tokio::test]
async fn create_reservation_missing_fields_returns_error() {
    let state = build_state(MockDhcpService);
    let app = dhcp_router(state);

    // Missing required `ip_address` field.
    let body = serde_json::json!({"mac_address": "AA:BB:CC:DD:EE:FF"});

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/dhcp/reservations")
                .header("Content-Type", "application/json")
                .header("Cookie", "wardnet_session=valid-token")
                .extension(connect_info())
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    let status = resp.status();
    assert!(
        status == StatusCode::BAD_REQUEST || status == StatusCode::UNPROCESSABLE_ENTITY,
        "expected 400 or 422, got {status}"
    );
}

// ---------------------------------------------------------------------------
// DELETE /api/dhcp/reservations/:id
// ---------------------------------------------------------------------------

#[tokio::test]
async fn delete_reservation_not_found() {
    let state = build_state(MockDhcpService);
    let app = dhcp_router(state);

    let (status, json) = delete_json(
        app,
        "/api/dhcp/reservations/00000000-0000-0000-0000-000000000077",
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(json["error"], "not found");
}

#[tokio::test]
async fn delete_reservation_unauthorized_without_session() {
    let state = build_state(MockDhcpService);
    let app = dhcp_router(state);

    let resp = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/dhcp/reservations/00000000-0000-0000-0000-000000000077")
                .extension(connect_info())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}
