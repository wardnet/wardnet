//! Tests for the request context task-local and Tower layer.

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::response::IntoResponse;
use axum::routing::get;
use tower::ServiceExt;

use crate::request_context::{self, RequestContextLayer, RequestId};

/// Handler that reads the task-local request ID and returns it.
async fn echo_request_id() -> impl IntoResponse {
    request_context::current_request_id().unwrap_or_else(|| "none".to_owned())
}

#[tokio::test]
async fn current_request_id_returns_none_outside_scope() {
    assert!(request_context::current_request_id().is_none());
}

#[tokio::test]
async fn with_request_id_sets_task_local() {
    let result = request_context::with_request_id("test-id-123".to_owned(), async {
        request_context::current_request_id()
    })
    .await;

    assert_eq!(result, Some("test-id-123".to_owned()));
}

#[tokio::test]
async fn layer_propagates_request_id_from_extensions() {
    let app = Router::new()
        .route("/test", get(echo_request_id))
        .layer(RequestContextLayer);

    let req = Request::builder()
        .uri("/test")
        .extension(RequestId("my-request-id".to_owned()))
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
    assert_eq!(String::from_utf8_lossy(&body), "my-request-id");
}

#[tokio::test]
async fn layer_defaults_to_empty_without_extension() {
    let app = Router::new()
        .route("/test", get(echo_request_id))
        .layer(RequestContextLayer);

    let req = Request::builder().uri("/test").body(Body::empty()).unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
    // Empty string because no RequestId extension was set.
    assert_eq!(String::from_utf8_lossy(&body), "");
}
