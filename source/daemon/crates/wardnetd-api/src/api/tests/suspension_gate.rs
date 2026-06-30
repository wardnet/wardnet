//! Router-level tests for the suspended-subscription serving gate.
//!
//! When the wardnet subscription is suspended, the two **premium** app surfaces
//! — the user PWA (`/`) and the admin mobile app (`/admin-app/`) — are short-
//! circuited with a `403` suspended page, while the admin **website** (`/admin/`)
//! and the `/api/*` surface stay reachable so the operator can always resubscribe.
//! See [`crate::web::static_handler`].

use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;
use wardnetd_services::entitlement::Entitlement;

use crate::state::AppState;
use crate::tests::stubs::test_app_state;

/// An [`AppState`] whose shared entitlement is in the suspended state.
fn suspended_state() -> AppState {
    let entitlement = Entitlement::shared();
    entitlement.suspend();
    test_app_state().with_entitlement(entitlement)
}

/// Drive a `GET path` through the full router and return (status, body bytes).
async fn get(state: AppState, path: &str) -> (StatusCode, Vec<u8>) {
    let app = crate::api::router(state);
    let req = Request::builder()
        .method("GET")
        .uri(path)
        .body(Body::empty())
        .expect("valid request");
    let resp = app.oneshot(req).await.expect("router should respond");
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .expect("body collected")
        .to_vec();
    (status, bytes)
}

#[tokio::test]
async fn suspended_blocks_the_user_pwa() {
    let (status, body) = get(suspended_state(), "/").await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(
        String::from_utf8_lossy(&body).contains("subscription"),
        "the user PWA root should serve the suspended page while suspended"
    );
}

#[tokio::test]
async fn suspended_blocks_the_admin_mobile_app() {
    let (status, _) = get(suspended_state(), "/admin-app/").await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn suspended_keeps_the_admin_website_reachable() {
    // `/admin/` must never be gated — it's how the operator resubscribes. The
    // embedded `dist/` may be the `.info`-only sentinel in test builds, so this
    // is a 404 rather than a 200; the point is that it is *not* the 403 gate.
    let (status, _) = get(suspended_state(), "/admin/").await;
    assert_ne!(
        status,
        StatusCode::FORBIDDEN,
        "the admin website must stay reachable while suspended"
    );
}

#[tokio::test]
async fn suspended_keeps_the_api_reachable() {
    // `/api/*` is real routing, not the static fallback, so the gate never sees
    // it. An admin-gated endpoint answers `401` (unauthenticated) rather than the
    // `403` suspended page — proving the gate didn't intercept it.
    let (status, _) = get(suspended_state(), "/api/openapi.json").await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn active_does_not_block_the_user_pwa() {
    // Active (default) state: the root is served normally. With an empty test
    // `dist/` that's a 404, but crucially never the 403 suspended page.
    let (status, _) = get(test_app_state(), "/").await;
    assert_ne!(status, StatusCode::FORBIDDEN);
}
