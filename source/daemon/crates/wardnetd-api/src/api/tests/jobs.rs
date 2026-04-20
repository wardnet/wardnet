//! Tests for the `/api/jobs/{id}` polling endpoint.

use std::sync::Arc;

use async_trait::async_trait;
use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::get;
use tower::ServiceExt;
use uuid::Uuid;
use wardnet_common::jobs::{Job, JobKind, JobStatus};

use crate::state::AppState;
use crate::tests::stubs::{
    StubDeviceService, StubDhcpServer, StubDhcpService, StubDiscoveryService, StubDnsServer,
    StubDnsService, StubEventPublisher, StubLogService, StubProviderService, StubRoutingService,
    StubSystemService, StubTunnelService,
};
use wardnetd_services::AuthService;
use wardnetd_services::LogService;
use wardnetd_services::auth::service::LoginResult;
use wardnetd_services::error::AppError;
use wardnetd_services::jobs::{BoxedJobTask, JobService};

struct AlwaysAdminAuth;
#[async_trait]
impl AuthService for AlwaysAdminAuth {
    async fn login(&self, _u: &str, _p: &str) -> Result<LoginResult, AppError> {
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
        Ok(true)
    }
}

/// Fake job service with one preloaded job — lets us exercise the 200 path
/// without spinning up the real in-memory registry.
struct FixedJobService(Option<Job>);
#[async_trait]
impl JobService for FixedJobService {
    async fn dispatch_boxed(&self, _kind: JobKind, _task: BoxedJobTask) -> Uuid {
        unimplemented!()
    }
    async fn get(&self, _id: Uuid) -> Option<Job> {
        self.0.clone()
    }
}

fn build_state(job_service: Arc<dyn JobService>) -> AppState {
    AppState::new(
        Arc::new(AlwaysAdminAuth),
        Arc::new(StubDeviceService),
        Arc::new(StubDhcpService),
        Arc::new(StubDnsService) as Arc<dyn wardnetd_services::DnsService>,
        Arc::new(StubDiscoveryService),
        Arc::new(StubLogService) as Arc<dyn LogService>,
        Arc::new(StubProviderService),
        Arc::new(StubRoutingService),
        Arc::new(StubSystemService),
        Arc::new(StubTunnelService),
        Arc::new(StubDhcpServer),
        Arc::new(StubDnsServer),
        Arc::new(StubEventPublisher),
        job_service,
    )
}

fn jobs_app(job_service: Arc<dyn JobService>) -> Router {
    Router::new()
        .route("/api/jobs/{id}", get(crate::api::jobs::get_job))
        .layer(wardnetd_services::auth_context::AuthContextLayer)
        .with_state(build_state(job_service))
}

fn session_request(path: &str) -> Request<Body> {
    Request::builder()
        .method("GET")
        .uri(path)
        .header("cookie", "wardnet_session=t")
        .body(Body::empty())
        .unwrap()
}

#[tokio::test]
async fn get_job_returns_200_when_job_exists() {
    let now = chrono::Utc::now();
    let id = Uuid::new_v4();
    let job = Job {
        id,
        kind: JobKind::BlocklistRefresh,
        status: JobStatus::Running,
        percentage_done: 42,
        error: None,
        created_at: now,
        updated_at: now,
    };
    let svc: Arc<dyn JobService> = Arc::new(FixedJobService(Some(job)));
    let app = jobs_app(svc);

    let resp = app
        .oneshot(session_request(&format!("/api/jobs/{id}")))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
    let got: Job = serde_json::from_slice(&body).unwrap();
    assert_eq!(got.id, id);
    assert_eq!(got.percentage_done, 42);
    assert_eq!(got.status, JobStatus::Running);
}

#[tokio::test]
async fn get_job_returns_404_when_unknown() {
    let svc: Arc<dyn JobService> = Arc::new(FixedJobService(None));
    let app = jobs_app(svc);
    let id = Uuid::new_v4();

    let resp = app
        .oneshot(session_request(&format!("/api/jobs/{id}")))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}
