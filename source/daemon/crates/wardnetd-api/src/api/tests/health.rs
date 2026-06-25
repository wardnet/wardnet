//! Tests for the unauthenticated `GET /health` endpoint (issue #214).

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::get;
use tower::ServiceExt;

use wardnet_common::api::{HealthResponse, HealthStatusDto};
use wardnetd_services::HealthMonitor;
use wardnetd_services::health::checks::LivenessHealthCheck;
use wardnetd_services::health::{CheckOutcome, HealthCheck};

use crate::tests::stubs::test_app_state;

/// A check that is always down — drives the DOWN/503 path.
struct AlwaysDown;

#[async_trait]
impl HealthCheck for AlwaysDown {
    fn name(&self) -> &'static str {
        "always-down"
    }
    async fn check(&self) -> CheckOutcome {
        CheckOutcome::down("forced down for test")
    }
}

async fn health_app_with(monitor: HealthMonitor) -> Router {
    monitor.refresh().await;
    let state = test_app_state().with_health_monitor(Arc::new(monitor));
    Router::new()
        .route("/health", get(crate::api::health::health))
        .with_state(state)
}

async fn get_health(app: Router) -> (StatusCode, HealthResponse) {
    let req = Request::builder()
        .method("GET")
        .uri("/health")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let status = resp.status();
    let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
    let json: HealthResponse = serde_json::from_slice(&body).unwrap();
    (status, json)
}

#[tokio::test]
async fn returns_200_up_when_all_components_healthy() {
    let mut monitor = HealthMonitor::new(1, Duration::from_secs(2));
    monitor.register(Arc::new(LivenessHealthCheck));
    let app = health_app_with(monitor).await;

    let (status, body) = get_health(app).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body.status, HealthStatusDto::Up);
    assert!(
        body.components.iter().any(|c| c.name == "liveness"),
        "expected the liveness component in the response",
    );
    assert!(
        body.components
            .iter()
            .all(|c| c.status == HealthStatusDto::Up)
    );
}

#[tokio::test]
async fn returns_503_down_when_a_component_is_down() {
    let mut monitor = HealthMonitor::new(1, Duration::from_secs(2));
    monitor.register(Arc::new(LivenessHealthCheck));
    monitor.register(Arc::new(AlwaysDown));
    let app = health_app_with(monitor).await;

    let (status, body) = get_health(app).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body.status, HealthStatusDto::Down);

    let down = body
        .components
        .iter()
        .find(|c| c.name == "always-down")
        .expect("always-down component present");
    assert_eq!(down.status, HealthStatusDto::Down);
    assert_eq!(down.detail.as_deref(), Some("forced down for test"));
}

#[tokio::test]
async fn does_not_require_authentication() {
    let mut monitor = HealthMonitor::new(1, Duration::from_secs(2));
    monitor.register(Arc::new(LivenessHealthCheck));
    let app = health_app_with(monitor).await;

    // No cookie, no bearer token — must still answer.
    let (status, _) = get_health(app).await;
    assert_eq!(status, StatusCode::OK);
}
