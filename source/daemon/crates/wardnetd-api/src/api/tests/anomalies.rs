//! Tests for the anomaly API endpoints (GET /api/anomalies, POST reevaluate).

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use std::time::Duration;

use async_trait::async_trait;
use axum::Router;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use axum::routing::{get, post};
use chrono::Utc;
use tower::ServiceExt;
use uuid::Uuid;
use wardnet_common::anomaly::{
    Anomaly, AnomalyFilter, AnomalyQueryStatus, AnomalyReport, AnomalyType, ReevaluateSummary,
};
use wardnet_common::auth::{AuthenticatedUser, UserRole};
use wardnet_test_support::principal;
use wardnetd_services::anomaly::AnomalyService;
use wardnetd_services::auth::service::LoginResult;
use wardnetd_services::auth::{CurrentUser, LoginAttempt};
use wardnetd_services::error::AppError;
use wardnetd_services::{AuthService, LogService};

use crate::state::AppState;
use crate::tests::stubs::{
    StubBackupService, StubDdnsService, StubDeviceService, StubDhcpServer, StubDhcpService,
    StubDiscoveryService, StubDnsFilterService, StubDnsLocalService, StubDnsServer, StubDnsService,
    StubEventPublisher, StubJobService, StubLogService, StubNetworkZoneService,
    StubProviderService, StubRoutingService, StubRuleRequestService, StubStatsService,
    StubSystemService, StubTlsService, StubTunnelService, StubUpdateService,
    StubZoneExceptionService,
};

/// Accepts the fixed session cookie the helpers below send.
struct AuthedAuthService;

#[async_trait]
impl AuthService for AuthedAuthService {
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
    async fn cleanup_expired_sessions(&self) -> Result<u64, AppError> {
        unimplemented!()
    }
}

/// Records the filter the handler built, so the query-parameter mapping is
/// asserted rather than assumed.
#[derive(Default)]
struct RecordingAnomalyService {
    anomalies: Vec<Anomaly>,
    last_filter: StdMutex<Option<(AnomalyFilter, u32)>>,
    reevaluations: StdMutex<u32>,
}

impl RecordingAnomalyService {
    fn with(anomalies: Vec<Anomaly>) -> Arc<Self> {
        Arc::new(Self {
            anomalies,
            ..Self::default()
        })
    }

    fn filter(&self) -> (AnomalyFilter, u32) {
        self.last_filter.lock().unwrap().clone().unwrap()
    }
}

#[async_trait]
impl AnomalyService for RecordingAnomalyService {
    async fn list(&self, filter: AnomalyFilter, limit: u32) -> Result<Vec<Anomaly>, AppError> {
        *self.last_filter.lock().unwrap() = Some((filter, limit));
        Ok(self.anomalies.clone())
    }
    async fn submit(&self, _r: AnomalyReport) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn resolve(&self, _id: Uuid) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn run_detector(&self, _t: AnomalyType) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn reevaluate_all(&self) -> Result<ReevaluateSummary, AppError> {
        *self.reevaluations.lock().unwrap() += 1;
        Ok(ReevaluateSummary {
            evaluated: 4,
            resolved: 1,
        })
    }
    fn schedule(&self) -> Vec<(AnomalyType, Duration)> {
        Vec::new()
    }
}

fn sample_anomaly() -> Anomaly {
    Anomaly {
        id: Uuid::nil(),
        anomaly_type: AnomalyType::BlocklistRefreshFailing,
        subject_id: Some("bl-1".to_owned()),
        message: "Blocklist \"HaGeZi\" has failed to refresh 7 times".to_owned(),
        details: Some(serde_json::json!({ "profile_id": "p-1" })),
        opened_at: Utc::now(),
        last_seen_at: Utc::now(),
        occurrences: 7,
        resolved_at: None,
    }
}

fn connect_info() -> ConnectInfo<SocketAddr> {
    ConnectInfo(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 12345))
}

fn build_state(auth: Arc<dyn AuthService>, anomalies: Arc<dyn AnomalyService>) -> AppState {
    AppState::new(
        auth,
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
        Arc::new(StubZoneExceptionService),
    )
    .with_anomaly_service(anomalies)
}

fn router(state: AppState) -> Router {
    Router::new()
        .route("/api/anomalies", get(crate::api::anomalies::list_anomalies))
        .route(
            "/api/anomalies/reevaluate",
            post(crate::api::anomalies::reevaluate_anomalies),
        )
        .with_state(state)
}

async fn send(
    app: Router,
    method: &str,
    uri: &str,
    authed: bool,
) -> (StatusCode, serde_json::Value) {
    let mut req = Request::builder()
        .method(method)
        .uri(uri)
        .extension(connect_info());
    if authed {
        req = req.header("Cookie", "wardnet_session=valid-token");
    }
    let resp = app.oneshot(req.body(Body::empty()).unwrap()).await.unwrap();
    let status = resp.status();
    let body = axum::body::to_bytes(resp.into_body(), 65536).await.unwrap();
    let json = serde_json::from_slice(&body).unwrap_or(serde_json::Value::Null);
    (status, json)
}

/// The API materialises severity/component/hint from the catalogue so clients
/// do not need their own copy of it.
#[tokio::test]
async fn list_returns_anomalies_with_catalogue_fields_filled_in() {
    let svc = RecordingAnomalyService::with(vec![sample_anomaly()]);
    let app = router(build_state(Arc::new(AuthedAuthService), svc));

    let (status, body) = send(app, "GET", "/api/anomalies", true).await;

    assert_eq!(status, StatusCode::OK);
    let a = &body["anomalies"][0];
    assert_eq!(a["type"], "blocklist_refresh_failing");
    assert_eq!(a["severity"], "error");
    assert_eq!(a["component"], "dns");
    assert!(a["hint"].as_str().unwrap().contains("Ad Blocking"));
    assert_eq!(a["subject_id"], "bl-1");
    assert_eq!(a["occurrences"], 7);
    assert!(a["resolved_at"].is_null());
    // `details` is what lets the web surfaces deep link to the subject.
    assert_eq!(a["details"]["profile_id"], "p-1");
}

#[tokio::test]
async fn list_defaults_to_open_anomalies_with_a_sane_limit() {
    let svc = RecordingAnomalyService::with(Vec::new());
    let app = router(build_state(Arc::new(AuthedAuthService), svc.clone()));

    let (status, _) = send(app, "GET", "/api/anomalies", true).await;

    assert_eq!(status, StatusCode::OK);
    let (filter, limit) = svc.filter();
    assert_eq!(filter.status, AnomalyQueryStatus::Open);
    assert!(filter.subject_id.is_none());
    assert_eq!(limit, 50);
}

#[tokio::test]
async fn list_passes_status_subject_and_limit_through() {
    let svc = RecordingAnomalyService::with(Vec::new());
    let app = router(build_state(Arc::new(AuthedAuthService), svc.clone()));

    let (status, _) = send(
        app,
        "GET",
        "/api/anomalies?status=all&subject_id=tunnel-9&limit=5",
        true,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let (filter, limit) = svc.filter();
    assert_eq!(filter.status, AnomalyQueryStatus::All);
    assert_eq!(filter.subject_id.as_deref(), Some("tunnel-9"));
    assert_eq!(limit, 5);
}

#[tokio::test]
async fn list_requires_authentication() {
    let svc = RecordingAnomalyService::with(Vec::new());
    let app = router(build_state(Arc::new(AuthedAuthService), svc));

    let (status, _) = send(app, "GET", "/api/anomalies", false).await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn reevaluate_reports_what_it_checked_and_closed() {
    let svc = RecordingAnomalyService::with(Vec::new());
    let app = router(build_state(Arc::new(AuthedAuthService), svc.clone()));

    let (status, body) = send(app, "POST", "/api/anomalies/reevaluate", true).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["evaluated"], 4);
    assert_eq!(body["resolved"], 1);
    assert_eq!(*svc.reevaluations.lock().unwrap(), 1);
}

#[tokio::test]
async fn reevaluate_requires_authentication() {
    let svc = RecordingAnomalyService::with(Vec::new());
    let app = router(build_state(Arc::new(AuthedAuthService), svc.clone()));

    let (status, _) = send(app, "POST", "/api/anomalies/reevaluate", false).await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(*svc.reevaluations.lock().unwrap(), 0);
}

/// An `AppState` without a real anomaly service must fail loudly rather than
/// return an empty list that reads as "nothing is wrong".
#[tokio::test]
async fn the_default_service_refuses_to_answer() {
    let state = AppState::new(
        Arc::new(AuthedAuthService),
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
        Arc::new(StubZoneExceptionService),
    );

    let (status, _) = send(router(state), "GET", "/api/anomalies", true).await;

    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
}
