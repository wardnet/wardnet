//! Tests for the `/api/backup/*` HTTP handlers.
//!
//! The handlers themselves are thin — they extract request fields and
//! call into [`BackupService`]. A `MockBackupService` returns canned
//! responses so each test can nail down one handler path: the admin
//! guard, the happy path (status code + body + response headers), the
//! service-error surface, and the multipart-extraction branches
//! specific to `preview_import`.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use async_trait::async_trait;
use axum::Router;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use axum::routing::{get, post};
use tower::ServiceExt;
use uuid::Uuid;
use wardnet_common::api::{
    ApplyImportRequest, ApplyImportResponse, BackupStatusResponse, ExportBackupRequest,
    ListSnapshotsResponse, RestorePreviewResponse,
};
use wardnet_common::backup::{BackupStatus, BundleManifest, LocalSnapshot, SnapshotKind};
use wardnetd_services::BackupService;
use wardnetd_services::auth::service::LoginResult;
use wardnetd_services::error::AppError;

use crate::state::AppState;
use crate::tests::stubs::{
    StubDeviceService, StubDhcpServer, StubDhcpService, StubDiscoveryService, StubDnsFilterService,
    StubDnsLocalService, StubDnsServer, StubDnsService, StubEventPublisher, StubLogService,
    StubNetworkZoneService, StubProviderService, StubRoutingService, StubSystemService,
    StubTunnelService,
};
use wardnetd_services::{AuthService, LogService};

// ---------------------------------------------------------------------------
// Auth mocks
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
    async fn cleanup_expired_sessions(&self) -> Result<u64, AppError> {
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
    async fn cleanup_expired_sessions(&self) -> Result<u64, AppError> {
        unimplemented!()
    }
}

// ---------------------------------------------------------------------------
// Mock BackupService
// ---------------------------------------------------------------------------

#[derive(Default)]
struct MockCalls {
    export_passphrase: Option<String>,
    preview_bundle_len: Option<usize>,
    preview_passphrase: Option<String>,
    apply_token: Option<String>,
}

struct MockBackupService {
    status: Result<BackupStatusResponse, AppError>,
    export: Result<Vec<u8>, AppError>,
    preview: Result<RestorePreviewResponse, AppError>,
    apply: Result<ApplyImportResponse, AppError>,
    list: Result<ListSnapshotsResponse, AppError>,
    calls: Mutex<MockCalls>,
}

impl MockBackupService {
    fn ok_idle() -> Self {
        Self {
            status: Ok(BackupStatusResponse {
                status: BackupStatus::Idle,
            }),
            export: Ok(b"age-encryption.org/v1\nfake-bundle".to_vec()),
            preview: Ok(sample_preview()),
            apply: Ok(sample_apply()),
            list: Ok(ListSnapshotsResponse {
                snapshots: vec![sample_snapshot()],
            }),
            calls: Mutex::new(MockCalls::default()),
        }
    }

    fn clone_result<T: Clone>(res: &Result<T, AppError>) -> Result<T, AppError> {
        match res {
            Ok(v) => Ok(v.clone()),
            Err(AppError::BadRequest(m)) => Err(AppError::BadRequest(m.clone())),
            Err(AppError::Unauthorized(m)) => Err(AppError::Unauthorized(m.clone())),
            Err(AppError::Forbidden(m)) => Err(AppError::Forbidden(m.clone())),
            Err(AppError::NotFound(m)) => Err(AppError::NotFound(m.clone())),
            Err(AppError::Conflict(m)) => Err(AppError::Conflict(m.clone())),
            Err(_) => Err(AppError::Internal(anyhow::anyhow!("mock internal error"))),
        }
    }
}

fn sample_preview() -> RestorePreviewResponse {
    RestorePreviewResponse {
        manifest: BundleManifest::new("0.1.0-test", 7, "preview-host", 2),
        compatible: true,
        incompatibility_reason: None,
        files_to_replace: vec![
            "/etc/wardnet/wardnet.db".into(),
            "/etc/wardnet/wardnet.toml".into(),
        ],
        preview_token: "abc-123".into(),
    }
}

fn sample_apply() -> ApplyImportResponse {
    ApplyImportResponse {
        manifest: BundleManifest::new("0.1.0-test", 7, "apply-host", 2),
        snapshots: vec![sample_snapshot()],
    }
}

fn sample_snapshot() -> LocalSnapshot {
    LocalSnapshot {
        path: "/etc/wardnet/wardnet.db.bak-20260422T120000Z".into(),
        kind: SnapshotKind::Database,
        created_at: chrono::Utc::now(),
        size_bytes: 4096,
    }
}

#[async_trait]
impl BackupService for MockBackupService {
    async fn status(&self) -> Result<BackupStatusResponse, AppError> {
        Self::clone_result(&self.status)
    }
    async fn export(&self, req: ExportBackupRequest) -> Result<Vec<u8>, AppError> {
        self.calls.lock().unwrap().export_passphrase = Some(req.passphrase);
        Self::clone_result(&self.export)
    }
    async fn preview_import(
        &self,
        bundle: Vec<u8>,
        passphrase: String,
    ) -> Result<RestorePreviewResponse, AppError> {
        let mut calls = self.calls.lock().unwrap();
        calls.preview_bundle_len = Some(bundle.len());
        calls.preview_passphrase = Some(passphrase);
        Self::clone_result(&self.preview)
    }
    async fn apply_import(&self, req: ApplyImportRequest) -> Result<ApplyImportResponse, AppError> {
        self.calls.lock().unwrap().apply_token = Some(req.preview_token);
        Self::clone_result(&self.apply)
    }
    async fn list_snapshots(&self) -> Result<ListSnapshotsResponse, AppError> {
        Self::clone_result(&self.list)
    }
    async fn cleanup_old_snapshots(&self, _retain: Duration) -> Result<u32, AppError> {
        Ok(0)
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_state(auth: impl AuthService + 'static, backup: Arc<dyn BackupService>) -> AppState {
    AppState::new(
        Arc::new(auth),
        backup,
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
        Arc::new(StubSystemService),
        Arc::new(StubTunnelService),
        Arc::new(crate::tests::stubs::StubUpdateService),
        Arc::new(StubDhcpServer),
        Arc::new(StubDnsServer),
        Arc::new(StubEventPublisher),
        crate::tests::stubs::StubJobService::new_arc(),
        Arc::new(crate::tests::stubs::StubStatsService),
        Arc::new(crate::tests::stubs::StubRuleRequestService),
    )
}

fn backup_app(state: AppState) -> Router {
    Router::new()
        .route("/api/backup/status", get(crate::api::backup::status))
        .route("/api/backup/export", post(crate::api::backup::export))
        .route(
            "/api/backup/import/preview",
            post(crate::api::backup::preview_import),
        )
        .route(
            "/api/backup/import/apply",
            post(crate::api::backup::apply_import),
        )
        .route(
            "/api/backup/snapshots",
            get(crate::api::backup::list_snapshots),
        )
        .with_state(state)
}

fn connect_info_ext() -> ConnectInfo<SocketAddr> {
    ConnectInfo(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 1234))
}

fn multipart_body(boundary: &str, bundle: Option<&[u8]>, passphrase: Option<&str>) -> Vec<u8> {
    let mut body = Vec::new();
    if let Some(b) = bundle {
        body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
        body.extend_from_slice(
            b"Content-Disposition: form-data; name=\"bundle\"; filename=\"bundle.wardnet.age\"\r\n",
        );
        body.extend_from_slice(b"Content-Type: application/octet-stream\r\n\r\n");
        body.extend_from_slice(b);
        body.extend_from_slice(b"\r\n");
    }
    if let Some(p) = passphrase {
        body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
        body.extend_from_slice(b"Content-Disposition: form-data; name=\"passphrase\"\r\n\r\n");
        body.extend_from_slice(p.as_bytes());
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
    body
}

// ---------------------------------------------------------------------------
// GET /api/backup/status
// ---------------------------------------------------------------------------

#[tokio::test]
async fn status_returns_200_with_subsystem_phase() {
    let backup = Arc::new(MockBackupService::ok_idle());
    let state = make_state(
        AlwaysAuthService {
            admin_id: Uuid::new_v4(),
        },
        backup,
    );
    let app = backup_app(state);

    let req = Request::builder()
        .uri("/api/backup/status")
        .header("Cookie", "wardnet_session=valid")
        .extension(connect_info_ext())
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"]["state"], "idle");
}

#[tokio::test]
async fn status_requires_authentication() {
    let backup = Arc::new(MockBackupService::ok_idle());
    let state = make_state(NeverAuthService, backup);
    let app = backup_app(state);

    let req = Request::builder()
        .uri("/api/backup/status")
        .extension(connect_info_ext())
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

// ---------------------------------------------------------------------------
// POST /api/backup/export
// ---------------------------------------------------------------------------

#[tokio::test]
async fn export_returns_octet_stream_with_attachment_header() {
    let backup = Arc::new(MockBackupService::ok_idle());
    let state = make_state(
        AlwaysAuthService {
            admin_id: Uuid::new_v4(),
        },
        backup.clone(),
    );
    let app = backup_app(state);

    let body = serde_json::json!({
        "passphrase": "correct-horse-battery-staple"
    });
    let req = Request::builder()
        .method("POST")
        .uri("/api/backup/export")
        .header("Content-Type", "application/json")
        .header("Authorization", "Bearer test-key")
        .extension(connect_info_ext())
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        resp.headers()
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap(),
        "application/octet-stream"
    );
    let disposition = resp
        .headers()
        .get("content-disposition")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(disposition.starts_with("attachment; filename=\"wardnet-"));
    assert!(disposition.ends_with(".wardnet.age\""));

    let bytes = axum::body::to_bytes(resp.into_body(), 65536).await.unwrap();
    assert!(bytes.starts_with(b"age-encryption.org/v1"));

    // The handler forwards the passphrase to the service unchanged.
    assert_eq!(
        backup.calls.lock().unwrap().export_passphrase.as_deref(),
        Some("correct-horse-battery-staple")
    );
}

#[tokio::test]
async fn export_requires_authentication() {
    let backup = Arc::new(MockBackupService::ok_idle());
    let state = make_state(NeverAuthService, backup);
    let app = backup_app(state);

    let body = serde_json::json!({ "passphrase": "p" });
    let req = Request::builder()
        .method("POST")
        .uri("/api/backup/export")
        .header("Content-Type", "application/json")
        .extension(connect_info_ext())
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn export_surfaces_service_bad_request_as_400() {
    let backup = Arc::new(MockBackupService {
        status: Ok(BackupStatusResponse {
            status: BackupStatus::Idle,
        }),
        export: Err(AppError::BadRequest("passphrase too short".into())),
        preview: Ok(sample_preview()),
        apply: Ok(sample_apply()),
        list: Ok(ListSnapshotsResponse {
            snapshots: Vec::new(),
        }),
        calls: Mutex::new(MockCalls::default()),
    });
    let state = make_state(
        AlwaysAuthService {
            admin_id: Uuid::new_v4(),
        },
        backup,
    );
    let app = backup_app(state);

    let body = serde_json::json!({ "passphrase": "short" });
    let req = Request::builder()
        .method("POST")
        .uri("/api/backup/export")
        .header("Content-Type", "application/json")
        .header("Authorization", "Bearer k")
        .extension(connect_info_ext())
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

// ---------------------------------------------------------------------------
// POST /api/backup/import/preview
// ---------------------------------------------------------------------------

#[tokio::test]
async fn preview_import_extracts_multipart_and_returns_token() {
    let backup = Arc::new(MockBackupService::ok_idle());
    let state = make_state(
        AlwaysAuthService {
            admin_id: Uuid::new_v4(),
        },
        backup.clone(),
    );
    let app = backup_app(state);

    let boundary = "----wardnet-test-boundary";
    let body = multipart_body(
        boundary,
        Some(b"fake-bundle-bytes"),
        Some("correct-horse-battery-staple"),
    );

    let req = Request::builder()
        .method("POST")
        .uri("/api/backup/import/preview")
        .header(
            "Content-Type",
            format!("multipart/form-data; boundary={boundary}"),
        )
        .header("Authorization", "Bearer k")
        .extension(connect_info_ext())
        .body(Body::from(body))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["preview_token"], "abc-123");
    assert_eq!(json["compatible"], true);

    let calls = backup.calls.lock().unwrap();
    assert_eq!(calls.preview_bundle_len, Some(17));
    assert_eq!(
        calls.preview_passphrase.as_deref(),
        Some("correct-horse-battery-staple")
    );
}

#[tokio::test]
async fn preview_import_requires_authentication() {
    let backup = Arc::new(MockBackupService::ok_idle());
    let state = make_state(NeverAuthService, backup);
    let app = backup_app(state);

    let boundary = "----wardnet-test-boundary";
    let body = multipart_body(boundary, Some(b"bytes"), Some("p"));

    let req = Request::builder()
        .method("POST")
        .uri("/api/backup/import/preview")
        .header(
            "Content-Type",
            format!("multipart/form-data; boundary={boundary}"),
        )
        .extension(connect_info_ext())
        .body(Body::from(body))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn preview_import_rejects_missing_bundle_field() {
    let backup = Arc::new(MockBackupService::ok_idle());
    let state = make_state(
        AlwaysAuthService {
            admin_id: Uuid::new_v4(),
        },
        backup,
    );
    let app = backup_app(state);

    let boundary = "----wardnet-test-boundary";
    // Only the passphrase field — no bundle.
    let body = multipart_body(boundary, None, Some("correct-horse-battery-staple"));

    let req = Request::builder()
        .method("POST")
        .uri("/api/backup/import/preview")
        .header(
            "Content-Type",
            format!("multipart/form-data; boundary={boundary}"),
        )
        .header("Authorization", "Bearer k")
        .extension(connect_info_ext())
        .body(Body::from(body))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
    let text = String::from_utf8_lossy(&body);
    assert!(
        text.contains("bundle"),
        "expected missing-bundle error, got: {text}"
    );
}

#[tokio::test]
async fn preview_import_rejects_missing_passphrase_field() {
    let backup = Arc::new(MockBackupService::ok_idle());
    let state = make_state(
        AlwaysAuthService {
            admin_id: Uuid::new_v4(),
        },
        backup,
    );
    let app = backup_app(state);

    let boundary = "----wardnet-test-boundary";
    // Only the bundle field — no passphrase.
    let body = multipart_body(boundary, Some(b"bytes"), None);

    let req = Request::builder()
        .method("POST")
        .uri("/api/backup/import/preview")
        .header(
            "Content-Type",
            format!("multipart/form-data; boundary={boundary}"),
        )
        .header("Authorization", "Bearer k")
        .extension(connect_info_ext())
        .body(Body::from(body))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
    let text = String::from_utf8_lossy(&body);
    assert!(
        text.contains("passphrase"),
        "expected missing-passphrase error, got: {text}"
    );
}

#[tokio::test]
async fn preview_import_ignores_unknown_multipart_fields() {
    let backup = Arc::new(MockBackupService::ok_idle());
    let state = make_state(
        AlwaysAuthService {
            admin_id: Uuid::new_v4(),
        },
        backup,
    );
    let app = backup_app(state);

    let boundary = "----wardnet-test-boundary";
    let mut body = Vec::new();
    // Unknown field should be skipped.
    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(b"Content-Disposition: form-data; name=\"extra\"\r\n\r\n");
    body.extend_from_slice(b"ignored");
    body.extend_from_slice(b"\r\n");
    // The expected fields.
    body.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"bundle\"; filename=\"b\"\r\n\
             Content-Type: application/octet-stream\r\n\r\nXYZ\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"passphrase\"\r\n\r\n\
             correct-horse-battery-staple\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());

    let req = Request::builder()
        .method("POST")
        .uri("/api/backup/import/preview")
        .header(
            "Content-Type",
            format!("multipart/form-data; boundary={boundary}"),
        )
        .header("Authorization", "Bearer k")
        .extension(connect_info_ext())
        .body(Body::from(body))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

// ---------------------------------------------------------------------------
// POST /api/backup/import/apply
// ---------------------------------------------------------------------------

#[tokio::test]
async fn apply_import_forwards_token_and_returns_200() {
    let backup = Arc::new(MockBackupService::ok_idle());
    let state = make_state(
        AlwaysAuthService {
            admin_id: Uuid::new_v4(),
        },
        backup.clone(),
    );
    let app = backup_app(state);

    let body = serde_json::json!({ "preview_token": "abc-123" });
    let req = Request::builder()
        .method("POST")
        .uri("/api/backup/import/apply")
        .header("Content-Type", "application/json")
        .header("Authorization", "Bearer k")
        .extension(connect_info_ext())
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), 8192).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["manifest"]["host_id"], "apply-host");
    assert!(!json["snapshots"].as_array().unwrap().is_empty());

    assert_eq!(
        backup.calls.lock().unwrap().apply_token.as_deref(),
        Some("abc-123")
    );
}

#[tokio::test]
async fn apply_import_requires_authentication() {
    let backup = Arc::new(MockBackupService::ok_idle());
    let state = make_state(NeverAuthService, backup);
    let app = backup_app(state);

    let body = serde_json::json!({ "preview_token": "t" });
    let req = Request::builder()
        .method("POST")
        .uri("/api/backup/import/apply")
        .header("Content-Type", "application/json")
        .extension(connect_info_ext())
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

// ---------------------------------------------------------------------------
// GET /api/backup/snapshots
// ---------------------------------------------------------------------------

#[tokio::test]
async fn list_snapshots_returns_retained_backups() {
    let backup = Arc::new(MockBackupService::ok_idle());
    let state = make_state(
        AlwaysAuthService {
            admin_id: Uuid::new_v4(),
        },
        backup,
    );
    let app = backup_app(state);

    let req = Request::builder()
        .uri("/api/backup/snapshots")
        .header("Authorization", "Bearer k")
        .extension(connect_info_ext())
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let snapshots = json["snapshots"].as_array().unwrap();
    assert_eq!(snapshots.len(), 1);
    assert_eq!(snapshots[0]["kind"], "database");
}

#[tokio::test]
async fn list_snapshots_requires_authentication() {
    let backup = Arc::new(MockBackupService::ok_idle());
    let state = make_state(NeverAuthService, backup);
    let app = backup_app(state);

    let req = Request::builder()
        .uri("/api/backup/snapshots")
        .extension(connect_info_ext())
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}
