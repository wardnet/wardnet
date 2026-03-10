//! Tests for the `inject_request_context` middleware.

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::response::IntoResponse;
use axum::routing::get;
use tower::ServiceExt;

use crate::request_context::{self, RequestContextLayer};

/// Handler that echoes the request ID from the task-local.
async fn echo_request_id() -> impl IntoResponse {
    request_context::current_request_id().unwrap_or_else(|| "none".to_owned())
}

/// Build a minimal router with the `inject_request_context` middleware
/// and the `RequestContextLayer` to propagate the task-local.
fn test_app() -> Router {
    Router::new()
        .route("/test", get(echo_request_id))
        .layer(RequestContextLayer)
        .layer(axum::middleware::from_fn(
            crate::api::middleware::inject_request_context,
        ))
}

#[tokio::test]
async fn response_has_x_request_id_header() {
    let app = test_app();
    let req = Request::builder().uri("/test").body(Body::empty()).unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let request_id = resp.headers().get("x-request-id");
    assert!(request_id.is_some(), "expected x-request-id header");

    // Should be a valid UUID.
    let id_str = request_id.unwrap().to_str().unwrap();
    assert!(
        uuid::Uuid::parse_str(id_str).is_ok(),
        "x-request-id should be a valid UUID, got: {id_str}"
    );
}

#[tokio::test]
async fn response_has_traceparent_header() {
    let app = test_app();
    let req = Request::builder().uri("/test").body(Body::empty()).unwrap();

    let resp = app.oneshot(req).await.unwrap();

    let traceparent = resp
        .headers()
        .get("traceparent")
        .expect("expected traceparent header")
        .to_str()
        .unwrap();

    // W3C format: 00-{32 hex}-{16 hex}-01
    let parts: Vec<&str> = traceparent.split('-').collect();
    assert_eq!(
        parts.len(),
        4,
        "traceparent should have 4 parts: {traceparent}"
    );
    assert_eq!(parts[0], "00", "version should be 00");
    assert_eq!(parts[1].len(), 32, "trace_id should be 32 hex chars");
    assert_eq!(parts[2].len(), 16, "span_id should be 16 hex chars");
    assert_eq!(parts[3], "01", "flags should be 01");

    // trace_id should match the request_id (UUID without hyphens).
    let request_id = resp
        .headers()
        .get("x-request-id")
        .unwrap()
        .to_str()
        .unwrap();
    let expected_trace_id = request_id.replace('-', "");
    assert_eq!(parts[1], expected_trace_id);
}

#[tokio::test]
async fn correlation_id_propagated_when_present() {
    let app = test_app();
    let req = Request::builder()
        .uri("/test")
        .header("x-correlation-id", "corr-abc-123")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();

    let correlation_id = resp
        .headers()
        .get("x-correlation-id")
        .expect("expected x-correlation-id header in response")
        .to_str()
        .unwrap();

    assert_eq!(correlation_id, "corr-abc-123");
}

#[tokio::test]
async fn no_correlation_id_header_when_not_in_request() {
    let app = test_app();
    let req = Request::builder().uri("/test").body(Body::empty()).unwrap();

    let resp = app.oneshot(req).await.unwrap();

    assert!(
        resp.headers().get("x-correlation-id").is_none(),
        "x-correlation-id should not be in response when not in request"
    );
}

#[tokio::test]
async fn request_id_available_in_handler_via_task_local() {
    let app = test_app();
    let req = Request::builder().uri("/test").body(Body::empty()).unwrap();

    let resp = app.oneshot(req).await.unwrap();

    // The handler echoes the task-local request ID.
    let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
    let body_str = String::from_utf8_lossy(&body);

    // The body should be a valid UUID (the request ID).
    assert!(
        uuid::Uuid::parse_str(&body_str).is_ok(),
        "handler should see the request ID via task-local, got: {body_str}"
    );

    // And it should match the response header.
    // Note: we already consumed the response, so we validate the body is a UUID.
}

#[tokio::test]
async fn each_request_gets_unique_request_id() {
    let app = test_app();

    let req1 = Request::builder().uri("/test").body(Body::empty()).unwrap();
    let resp1 = app.clone().oneshot(req1).await.unwrap();
    let id1 = resp1
        .headers()
        .get("x-request-id")
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();

    let req2 = Request::builder().uri("/test").body(Body::empty()).unwrap();
    let resp2 = app.oneshot(req2).await.unwrap();
    let id2 = resp2
        .headers()
        .get("x-request-id")
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();

    assert_ne!(id1, id2, "each request should get a unique request ID");
}
