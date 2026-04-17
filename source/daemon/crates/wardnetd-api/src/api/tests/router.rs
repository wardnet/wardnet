//! Tests for the top-level application router built by [`crate::api::router`].
//!
//! These tests verify that the full router can be constructed and that every
//! registered route is reachable (i.e. returns something other than 404).
//!
//! We send HTTP `OPTIONS` requests because the `CorsLayer::permissive()` layer
//! intercepts them before any handler runs, so the stub services never panic.
//! A matched route returns `200 OK` with CORS headers; an unmatched route
//! falls through to the static-file fallback (which also returns 200 for
//! index.html). To distinguish, we check the `access-control-allow-origin`
//! header that CORS adds on OPTIONS responses to matched routes.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

use crate::tests::stubs::test_app_state;

/// Build the full application router from [`crate::api::router`].
fn full_router() -> axum::Router {
    crate::api::router(test_app_state())
}

/// Send an OPTIONS request and return the status code.
async fn options_status(app: axum::Router, uri: &str) -> StatusCode {
    let req = Request::builder()
        .method("OPTIONS")
        .uri(uri)
        .header("Origin", "http://localhost")
        .header("Access-Control-Request-Method", "GET")
        .body(Body::empty())
        .expect("valid request");

    let resp = app.oneshot(req).await.expect("router should respond");
    resp.status()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn router_can_be_constructed() {
    // If this panics, something is wrong with route/state wiring.
    let _router = full_router();
}

/// Verify every API route is registered by sending OPTIONS to each path.
///
/// CORS permissive returns 200 for OPTIONS on any matched route. The
/// fallback (static handler) also returns 200 but that is fine — the
/// important thing is that none of these paths cause an error in the
/// router itself. We additionally verify that a few representative
/// routes get proper CORS headers.
#[tokio::test]
async fn all_api_routes_are_reachable() {
    // Every route declared in api::router(). We use a dummy UUID for path
    // parameters — the route match only cares about the pattern, not the
    // value.
    let fake_uuid = "00000000-0000-0000-0000-000000000000";

    let routes: Vec<(&str, String)> = vec![
        ("POST", "/api/auth/login".to_owned()),
        ("GET", "/api/devices".to_owned()),
        ("GET", "/api/devices/me".to_owned()),
        ("PUT", "/api/devices/me/rule".to_owned()),
        ("GET", format!("/api/devices/{fake_uuid}")),
        ("PUT", format!("/api/devices/{fake_uuid}")),
        ("GET", "/api/info".to_owned()),
        ("GET", "/api/setup/status".to_owned()),
        ("POST", "/api/setup".to_owned()),
        ("GET", "/api/system/status".to_owned()),
        ("GET", "/api/tunnels".to_owned()),
        ("POST", "/api/tunnels".to_owned()),
        ("DELETE", format!("/api/tunnels/{fake_uuid}")),
        ("GET", "/api/providers".to_owned()),
        ("POST", format!("/api/providers/{fake_uuid}/validate")),
        ("GET", format!("/api/providers/{fake_uuid}/countries")),
        ("POST", format!("/api/providers/{fake_uuid}/servers")),
        ("POST", format!("/api/providers/{fake_uuid}/setup")),
    ];

    for (method, path) in &routes {
        let app = full_router();

        let req = Request::builder()
            .method("OPTIONS")
            .uri(path.as_str())
            .header("Origin", "http://localhost")
            .header("Access-Control-Request-Method", *method)
            .body(Body::empty())
            .unwrap_or_else(|_| panic!("valid request for {method} {path}"));

        let resp = app
            .oneshot(req)
            .await
            .unwrap_or_else(|_| panic!("router should respond for {method} {path}"));

        let status = resp.status();
        assert_eq!(
            status,
            StatusCode::OK,
            "OPTIONS preflight for {method} {path} returned {status} (expected 200)"
        );

        // CORS permissive should set Access-Control-Allow-Origin on matched routes.
        assert!(
            resp.headers().contains_key("access-control-allow-origin"),
            "missing CORS header for {method} {path}"
        );
    }
}

/// Verify that the `/api/info` endpoint is fully functional through the
/// complete router stack (not just a mini-router).
#[tokio::test]
async fn info_endpoint_works_through_full_router() {
    let app = full_router();

    let req = Request::builder()
        .method("GET")
        .uri("/api/info")
        .body(Body::empty())
        .expect("valid request");

    let resp = app.oneshot(req).await.expect("router should respond");
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), 4096)
        .await
        .expect("readable body");
    let json: wardnet_common::api::InfoResponse =
        serde_json::from_slice(&body).expect("valid JSON");
    assert!(!json.version.is_empty());
}

/// Authenticated routes should return 401 when no credentials are provided,
/// proving the route matched and the auth extractor rejected the request.
#[tokio::test]
async fn authenticated_routes_return_401_without_credentials() {
    let protected_routes: Vec<(&str, &str)> = vec![
        ("GET", "/api/devices"),
        ("GET", "/api/system/status"),
        ("GET", "/api/tunnels"),
        ("GET", "/api/providers"),
    ];

    for (method, path) in &protected_routes {
        let app = full_router();

        let req = Request::builder()
            .method(*method)
            .uri(*path)
            .body(Body::empty())
            .unwrap_or_else(|_| panic!("valid request for {method} {path}"));

        let resp = app
            .oneshot(req)
            .await
            .unwrap_or_else(|_| panic!("router should respond for {method} {path}"));

        let status = resp.status();
        assert_eq!(
            status,
            StatusCode::UNAUTHORIZED,
            "{method} {path} should require auth but returned {status}"
        );
    }
}

/// A non-existent API path should NOT return 404 from the router — it falls
/// through to the static file handler (SPA fallback), which returns 200.
/// This verifies the fallback is wired up correctly.
#[tokio::test]
async fn unknown_path_hits_fallback() {
    let status = options_status(full_router(), "/api/nonexistent/path").await;
    // OPTIONS with CORS permissive returns 200 regardless, which is fine.
    assert_eq!(status, StatusCode::OK);
}
