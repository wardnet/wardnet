//! Tests for the public info endpoint (GET /api/info).

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::get;
use tower::ServiceExt;
use wardnet_common::api::InfoResponse;

use crate::tests::stubs::test_app_state;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn info_app() -> Router {
    Router::new()
        .route("/api/info", get(crate::api::info::info))
        .with_state(test_app_state())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn info_returns_200_with_version_and_uptime() {
    let app = info_app();

    let req = Request::builder()
        .method("GET")
        .uri("/api/info")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
    let json: InfoResponse = serde_json::from_slice(&body).unwrap();

    assert!(!json.version.is_empty(), "version must be non-empty");
    // Uptime is computed from Instant::now() so it will be 0 or very small.
    // The important thing is it deserializes successfully as u64.
}

#[tokio::test]
async fn info_does_not_require_authentication() {
    let app = info_app();

    // Send a request with no cookies, no API key header — should still succeed.
    let req = Request::builder()
        .method("GET")
        .uri("/api/info")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}
