//! Tests for the system status API endpoint (GET /api/system/status).

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use async_trait::async_trait;
use axum::Router;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use axum::routing::get;
use tower::ServiceExt;
use wardnet_common::api::{
    LastShutdownState, LastShutdownStatus, SetDefaultPolicyRequest, SystemStatusResponse,
};

use crate::state::AppState;
use crate::tests::stubs::{
    StubDeviceService, StubDhcpServer, StubDhcpService, StubDiscoveryService, StubDnsFilterService,
    StubDnsLocalService, StubDnsServer, StubDnsService, StubEventPublisher, StubLogService,
    StubNetworkZoneService, StubProviderService, StubRoutingService, StubTunnelService,
};
use uuid::Uuid;
use wardnet_common::auth::{AuthenticatedUser, UserRole};
use wardnet_common::routing::RoutingTarget;
use wardnet_test_support::principal;
use wardnetd_services::LogService;
use wardnetd_services::RoutingService;
use wardnetd_services::auth::service::LoginResult;
use wardnetd_services::auth::{CurrentUser, LoginAttempt};
use wardnetd_services::error::AppError;
use wardnetd_services::{AuthService, SystemService};

// ---------------------------------------------------------------------------
// Mock services
// ---------------------------------------------------------------------------

/// Mock auth service that always validates the session (so admin routes pass).
struct AlwaysAuthService {
    admin_id: Uuid,
}

#[async_trait]
impl AuthService for AlwaysAuthService {
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
        Ok(Some(principal::admin(self.admin_id)))
    }
    async fn validate_api_key(&self, _key: &str) -> Result<Option<AuthenticatedUser>, AppError> {
        Ok(Some(principal::admin(self.admin_id)))
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

/// Mock auth service that always rejects.
struct NeverAuthService;
#[async_trait]
impl AuthService for NeverAuthService {
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
        Ok(None)
    }
    async fn validate_api_key(&self, _key: &str) -> Result<Option<AuthenticatedUser>, AppError> {
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

/// Mock system service returning a preconfigured response.
struct MockSystemService {
    response: Result<SystemStatusResponse, AppError>,
}

#[async_trait]
impl SystemService for MockSystemService {
    fn version(&self) -> &'static str {
        "0.1.0-test"
    }
    fn uptime(&self) -> std::time::Duration {
        std::time::Duration::from_secs(42)
    }
    async fn status(&self) -> Result<SystemStatusResponse, AppError> {
        match &self.response {
            Ok(r) => Ok(SystemStatusResponse {
                version: r.version.clone(),
                release_version: r.release_version.clone(),
                uptime_seconds: r.uptime_seconds,
                device_count: r.device_count,
                tunnel_count: r.tunnel_count,
                tunnel_active_count: r.tunnel_active_count,
                db_size_bytes: r.db_size_bytes,
                cpu_usage_percent: r.cpu_usage_percent,
                memory_used_bytes: r.memory_used_bytes,
                memory_total_bytes: r.memory_total_bytes,
                disk_free_bytes: r.disk_free_bytes,
                disk_total_bytes: r.disk_total_bytes,
                last_shutdown: r.last_shutdown.clone(),
            }),
            Err(_) => Err(AppError::Internal(anyhow::anyhow!("mock error"))),
        }
    }
    async fn request_restart(&self) -> Result<(), AppError> {
        Ok(())
    }
    async fn request_reboot(&self) -> Result<(), AppError> {
        Ok(())
    }
    async fn request_shutdown(&self) -> Result<(), AppError> {
        Ok(())
    }
    async fn network_status(&self) -> Result<wardnet_common::api::NetworkStatusResponse, AppError> {
        unimplemented!()
    }
    async fn discover_gateway_mac(
        &self,
        _request: wardnet_common::api::DiscoverGatewayMacRequest,
    ) -> Result<wardnet_common::api::DiscoverGatewayMacResponse, AppError> {
        unimplemented!()
    }
    async fn dhcp_self_probe(
        &self,
    ) -> Result<wardnet_common::api::DhcpSelfProbeResponse, AppError> {
        unimplemented!()
    }
    async fn record_heartbeat(&self) -> Result<(), AppError> {
        Ok(())
    }
    async fn record_graceful_shutdown(&self) -> Result<(), AppError> {
        Ok(())
    }
    async fn acknowledge_last_shutdown(&self) -> Result<(), AppError> {
        Ok(())
    }
}

/// Mock routing service that returns canned responses for the
/// `default_policy` / `set_default_policy` pair. All other trait
/// methods delegate to [`StubRoutingService`] so the production
/// `set_default_policy` re-apply walk doesn't get exercised here —
/// these tests only care about the API-handler surface.
struct MockRoutingService {
    inner: StubRoutingService,
    /// Result returned by `default_policy()`.
    get_response: Result<String, AppError>,
    /// Result returned by `set_default_policy()`.
    set_response: Result<(), AppError>,
}

impl MockRoutingService {
    fn new(get_response: Result<String, AppError>, set_response: Result<(), AppError>) -> Self {
        Self {
            inner: StubRoutingService,
            get_response,
            set_response,
        }
    }

    /// Convenience for tests that don't care about errors from either call.
    fn ok(policy: &str) -> Self {
        Self::new(Ok(policy.to_owned()), Ok(()))
    }
}

#[async_trait]
#[allow(clippy::similar_names)] // matches the RoutingService trait's own argument names.
impl RoutingService for MockRoutingService {
    async fn set_switchback_targets(
        &self,
        device_id: Uuid,
        device_ip: String,
        target_cidrs: Vec<String>,
    ) -> Result<(), AppError> {
        self.inner
            .set_switchback_targets(device_id, device_ip, target_cidrs)
            .await
    }
    async fn route_resolved_domain(
        &self,
        device_ip: &str,
        resolved_ips: &[std::net::IpAddr],
        target: &wardnet_common::routing_profile::DomainRoutingTarget,
        ttl_secs: u32,
    ) -> Result<(), AppError> {
        self.inner
            .route_resolved_domain(device_ip, resolved_ips, target, ttl_secs)
            .await
    }
    async fn gc_domain_routes(&self) -> Result<(), AppError> {
        self.inner.gc_domain_routes().await
    }
    async fn apply_rule(
        &self,
        device_id: Uuid,
        device_ip: &str,
        target: &RoutingTarget,
    ) -> Result<(), AppError> {
        self.inner.apply_rule(device_id, device_ip, target).await
    }
    async fn remove_device_routes(&self, device_id: Uuid, device_ip: &str) -> Result<(), AppError> {
        self.inner.remove_device_routes(device_id, device_ip).await
    }
    async fn handle_ip_change(
        &self,
        device_id: Uuid,
        old_ip: &str,
        new_ip: &str,
    ) -> Result<(), AppError> {
        self.inner.handle_ip_change(device_id, old_ip, new_ip).await
    }
    async fn handle_tunnel_down(&self, tunnel_id: Uuid) -> Result<(), AppError> {
        self.inner.handle_tunnel_down(tunnel_id).await
    }
    async fn handle_tunnel_up(&self, tunnel_id: Uuid) -> Result<(), AppError> {
        self.inner.handle_tunnel_up(tunnel_id).await
    }
    async fn reconcile(&self) -> Result<(), AppError> {
        self.inner.reconcile().await
    }
    async fn handle_route_table_lost(&self, table: u32) -> Result<(), AppError> {
        self.inner.handle_route_table_lost(table).await
    }
    async fn devices_using_tunnel(&self, tunnel_id: Uuid) -> Result<Vec<Uuid>, AppError> {
        self.inner.devices_using_tunnel(tunnel_id).await
    }
    async fn apply_rule_for_device(
        &self,
        device_id: Uuid,
        target: &RoutingTarget,
    ) -> Result<(), AppError> {
        self.inner.apply_rule_for_device(device_id, target).await
    }
    async fn apply_rule_for_discovered_device(
        &self,
        device_id: Uuid,
        ip: &str,
    ) -> Result<(), AppError> {
        self.inner
            .apply_rule_for_discovered_device(device_id, ip)
            .await
    }
    async fn set_default_policy(&self, _policy: &str) -> Result<(), AppError> {
        match &self.set_response {
            Ok(()) => Ok(()),
            Err(e) => Err(clone_app_error(e)),
        }
    }
    async fn default_policy(&self) -> Result<String, AppError> {
        match &self.get_response {
            Ok(s) => Ok(s.clone()),
            Err(e) => Err(clone_app_error(e)),
        }
    }
    async fn handle_default_policy_changed(&self) -> Result<(), AppError> {
        self.inner.handle_default_policy_changed().await
    }
    fn dns_upstream_snapshot(
        &self,
    ) -> std::sync::Arc<
        arc_swap::ArcSwap<
            std::collections::HashMap<std::net::IpAddr, wardnet_common::dns::UpstreamId>,
        >,
    > {
        self.inner.dns_upstream_snapshot()
    }
    async fn rebuild_dns_upstream_snapshot(&self) -> Result<(), AppError> {
        self.inner.rebuild_dns_upstream_snapshot().await
    }
    fn dns_device_upstream_snapshot(
        &self,
    ) -> std::sync::Arc<
        arc_swap::ArcSwap<std::collections::HashMap<uuid::Uuid, wardnet_common::dns::UpstreamId>>,
    > {
        self.inner.dns_device_upstream_snapshot()
    }
    async fn rebuild_dns_device_upstream_snapshot(&self) -> Result<(), AppError> {
        self.inner.rebuild_dns_device_upstream_snapshot().await
    }
}

/// Re-create an [`AppError`] of the same variant as `err` so a mock
/// can return a fresh `Err` from a borrowed source. Internal/Database
/// errors collapse into a generic `Internal` because [`anyhow::Error`]
/// is not cloneable; the variant alone determines the HTTP status,
/// which is what the API tests assert against.
fn clone_app_error(err: &AppError) -> AppError {
    match err {
        AppError::BadRequest(s) => AppError::BadRequest(s.clone()),
        AppError::NotFound(s) => AppError::NotFound(s.clone()),
        AppError::Conflict(s) => AppError::Conflict(s.clone()),
        AppError::Forbidden(s) => AppError::Forbidden(s.clone()),
        AppError::Unauthorized(s) => AppError::Unauthorized(s.clone()),
        AppError::UpstreamUnavailable(s) => AppError::UpstreamUnavailable(s.clone()),
        AppError::PreconditionFailed(s) => AppError::PreconditionFailed(s.clone()),
        AppError::TooManyRequests {
            message,
            retry_after_seconds,
        } => AppError::TooManyRequests {
            message: message.clone(),
            retry_after_seconds: *retry_after_seconds,
        },
        AppError::Internal(_) | AppError::Database(_) => {
            AppError::Internal(anyhow::anyhow!("mock internal"))
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_state(auth: impl AuthService + 'static, system: impl SystemService + 'static) -> AppState {
    make_state_with_routing(auth, system, StubRoutingService)
}

fn make_state_with_routing(
    auth: impl AuthService + 'static,
    system: impl SystemService + 'static,
    routing: impl RoutingService + 'static,
) -> AppState {
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
        Arc::new(routing),
        Arc::new(StubNetworkZoneService),
        Arc::new(system),
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

fn system_app(state: AppState) -> Router {
    Router::new()
        .route("/api/system/status", get(crate::api::system::status))
        .with_state(state)
}

fn system_app_full(state: AppState) -> Router {
    Router::new()
        .route("/api/system/status", get(crate::api::system::status))
        .route("/api/system/errors", get(crate::api::system::recent_errors))
        .route(
            "/api/system/logs/download",
            get(crate::api::system::download_logs),
        )
        .route(
            "/api/system/restart",
            axum::routing::post(crate::api::system::restart),
        )
        .route(
            "/api/system/reboot",
            axum::routing::post(crate::api::system::reboot),
        )
        .route(
            "/api/system/shutdown",
            axum::routing::post(crate::api::system::shutdown),
        )
        .route(
            "/api/system/default-policy",
            get(crate::api::system::get_default_policy).put(crate::api::system::set_default_policy),
        )
        .route(
            "/api/system/shutdown/acknowledge",
            axum::routing::post(crate::api::system::acknowledge_shutdown),
        )
        .with_state(state)
}

fn default_last_shutdown() -> LastShutdownStatus {
    LastShutdownStatus {
        state: LastShutdownState::Unknown,
        at: None,
        acknowledged_at: None,
    }
}

fn default_status() -> SystemStatusResponse {
    SystemStatusResponse {
        version: "0.0.1".to_owned(),
        release_version: "0.0.1".to_owned(),
        uptime_seconds: 0,
        device_count: 0,
        tunnel_count: 0,
        tunnel_active_count: 0,
        db_size_bytes: 0,
        cpu_usage_percent: 0.0,
        memory_used_bytes: 0,
        memory_total_bytes: 0,
        disk_free_bytes: 0,
        disk_total_bytes: 0,
        last_shutdown: default_last_shutdown(),
    }
}

fn connect_info_ext() -> ConnectInfo<SocketAddr> {
    ConnectInfo(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 1234))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn status_returns_200_with_correct_json() {
    let admin_id = Uuid::new_v4();
    let state = make_state(
        AlwaysAuthService { admin_id },
        MockSystemService {
            response: Ok(SystemStatusResponse {
                version: "1.2.3".to_owned(),
                release_version: "1.2.3".to_owned(),
                uptime_seconds: 3600,
                device_count: 10,
                tunnel_count: 3,
                tunnel_active_count: 1,
                db_size_bytes: 4096,
                cpu_usage_percent: 25.5,
                memory_used_bytes: 1_073_741_824,
                memory_total_bytes: 4_294_967_296,
                disk_free_bytes: 1_000_000_000,
                disk_total_bytes: 32_000_000_000,
                last_shutdown: default_last_shutdown(),
            }),
        },
    );

    let app = system_app(state);
    let req = Request::builder()
        .uri("/api/system/status")
        .header("Cookie", "wardnet_session=valid-token")
        .extension(connect_info_ext())
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["version"], "1.2.3");
    assert_eq!(json["uptime_seconds"], 3600);
    assert_eq!(json["device_count"], 10);
    assert_eq!(json["tunnel_count"], 3);
    assert_eq!(json["db_size_bytes"], 4096);
    assert_eq!(json["cpu_usage_percent"], 25.5);
    assert_eq!(json["memory_used_bytes"], 1_073_741_824_u64);
    assert_eq!(json["memory_total_bytes"], 4_294_967_296_u64);
}

#[tokio::test]
async fn status_requires_authentication() {
    let state = make_state(
        NeverAuthService,
        MockSystemService {
            response: Ok(SystemStatusResponse {
                version: "1.0.0".to_owned(),
                release_version: "1.0.0".to_owned(),
                uptime_seconds: 0,
                device_count: 0,
                tunnel_count: 0,
                tunnel_active_count: 0,
                db_size_bytes: 0,
                cpu_usage_percent: 0.0,
                memory_used_bytes: 0,
                memory_total_bytes: 0,
                disk_free_bytes: 0,
                disk_total_bytes: 0,
                last_shutdown: default_last_shutdown(),
            }),
        },
    );

    let app = system_app(state);
    let req = Request::builder()
        .uri("/api/system/status")
        .extension(connect_info_ext())
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn status_service_error_returns_500() {
    let admin_id = Uuid::new_v4();
    let state = make_state(
        AlwaysAuthService { admin_id },
        MockSystemService {
            response: Err(AppError::Internal(anyhow::anyhow!("db down"))),
        },
    );

    let app = system_app(state);
    let req = Request::builder()
        .uri("/api/system/status")
        .header("Authorization", "Bearer some-key")
        .extension(connect_info_ext())
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

// ---------------------------------------------------------------------------
// GET /api/system/errors
// ---------------------------------------------------------------------------

#[tokio::test]
async fn recent_errors_returns_empty_list() {
    let admin_id = Uuid::new_v4();
    let state = make_state(
        AlwaysAuthService { admin_id },
        MockSystemService {
            response: Ok(default_status()),
        },
    );

    let app = system_app_full(state);
    let req = Request::builder()
        .uri("/api/system/errors")
        .header("Cookie", "wardnet_session=valid-token")
        .extension(connect_info_ext())
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json["errors"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn recent_errors_returns_populated_errors() {
    use wardnetd_services::diagnostics::Diagnostic;
    use wardnetd_services::logging::component::BoxedLayer;
    use wardnetd_services::logging::service::LogFileInfo;
    use wardnetd_services::logging::stream::LogEntry;

    /// A mock `LogService` that returns canned error entries.
    struct MockLogServiceWithErrors;

    #[async_trait]
    impl LogService for MockLogServiceWithErrors {
        fn subscribe(&self) -> Result<tokio::sync::broadcast::Receiver<LogEntry>, AppError> {
            let (tx, rx) = tokio::sync::broadcast::channel(1);
            drop(tx);
            Ok(rx)
        }
        fn get_recent_errors(&self) -> Result<Vec<Diagnostic>, AppError> {
            use wardnet_common::event::WardnetEvent;
            Ok(vec![
                Diagnostic::from_event(&WardnetEvent::TunnelStartFailed {
                    tunnel_id: Uuid::new_v4(),
                    interface_name: "wg-work".to_owned(),
                    error: "boom".to_owned(),
                    timestamp: chrono::Utc::now(),
                })
                .unwrap(),
                Diagnostic::from_event(&WardnetEvent::DhcpConflictDetected {
                    mac: "aa:bb:cc:dd:ee:ff".to_owned(),
                    ip: "192.168.1.2".to_owned(),
                    details: "careful".to_owned(),
                    timestamp: chrono::Utc::now(),
                })
                .unwrap(),
            ])
        }
        async fn list_log_files(&self) -> Result<Vec<LogFileInfo>, AppError> {
            Ok(Vec::new())
        }
        async fn download_log_file(&self, _name: Option<&str>) -> Result<String, AppError> {
            Ok(String::new())
        }
        fn tracing_layers(&self) -> Vec<BoxedLayer> {
            Vec::new()
        }
        fn start_all(&self) {}
        fn stop_all(&self) {}
    }

    let admin_id = Uuid::new_v4();

    let state = AppState::new(
        Arc::new(AlwaysAuthService { admin_id }),
        Arc::new(crate::tests::stubs::StubBackupService),
        Arc::new(StubDeviceService),
        Arc::new(StubDhcpService),
        Arc::new(StubDnsService),
        Arc::new(StubDnsFilterService),
        Arc::new(StubDnsLocalService),
        Arc::new(crate::tests::stubs::StubDdnsService),
        Arc::new(crate::tests::stubs::StubTlsService),
        Arc::new(StubDiscoveryService),
        Arc::new(MockLogServiceWithErrors) as Arc<dyn LogService>,
        Arc::new(StubProviderService),
        Arc::new(StubRoutingService),
        Arc::new(StubNetworkZoneService),
        Arc::new(MockSystemService {
            response: Ok(default_status()),
        }),
        Arc::new(StubTunnelService),
        Arc::new(crate::tests::stubs::StubUpdateService),
        Arc::new(StubDhcpServer),
        Arc::new(StubDnsServer),
        Arc::new(StubEventPublisher),
        crate::tests::stubs::StubJobService::new_arc(),
        Arc::new(crate::tests::stubs::StubStatsService),
        Arc::new(crate::tests::stubs::StubRuleRequestService),
        Arc::new(crate::tests::stubs::StubZoneExceptionService),
    );

    let app = system_app_full(state);
    let req = Request::builder()
        .uri("/api/system/errors")
        .header("Cookie", "wardnet_session=valid-token")
        .extension(connect_info_ext())
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let errors = json["errors"].as_array().unwrap();
    assert_eq!(errors.len(), 2);
    assert_eq!(errors[0]["code"], "tunnel_start_failed");
    assert_eq!(errors[0]["severity"], "error");
    assert!(!errors[0]["hint"].as_str().unwrap().is_empty());
    assert_eq!(errors[1]["code"], "dhcp_conflict");
    assert_eq!(errors[1]["severity"], "warning");
}

#[tokio::test]
async fn recent_errors_requires_authentication() {
    let state = make_state(
        NeverAuthService,
        MockSystemService {
            response: Ok(default_status()),
        },
    );

    let app = system_app_full(state);
    let req = Request::builder()
        .uri("/api/system/errors")
        .extension(connect_info_ext())
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

// ---------------------------------------------------------------------------
// GET /api/system/logs/download
// ---------------------------------------------------------------------------

#[tokio::test]
async fn download_logs_requires_authentication() {
    let state = make_state(
        NeverAuthService,
        MockSystemService {
            response: Ok(default_status()),
        },
    );

    let app = system_app_full(state);
    let req = Request::builder()
        .uri("/api/system/logs/download")
        .extension(connect_info_ext())
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn download_logs_returns_text_when_log_exists() {
    use wardnetd_services::diagnostics::Diagnostic;
    use wardnetd_services::logging::component::BoxedLayer;
    use wardnetd_services::logging::service::LogFileInfo;
    use wardnetd_services::logging::stream::LogEntry;

    /// Mock `LogService` that returns formatted log content.
    struct MockLogServiceWithContent {
        content: String,
    }

    #[async_trait]
    impl LogService for MockLogServiceWithContent {
        fn subscribe(&self) -> Result<tokio::sync::broadcast::Receiver<LogEntry>, AppError> {
            let (tx, rx) = tokio::sync::broadcast::channel(1);
            drop(tx);
            Ok(rx)
        }
        fn get_recent_errors(&self) -> Result<Vec<Diagnostic>, AppError> {
            Ok(Vec::new())
        }
        async fn list_log_files(&self) -> Result<Vec<LogFileInfo>, AppError> {
            Ok(Vec::new())
        }
        async fn download_log_file(&self, _name: Option<&str>) -> Result<String, AppError> {
            Ok(self.content.clone())
        }
        fn tracing_layers(&self) -> Vec<BoxedLayer> {
            Vec::new()
        }
        fn start_all(&self) {}
        fn stop_all(&self) {}
    }

    let admin_id = Uuid::new_v4();

    let state = AppState::new(
        Arc::new(AlwaysAuthService { admin_id }),
        Arc::new(crate::tests::stubs::StubBackupService),
        Arc::new(StubDeviceService),
        Arc::new(StubDhcpService),
        Arc::new(StubDnsService),
        Arc::new(StubDnsFilterService),
        Arc::new(StubDnsLocalService),
        Arc::new(crate::tests::stubs::StubDdnsService),
        Arc::new(crate::tests::stubs::StubTlsService),
        Arc::new(StubDiscoveryService),
        Arc::new(MockLogServiceWithContent {
            content: "2026-04-13T00:00:00Z  INFO test hello world".to_owned(),
        }) as Arc<dyn LogService>,
        Arc::new(StubProviderService),
        Arc::new(StubRoutingService),
        Arc::new(StubNetworkZoneService),
        Arc::new(MockSystemService {
            response: Ok(default_status()),
        }),
        Arc::new(StubTunnelService),
        Arc::new(crate::tests::stubs::StubUpdateService),
        Arc::new(StubDhcpServer),
        Arc::new(StubDnsServer),
        Arc::new(StubEventPublisher),
        crate::tests::stubs::StubJobService::new_arc(),
        Arc::new(crate::tests::stubs::StubStatsService),
        Arc::new(crate::tests::stubs::StubRuleRequestService),
        Arc::new(crate::tests::stubs::StubZoneExceptionService),
    );

    let app = system_app_full(state);
    let req = Request::builder()
        .uri("/api/system/logs/download")
        .header("Cookie", "wardnet_session=valid-token")
        .extension(connect_info_ext())
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        resp.headers()
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap(),
        "text/plain; charset=utf-8"
    );
    assert!(
        resp.headers()
            .get("content-disposition")
            .unwrap()
            .to_str()
            .unwrap()
            .contains("wardnetd.log")
    );

    let body = axum::body::to_bytes(resp.into_body(), 16384).await.unwrap();
    let text = String::from_utf8(body.to_vec()).unwrap();
    assert!(text.contains("hello world"), "body was: {text}");
    assert!(text.contains("INFO"), "body was: {text}");
}

#[tokio::test]
async fn download_logs_formats_non_json_lines_as_is() {
    use wardnetd_services::diagnostics::Diagnostic;
    use wardnetd_services::logging::component::BoxedLayer;
    use wardnetd_services::logging::service::LogFileInfo;
    use wardnetd_services::logging::stream::LogEntry;

    struct MockLogServicePlainText;

    #[async_trait]
    impl LogService for MockLogServicePlainText {
        fn subscribe(&self) -> Result<tokio::sync::broadcast::Receiver<LogEntry>, AppError> {
            let (tx, rx) = tokio::sync::broadcast::channel(1);
            drop(tx);
            Ok(rx)
        }
        fn get_recent_errors(&self) -> Result<Vec<Diagnostic>, AppError> {
            Ok(Vec::new())
        }
        async fn list_log_files(&self) -> Result<Vec<LogFileInfo>, AppError> {
            Ok(Vec::new())
        }
        async fn download_log_file(&self, _name: Option<&str>) -> Result<String, AppError> {
            Ok("plain text log line".to_owned())
        }
        fn tracing_layers(&self) -> Vec<BoxedLayer> {
            Vec::new()
        }
        fn start_all(&self) {}
        fn stop_all(&self) {}
    }

    let admin_id = Uuid::new_v4();

    let state = AppState::new(
        Arc::new(AlwaysAuthService { admin_id }),
        Arc::new(crate::tests::stubs::StubBackupService),
        Arc::new(StubDeviceService),
        Arc::new(StubDhcpService),
        Arc::new(StubDnsService),
        Arc::new(StubDnsFilterService),
        Arc::new(StubDnsLocalService),
        Arc::new(crate::tests::stubs::StubDdnsService),
        Arc::new(crate::tests::stubs::StubTlsService),
        Arc::new(StubDiscoveryService),
        Arc::new(MockLogServicePlainText) as Arc<dyn LogService>,
        Arc::new(StubProviderService),
        Arc::new(StubRoutingService),
        Arc::new(StubNetworkZoneService),
        Arc::new(MockSystemService {
            response: Ok(default_status()),
        }),
        Arc::new(StubTunnelService),
        Arc::new(crate::tests::stubs::StubUpdateService),
        Arc::new(StubDhcpServer),
        Arc::new(StubDnsServer),
        Arc::new(StubEventPublisher),
        crate::tests::stubs::StubJobService::new_arc(),
        Arc::new(crate::tests::stubs::StubStatsService),
        Arc::new(crate::tests::stubs::StubRuleRequestService),
        Arc::new(crate::tests::stubs::StubZoneExceptionService),
    );

    let app = system_app_full(state);
    let req = Request::builder()
        .uri("/api/system/logs/download")
        .header("Cookie", "wardnet_session=valid-token")
        .extension(connect_info_ext())
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), 16384).await.unwrap();
    let text = String::from_utf8(body.to_vec()).unwrap();
    assert!(
        text.contains("plain text log line"),
        "non-JSON lines should pass through unchanged"
    );
}

#[tokio::test]
async fn download_logs_finds_dated_file() {
    use wardnetd_services::diagnostics::Diagnostic;
    use wardnetd_services::logging::component::BoxedLayer;
    use wardnetd_services::logging::service::LogFileInfo;
    use wardnetd_services::logging::stream::LogEntry;

    struct MockLogServiceDated;

    #[async_trait]
    impl LogService for MockLogServiceDated {
        fn subscribe(&self) -> Result<tokio::sync::broadcast::Receiver<LogEntry>, AppError> {
            let (tx, rx) = tokio::sync::broadcast::channel(1);
            drop(tx);
            Ok(rx)
        }
        fn get_recent_errors(&self) -> Result<Vec<Diagnostic>, AppError> {
            Ok(Vec::new())
        }
        async fn list_log_files(&self) -> Result<Vec<LogFileInfo>, AppError> {
            Ok(Vec::new())
        }
        async fn download_log_file(&self, _name: Option<&str>) -> Result<String, AppError> {
            Ok("dated log content".to_owned())
        }
        fn tracing_layers(&self) -> Vec<BoxedLayer> {
            Vec::new()
        }
        fn start_all(&self) {}
        fn stop_all(&self) {}
    }

    let admin_id = Uuid::new_v4();

    let state = AppState::new(
        Arc::new(AlwaysAuthService { admin_id }),
        Arc::new(crate::tests::stubs::StubBackupService),
        Arc::new(StubDeviceService),
        Arc::new(StubDhcpService),
        Arc::new(StubDnsService),
        Arc::new(StubDnsFilterService),
        Arc::new(StubDnsLocalService),
        Arc::new(crate::tests::stubs::StubDdnsService),
        Arc::new(crate::tests::stubs::StubTlsService),
        Arc::new(StubDiscoveryService),
        Arc::new(MockLogServiceDated) as Arc<dyn LogService>,
        Arc::new(StubProviderService),
        Arc::new(StubRoutingService),
        Arc::new(StubNetworkZoneService),
        Arc::new(MockSystemService {
            response: Ok(default_status()),
        }),
        Arc::new(StubTunnelService),
        Arc::new(crate::tests::stubs::StubUpdateService),
        Arc::new(StubDhcpServer),
        Arc::new(StubDnsServer),
        Arc::new(StubEventPublisher),
        crate::tests::stubs::StubJobService::new_arc(),
        Arc::new(crate::tests::stubs::StubStatsService),
        Arc::new(crate::tests::stubs::StubRuleRequestService),
        Arc::new(crate::tests::stubs::StubZoneExceptionService),
    );

    let app = system_app_full(state);
    let req = Request::builder()
        .uri("/api/system/logs/download")
        .header("Cookie", "wardnet_session=valid-token")
        .extension(connect_info_ext())
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), 16384).await.unwrap();
    let text = String::from_utf8(body.to_vec()).unwrap();
    assert!(
        text.contains("dated log content"),
        "should find the dated log file; body was: {text}"
    );
}

#[tokio::test]
async fn download_logs_no_file_returns_500() {
    use wardnetd_services::diagnostics::Diagnostic;
    use wardnetd_services::logging::component::BoxedLayer;
    use wardnetd_services::logging::service::LogFileInfo;
    use wardnetd_services::logging::stream::LogEntry;

    struct MockLogServiceNoFile;

    #[async_trait]
    impl LogService for MockLogServiceNoFile {
        fn subscribe(&self) -> Result<tokio::sync::broadcast::Receiver<LogEntry>, AppError> {
            let (tx, rx) = tokio::sync::broadcast::channel(1);
            drop(tx);
            Ok(rx)
        }
        fn get_recent_errors(&self) -> Result<Vec<Diagnostic>, AppError> {
            Ok(Vec::new())
        }
        async fn list_log_files(&self) -> Result<Vec<LogFileInfo>, AppError> {
            Ok(Vec::new())
        }
        async fn download_log_file(&self, _name: Option<&str>) -> Result<String, AppError> {
            Err(AppError::Internal(anyhow::anyhow!("no log files found")))
        }
        fn tracing_layers(&self) -> Vec<BoxedLayer> {
            Vec::new()
        }
        fn start_all(&self) {}
        fn stop_all(&self) {}
    }

    let admin_id = Uuid::new_v4();

    let state = AppState::new(
        Arc::new(AlwaysAuthService { admin_id }),
        Arc::new(crate::tests::stubs::StubBackupService),
        Arc::new(StubDeviceService),
        Arc::new(StubDhcpService),
        Arc::new(StubDnsService),
        Arc::new(StubDnsFilterService),
        Arc::new(StubDnsLocalService),
        Arc::new(crate::tests::stubs::StubDdnsService),
        Arc::new(crate::tests::stubs::StubTlsService),
        Arc::new(StubDiscoveryService),
        Arc::new(MockLogServiceNoFile) as Arc<dyn LogService>,
        Arc::new(StubProviderService),
        Arc::new(StubRoutingService),
        Arc::new(StubNetworkZoneService),
        Arc::new(MockSystemService {
            response: Ok(default_status()),
        }),
        Arc::new(StubTunnelService),
        Arc::new(crate::tests::stubs::StubUpdateService),
        Arc::new(StubDhcpServer),
        Arc::new(StubDnsServer),
        Arc::new(StubEventPublisher),
        crate::tests::stubs::StubJobService::new_arc(),
        Arc::new(crate::tests::stubs::StubStatsService),
        Arc::new(crate::tests::stubs::StubRuleRequestService),
        Arc::new(crate::tests::stubs::StubZoneExceptionService),
    );

    let app = system_app_full(state);
    let req = Request::builder()
        .uri("/api/system/logs/download")
        .header("Cookie", "wardnet_session=valid-token")
        .extension(connect_info_ext())
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

// ---------------------------------------------------------------------------
// POST /api/system/restart
// ---------------------------------------------------------------------------

#[tokio::test]
async fn restart_returns_204_on_success() {
    let admin_id = Uuid::new_v4();
    let state = make_state(
        AlwaysAuthService { admin_id },
        MockSystemService {
            response: Ok(default_status()),
        },
    );

    let app = system_app_full(state);
    let req = Request::builder()
        .method("POST")
        .uri("/api/system/restart")
        .header("Cookie", "wardnet_session=valid-token")
        .extension(connect_info_ext())
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn restart_requires_authentication() {
    let state = make_state(
        NeverAuthService,
        MockSystemService {
            response: Ok(default_status()),
        },
    );

    let app = system_app_full(state);
    let req = Request::builder()
        .method("POST")
        .uri("/api/system/restart")
        .extension(connect_info_ext())
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn restart_surfaces_service_error_as_500() {
    // A SystemService that returns an internal error from request_restart.
    struct FailingRestartService;
    #[async_trait]
    impl SystemService for FailingRestartService {
        fn version(&self) -> &'static str {
            "0.0.0"
        }
        fn uptime(&self) -> std::time::Duration {
            std::time::Duration::ZERO
        }
        async fn status(&self) -> Result<SystemStatusResponse, AppError> {
            unimplemented!()
        }
        async fn request_restart(&self) -> Result<(), AppError> {
            Err(AppError::Internal(anyhow::anyhow!("shutdown not wired")))
        }
        async fn request_reboot(&self) -> Result<(), AppError> {
            Ok(())
        }
        async fn request_shutdown(&self) -> Result<(), AppError> {
            Ok(())
        }
        async fn network_status(
            &self,
        ) -> Result<wardnet_common::api::NetworkStatusResponse, AppError> {
            unimplemented!()
        }
        async fn discover_gateway_mac(
            &self,
            _request: wardnet_common::api::DiscoverGatewayMacRequest,
        ) -> Result<wardnet_common::api::DiscoverGatewayMacResponse, AppError> {
            unimplemented!()
        }
        async fn dhcp_self_probe(
            &self,
        ) -> Result<wardnet_common::api::DhcpSelfProbeResponse, AppError> {
            unimplemented!()
        }
        async fn record_heartbeat(&self) -> Result<(), AppError> {
            Ok(())
        }
        async fn record_graceful_shutdown(&self) -> Result<(), AppError> {
            Ok(())
        }
        async fn acknowledge_last_shutdown(&self) -> Result<(), AppError> {
            Ok(())
        }
    }

    let admin_id = Uuid::new_v4();
    let state = make_state(AlwaysAuthService { admin_id }, FailingRestartService);
    let app = system_app_full(state);

    let req = Request::builder()
        .method("POST")
        .uri("/api/system/restart")
        .header("Authorization", "Bearer k")
        .extension(connect_info_ext())
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

// ---------------------------------------------------------------------------
// POST /api/system/reboot
// ---------------------------------------------------------------------------

#[tokio::test]
async fn reboot_returns_204_on_success() {
    let admin_id = Uuid::new_v4();
    let state = make_state(
        AlwaysAuthService { admin_id },
        MockSystemService {
            response: Ok(default_status()),
        },
    );

    let app = system_app_full(state);
    let req = Request::builder()
        .method("POST")
        .uri("/api/system/reboot")
        .header("Cookie", "wardnet_session=valid-token")
        .extension(connect_info_ext())
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn reboot_requires_authentication() {
    let state = make_state(
        NeverAuthService,
        MockSystemService {
            response: Ok(default_status()),
        },
    );

    let app = system_app_full(state);
    let req = Request::builder()
        .method("POST")
        .uri("/api/system/reboot")
        .extension(connect_info_ext())
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn reboot_surfaces_service_error_as_500() {
    struct FailingRebootService;
    #[async_trait]
    impl SystemService for FailingRebootService {
        fn version(&self) -> &'static str {
            "0.0.0"
        }
        fn uptime(&self) -> std::time::Duration {
            std::time::Duration::ZERO
        }
        async fn status(&self) -> Result<SystemStatusResponse, AppError> {
            unimplemented!()
        }
        async fn request_restart(&self) -> Result<(), AppError> {
            Ok(())
        }
        async fn request_reboot(&self) -> Result<(), AppError> {
            Err(AppError::Internal(anyhow::anyhow!("polkit denied")))
        }
        async fn request_shutdown(&self) -> Result<(), AppError> {
            Ok(())
        }
        async fn network_status(
            &self,
        ) -> Result<wardnet_common::api::NetworkStatusResponse, AppError> {
            unimplemented!()
        }
        async fn discover_gateway_mac(
            &self,
            _request: wardnet_common::api::DiscoverGatewayMacRequest,
        ) -> Result<wardnet_common::api::DiscoverGatewayMacResponse, AppError> {
            unimplemented!()
        }
        async fn dhcp_self_probe(
            &self,
        ) -> Result<wardnet_common::api::DhcpSelfProbeResponse, AppError> {
            unimplemented!()
        }
        async fn record_heartbeat(&self) -> Result<(), AppError> {
            Ok(())
        }
        async fn record_graceful_shutdown(&self) -> Result<(), AppError> {
            Ok(())
        }
        async fn acknowledge_last_shutdown(&self) -> Result<(), AppError> {
            Ok(())
        }
    }

    let admin_id = Uuid::new_v4();
    let state = make_state(AlwaysAuthService { admin_id }, FailingRebootService);
    let app = system_app_full(state);

    let req = Request::builder()
        .method("POST")
        .uri("/api/system/reboot")
        .header("Authorization", "Bearer k")
        .extension(connect_info_ext())
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

// ---------------------------------------------------------------------------
// POST /api/system/shutdown
// ---------------------------------------------------------------------------

#[tokio::test]
async fn shutdown_returns_204_on_success() {
    let admin_id = Uuid::new_v4();
    let state = make_state(
        AlwaysAuthService { admin_id },
        MockSystemService {
            response: Ok(default_status()),
        },
    );

    let app = system_app_full(state);
    let req = Request::builder()
        .method("POST")
        .uri("/api/system/shutdown")
        .header("Cookie", "wardnet_session=valid-token")
        .extension(connect_info_ext())
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn shutdown_requires_authentication() {
    let state = make_state(
        NeverAuthService,
        MockSystemService {
            response: Ok(default_status()),
        },
    );

    let app = system_app_full(state);
    let req = Request::builder()
        .method("POST")
        .uri("/api/system/shutdown")
        .extension(connect_info_ext())
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn shutdown_surfaces_service_error_as_500() {
    struct FailingShutdownService;
    #[async_trait]
    impl SystemService for FailingShutdownService {
        fn version(&self) -> &'static str {
            "0.0.0"
        }
        fn uptime(&self) -> std::time::Duration {
            std::time::Duration::ZERO
        }
        async fn status(&self) -> Result<SystemStatusResponse, AppError> {
            unimplemented!()
        }
        async fn request_restart(&self) -> Result<(), AppError> {
            Ok(())
        }
        async fn request_reboot(&self) -> Result<(), AppError> {
            Ok(())
        }
        async fn request_shutdown(&self) -> Result<(), AppError> {
            Err(AppError::Internal(anyhow::anyhow!("polkit denied")))
        }
        async fn network_status(
            &self,
        ) -> Result<wardnet_common::api::NetworkStatusResponse, AppError> {
            unimplemented!()
        }
        async fn discover_gateway_mac(
            &self,
            _request: wardnet_common::api::DiscoverGatewayMacRequest,
        ) -> Result<wardnet_common::api::DiscoverGatewayMacResponse, AppError> {
            unimplemented!()
        }
        async fn dhcp_self_probe(
            &self,
        ) -> Result<wardnet_common::api::DhcpSelfProbeResponse, AppError> {
            unimplemented!()
        }
        async fn record_heartbeat(&self) -> Result<(), AppError> {
            Ok(())
        }
        async fn record_graceful_shutdown(&self) -> Result<(), AppError> {
            Ok(())
        }
        async fn acknowledge_last_shutdown(&self) -> Result<(), AppError> {
            Ok(())
        }
    }

    let admin_id = Uuid::new_v4();
    let state = make_state(AlwaysAuthService { admin_id }, FailingShutdownService);
    let app = system_app_full(state);

    let req = Request::builder()
        .method("POST")
        .uri("/api/system/shutdown")
        .header("Authorization", "Bearer k")
        .extension(connect_info_ext())
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

// ---------------------------------------------------------------------------
// POST /api/system/shutdown/acknowledge
// ---------------------------------------------------------------------------

#[tokio::test]
async fn acknowledge_shutdown_returns_204_on_success() {
    let admin_id = Uuid::new_v4();
    let state = make_state(
        AlwaysAuthService { admin_id },
        MockSystemService {
            response: Ok(default_status()),
        },
    );

    let app = system_app_full(state);
    let req = Request::builder()
        .method("POST")
        .uri("/api/system/shutdown/acknowledge")
        .header("Cookie", "wardnet_session=valid-token")
        .extension(connect_info_ext())
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn acknowledge_shutdown_requires_authentication() {
    let state = make_state(
        NeverAuthService,
        MockSystemService {
            response: Ok(default_status()),
        },
    );

    let app = system_app_full(state);
    let req = Request::builder()
        .method("POST")
        .uri("/api/system/shutdown/acknowledge")
        .extension(connect_info_ext())
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn acknowledge_shutdown_surfaces_service_error_as_500() {
    struct FailingAckService;
    #[async_trait]
    impl SystemService for FailingAckService {
        fn version(&self) -> &'static str {
            "0.0.0"
        }
        fn uptime(&self) -> std::time::Duration {
            std::time::Duration::ZERO
        }
        async fn status(&self) -> Result<SystemStatusResponse, AppError> {
            unimplemented!()
        }
        async fn request_restart(&self) -> Result<(), AppError> {
            Ok(())
        }
        async fn request_reboot(&self) -> Result<(), AppError> {
            Ok(())
        }
        async fn request_shutdown(&self) -> Result<(), AppError> {
            Ok(())
        }
        async fn network_status(
            &self,
        ) -> Result<wardnet_common::api::NetworkStatusResponse, AppError> {
            unimplemented!()
        }
        async fn discover_gateway_mac(
            &self,
            _request: wardnet_common::api::DiscoverGatewayMacRequest,
        ) -> Result<wardnet_common::api::DiscoverGatewayMacResponse, AppError> {
            unimplemented!()
        }
        async fn dhcp_self_probe(
            &self,
        ) -> Result<wardnet_common::api::DhcpSelfProbeResponse, AppError> {
            unimplemented!()
        }
        async fn record_heartbeat(&self) -> Result<(), AppError> {
            Ok(())
        }
        async fn record_graceful_shutdown(&self) -> Result<(), AppError> {
            Ok(())
        }
        async fn acknowledge_last_shutdown(&self) -> Result<(), AppError> {
            Err(AppError::Internal(anyhow::anyhow!("db locked")))
        }
    }

    let admin_id = Uuid::new_v4();
    let state = make_state(AlwaysAuthService { admin_id }, FailingAckService);
    let app = system_app_full(state);

    let req = Request::builder()
        .method("POST")
        .uri("/api/system/shutdown/acknowledge")
        .header("Authorization", "Bearer k")
        .extension(connect_info_ext())
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn status_includes_last_shutdown_payload() {
    use chrono::{TimeZone, Utc};

    let at = Utc.with_ymd_and_hms(2026, 5, 7, 12, 0, 0).unwrap();
    let admin_id = Uuid::new_v4();
    let state = make_state(
        AlwaysAuthService { admin_id },
        MockSystemService {
            response: Ok(SystemStatusResponse {
                version: "1.0.0".to_owned(),
                release_version: "1.0.0".to_owned(),
                uptime_seconds: 0,
                device_count: 0,
                tunnel_count: 0,
                tunnel_active_count: 0,
                db_size_bytes: 0,
                cpu_usage_percent: 0.0,
                memory_used_bytes: 0,
                memory_total_bytes: 0,
                disk_free_bytes: 0,
                disk_total_bytes: 0,
                last_shutdown: LastShutdownStatus {
                    state: LastShutdownState::Unclean,
                    at: Some(at),
                    acknowledged_at: None,
                },
            }),
        },
    );

    let app = system_app(state);
    let req = Request::builder()
        .uri("/api/system/status")
        .header("Cookie", "wardnet_session=valid-token")
        .extension(connect_info_ext())
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["last_shutdown"]["state"], "unclean");
    assert!(json["last_shutdown"]["at"].is_string());
    assert!(json["last_shutdown"]["acknowledged_at"].is_null());
}

#[tokio::test]
async fn status_authenticates_via_bearer_token() {
    let admin_id = Uuid::new_v4();
    let state = make_state(
        AlwaysAuthService { admin_id },
        MockSystemService {
            response: Ok(SystemStatusResponse {
                version: "0.0.1".to_owned(),
                release_version: "0.0.1".to_owned(),
                uptime_seconds: 1,
                device_count: 0,
                tunnel_count: 0,
                tunnel_active_count: 0,
                db_size_bytes: 0,
                cpu_usage_percent: 0.0,
                memory_used_bytes: 0,
                memory_total_bytes: 0,
                disk_free_bytes: 0,
                disk_total_bytes: 0,
                last_shutdown: default_last_shutdown(),
            }),
        },
    );

    let app = system_app(state);
    let req = Request::builder()
        .uri("/api/system/status")
        .header("Authorization", "Bearer test-api-key")
        .extension(connect_info_ext())
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

// ---------------------------------------------------------------------------
// GET / PUT /api/system/default-policy
// ---------------------------------------------------------------------------

#[tokio::test]
async fn get_default_policy_returns_direct() {
    let admin_id = Uuid::new_v4();
    let state = make_state_with_routing(
        AlwaysAuthService { admin_id },
        MockSystemService {
            response: Ok(default_status()),
        },
        MockRoutingService::ok("direct"),
    );

    let app = system_app_full(state);
    let req = Request::builder()
        .uri("/api/system/default-policy")
        .header("Cookie", "wardnet_session=valid-token")
        .extension(connect_info_ext())
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["policy"], "direct");
}

#[tokio::test]
async fn get_default_policy_returns_tunnel_uuid() {
    let admin_id = Uuid::new_v4();
    let tunnel_uuid = "10000000-0000-0000-0000-000000000001";
    let state = make_state_with_routing(
        AlwaysAuthService { admin_id },
        MockSystemService {
            response: Ok(default_status()),
        },
        MockRoutingService::ok(tunnel_uuid),
    );

    let app = system_app_full(state);
    let req = Request::builder()
        .uri("/api/system/default-policy")
        .header("Authorization", "Bearer api-key")
        .extension(connect_info_ext())
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["policy"], tunnel_uuid);
}

#[tokio::test]
async fn get_default_policy_requires_authentication() {
    let state = make_state(
        NeverAuthService,
        MockSystemService {
            response: Ok(default_status()),
        },
    );

    let app = system_app_full(state);
    let req = Request::builder()
        .uri("/api/system/default-policy")
        .extension(connect_info_ext())
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn get_default_policy_surfaces_service_error_as_500() {
    let admin_id = Uuid::new_v4();
    let state = make_state_with_routing(
        AlwaysAuthService { admin_id },
        MockSystemService {
            response: Ok(default_status()),
        },
        MockRoutingService::new(Err(AppError::Internal(anyhow::anyhow!("db down"))), Ok(())),
    );

    let app = system_app_full(state);
    let req = Request::builder()
        .uri("/api/system/default-policy")
        .header("Cookie", "wardnet_session=valid-token")
        .extension(connect_info_ext())
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn set_default_policy_succeeds_with_direct() {
    let admin_id = Uuid::new_v4();
    let state = make_state_with_routing(
        AlwaysAuthService { admin_id },
        MockSystemService {
            response: Ok(default_status()),
        },
        MockRoutingService::ok("direct"),
    );

    let body = serde_json::to_vec(&SetDefaultPolicyRequest {
        policy: "direct".to_owned(),
    })
    .unwrap();

    let app = system_app_full(state);
    let req = Request::builder()
        .method("PUT")
        .uri("/api/system/default-policy")
        .header("Cookie", "wardnet_session=valid-token")
        .header("Content-Type", "application/json")
        .extension(connect_info_ext())
        .body(Body::from(body))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["policy"], "direct");
}

#[tokio::test]
async fn set_default_policy_succeeds_with_tunnel_uuid() {
    let admin_id = Uuid::new_v4();
    let tunnel_uuid = "20000000-0000-0000-0000-000000000002";
    let state = make_state_with_routing(
        AlwaysAuthService { admin_id },
        MockSystemService {
            response: Ok(default_status()),
        },
        MockRoutingService::ok("direct"),
    );

    let body = serde_json::to_vec(&SetDefaultPolicyRequest {
        policy: tunnel_uuid.to_owned(),
    })
    .unwrap();

    let app = system_app_full(state);
    let req = Request::builder()
        .method("PUT")
        .uri("/api/system/default-policy")
        .header("Authorization", "Bearer key")
        .header("Content-Type", "application/json")
        .extension(connect_info_ext())
        .body(Body::from(body))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    // Echoed from the request body, not the mock get_response.
    assert_eq!(json["policy"], tunnel_uuid);
}

#[tokio::test]
async fn set_default_policy_invalid_returns_400() {
    let admin_id = Uuid::new_v4();
    let state = make_state_with_routing(
        AlwaysAuthService { admin_id },
        MockSystemService {
            response: Ok(default_status()),
        },
        MockRoutingService::new(
            Ok("direct".to_owned()),
            Err(AppError::BadRequest("not a uuid".to_owned())),
        ),
    );

    let body = serde_json::to_vec(&SetDefaultPolicyRequest {
        policy: "garbage".to_owned(),
    })
    .unwrap();

    let app = system_app_full(state);
    let req = Request::builder()
        .method("PUT")
        .uri("/api/system/default-policy")
        .header("Cookie", "wardnet_session=valid-token")
        .header("Content-Type", "application/json")
        .extension(connect_info_ext())
        .body(Body::from(body))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn set_default_policy_requires_authentication() {
    let state = make_state(
        NeverAuthService,
        MockSystemService {
            response: Ok(default_status()),
        },
    );

    let body = serde_json::to_vec(&SetDefaultPolicyRequest {
        policy: "direct".to_owned(),
    })
    .unwrap();

    let app = system_app_full(state);
    let req = Request::builder()
        .method("PUT")
        .uri("/api/system/default-policy")
        .header("Content-Type", "application/json")
        .extension(connect_info_ext())
        .body(Body::from(body))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}
