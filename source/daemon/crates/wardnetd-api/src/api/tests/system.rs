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
use uuid::Uuid;
use wardnet_common::api::SystemStatusResponse;

use crate::state::AppState;
use crate::tests::stubs::{
    StubDeviceService, StubDhcpServer, StubDhcpService, StubDiscoveryService, StubDnsServer,
    StubDnsService, StubEventPublisher, StubLogService, StubProviderService, StubRoutingService,
    StubTunnelService,
};
use wardnetd_services::LogService;
use wardnetd_services::auth::service::LoginResult;
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
    async fn login(&self, _u: &str, _p: &str) -> Result<LoginResult, AppError> {
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
}

/// Mock auth service that always rejects.
struct NeverAuthService;
#[async_trait]
impl AuthService for NeverAuthService {
    async fn login(&self, _u: &str, _p: &str) -> Result<LoginResult, AppError> {
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
                uptime_seconds: r.uptime_seconds,
                device_count: r.device_count,
                tunnel_count: r.tunnel_count,
                tunnel_active_count: r.tunnel_active_count,
                db_size_bytes: r.db_size_bytes,
                cpu_usage_percent: r.cpu_usage_percent,
                memory_used_bytes: r.memory_used_bytes,
                memory_total_bytes: r.memory_total_bytes,
            }),
            Err(_) => Err(AppError::Internal(anyhow::anyhow!("mock error"))),
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_state(auth: impl AuthService + 'static, system: impl SystemService + 'static) -> AppState {
    AppState::new(
        Arc::new(auth),
        Arc::new(StubDeviceService),
        Arc::new(StubDhcpService),
        Arc::new(StubDnsService),
        Arc::new(StubDiscoveryService),
        Arc::new(StubLogService) as Arc<dyn LogService>,
        Arc::new(StubProviderService),
        Arc::new(StubRoutingService),
        Arc::new(system),
        Arc::new(StubTunnelService),
        Arc::new(StubDhcpServer),
        Arc::new(StubDnsServer),
        Arc::new(StubEventPublisher),
        crate::tests::stubs::StubJobService::new_arc(),
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
        .with_state(state)
}

fn default_status() -> SystemStatusResponse {
    SystemStatusResponse {
        version: "0.0.1".to_owned(),
        uptime_seconds: 0,
        device_count: 0,
        tunnel_count: 0,
        tunnel_active_count: 0,
        db_size_bytes: 0,
        cpu_usage_percent: 0.0,
        memory_used_bytes: 0,
        memory_total_bytes: 0,
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
                uptime_seconds: 3600,
                device_count: 10,
                tunnel_count: 3,
                tunnel_active_count: 1,
                db_size_bytes: 4096,
                cpu_usage_percent: 25.5,
                memory_used_bytes: 1_073_741_824,
                memory_total_bytes: 4_294_967_296,
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
                uptime_seconds: 0,
                device_count: 0,
                tunnel_count: 0,
                tunnel_active_count: 0,
                db_size_bytes: 0,
                cpu_usage_percent: 0.0,
                memory_used_bytes: 0,
                memory_total_bytes: 0,
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
    use wardnetd_services::logging::component::BoxedLayer;
    use wardnetd_services::logging::error_notifier::ErrorEntry;
    use wardnetd_services::logging::service::LogFileInfo;
    use wardnetd_services::logging::stream::LogEntry;

    /// A mock `LogService` that returns canned error entries.
    struct MockLogServiceWithErrors;

    #[async_trait]
    impl LogService for MockLogServiceWithErrors {
        fn subscribe(&self) -> tokio::sync::broadcast::Receiver<LogEntry> {
            let (tx, rx) = tokio::sync::broadcast::channel(1);
            drop(tx);
            rx
        }
        fn get_recent_errors(&self) -> Vec<ErrorEntry> {
            vec![
                ErrorEntry {
                    level: "ERROR".to_owned(),
                    message: "boom".to_owned(),
                    target: "test".to_owned(),
                    timestamp: chrono::Utc::now(),
                },
                ErrorEntry {
                    level: "WARN".to_owned(),
                    message: "careful".to_owned(),
                    target: "test".to_owned(),
                    timestamp: chrono::Utc::now(),
                },
            ]
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
        Arc::new(StubDeviceService),
        Arc::new(StubDhcpService),
        Arc::new(StubDnsService),
        Arc::new(StubDiscoveryService),
        Arc::new(MockLogServiceWithErrors) as Arc<dyn LogService>,
        Arc::new(StubProviderService),
        Arc::new(StubRoutingService),
        Arc::new(MockSystemService {
            response: Ok(default_status()),
        }),
        Arc::new(StubTunnelService),
        Arc::new(StubDhcpServer),
        Arc::new(StubDnsServer),
        Arc::new(StubEventPublisher),
        crate::tests::stubs::StubJobService::new_arc(),
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
    assert_eq!(errors[0]["level"], "ERROR");
    assert_eq!(errors[1]["level"], "WARN");
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
    use wardnetd_services::logging::component::BoxedLayer;
    use wardnetd_services::logging::error_notifier::ErrorEntry;
    use wardnetd_services::logging::service::LogFileInfo;
    use wardnetd_services::logging::stream::LogEntry;

    /// Mock `LogService` that returns formatted log content.
    struct MockLogServiceWithContent {
        content: String,
    }

    #[async_trait]
    impl LogService for MockLogServiceWithContent {
        fn subscribe(&self) -> tokio::sync::broadcast::Receiver<LogEntry> {
            let (tx, rx) = tokio::sync::broadcast::channel(1);
            drop(tx);
            rx
        }
        fn get_recent_errors(&self) -> Vec<ErrorEntry> {
            Vec::new()
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
        Arc::new(StubDeviceService),
        Arc::new(StubDhcpService),
        Arc::new(StubDnsService),
        Arc::new(StubDiscoveryService),
        Arc::new(MockLogServiceWithContent {
            content: "2026-04-13T00:00:00Z  INFO test hello world".to_owned(),
        }) as Arc<dyn LogService>,
        Arc::new(StubProviderService),
        Arc::new(StubRoutingService),
        Arc::new(MockSystemService {
            response: Ok(default_status()),
        }),
        Arc::new(StubTunnelService),
        Arc::new(StubDhcpServer),
        Arc::new(StubDnsServer),
        Arc::new(StubEventPublisher),
        crate::tests::stubs::StubJobService::new_arc(),
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
    use wardnetd_services::logging::component::BoxedLayer;
    use wardnetd_services::logging::error_notifier::ErrorEntry;
    use wardnetd_services::logging::service::LogFileInfo;
    use wardnetd_services::logging::stream::LogEntry;

    struct MockLogServicePlainText;

    #[async_trait]
    impl LogService for MockLogServicePlainText {
        fn subscribe(&self) -> tokio::sync::broadcast::Receiver<LogEntry> {
            let (tx, rx) = tokio::sync::broadcast::channel(1);
            drop(tx);
            rx
        }
        fn get_recent_errors(&self) -> Vec<ErrorEntry> {
            Vec::new()
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
        Arc::new(StubDeviceService),
        Arc::new(StubDhcpService),
        Arc::new(StubDnsService),
        Arc::new(StubDiscoveryService),
        Arc::new(MockLogServicePlainText) as Arc<dyn LogService>,
        Arc::new(StubProviderService),
        Arc::new(StubRoutingService),
        Arc::new(MockSystemService {
            response: Ok(default_status()),
        }),
        Arc::new(StubTunnelService),
        Arc::new(StubDhcpServer),
        Arc::new(StubDnsServer),
        Arc::new(StubEventPublisher),
        crate::tests::stubs::StubJobService::new_arc(),
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
    use wardnetd_services::logging::component::BoxedLayer;
    use wardnetd_services::logging::error_notifier::ErrorEntry;
    use wardnetd_services::logging::service::LogFileInfo;
    use wardnetd_services::logging::stream::LogEntry;

    struct MockLogServiceDated;

    #[async_trait]
    impl LogService for MockLogServiceDated {
        fn subscribe(&self) -> tokio::sync::broadcast::Receiver<LogEntry> {
            let (tx, rx) = tokio::sync::broadcast::channel(1);
            drop(tx);
            rx
        }
        fn get_recent_errors(&self) -> Vec<ErrorEntry> {
            Vec::new()
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
        Arc::new(StubDeviceService),
        Arc::new(StubDhcpService),
        Arc::new(StubDnsService),
        Arc::new(StubDiscoveryService),
        Arc::new(MockLogServiceDated) as Arc<dyn LogService>,
        Arc::new(StubProviderService),
        Arc::new(StubRoutingService),
        Arc::new(MockSystemService {
            response: Ok(default_status()),
        }),
        Arc::new(StubTunnelService),
        Arc::new(StubDhcpServer),
        Arc::new(StubDnsServer),
        Arc::new(StubEventPublisher),
        crate::tests::stubs::StubJobService::new_arc(),
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
    use wardnetd_services::logging::component::BoxedLayer;
    use wardnetd_services::logging::error_notifier::ErrorEntry;
    use wardnetd_services::logging::service::LogFileInfo;
    use wardnetd_services::logging::stream::LogEntry;

    struct MockLogServiceNoFile;

    #[async_trait]
    impl LogService for MockLogServiceNoFile {
        fn subscribe(&self) -> tokio::sync::broadcast::Receiver<LogEntry> {
            let (tx, rx) = tokio::sync::broadcast::channel(1);
            drop(tx);
            rx
        }
        fn get_recent_errors(&self) -> Vec<ErrorEntry> {
            Vec::new()
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
        Arc::new(StubDeviceService),
        Arc::new(StubDhcpService),
        Arc::new(StubDnsService),
        Arc::new(StubDiscoveryService),
        Arc::new(MockLogServiceNoFile) as Arc<dyn LogService>,
        Arc::new(StubProviderService),
        Arc::new(StubRoutingService),
        Arc::new(MockSystemService {
            response: Ok(default_status()),
        }),
        Arc::new(StubTunnelService),
        Arc::new(StubDhcpServer),
        Arc::new(StubDnsServer),
        Arc::new(StubEventPublisher),
        crate::tests::stubs::StubJobService::new_arc(),
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

#[tokio::test]
async fn status_authenticates_via_bearer_token() {
    let admin_id = Uuid::new_v4();
    let state = make_state(
        AlwaysAuthService { admin_id },
        MockSystemService {
            response: Ok(SystemStatusResponse {
                version: "0.0.1".to_owned(),
                uptime_seconds: 1,
                device_count: 0,
                tunnel_count: 0,
                tunnel_active_count: 0,
                db_size_bytes: 0,
                cpu_usage_percent: 0.0,
                memory_used_bytes: 0,
                memory_total_bytes: 0,
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
