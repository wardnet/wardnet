//! Router-level tests for the premium-entitlement serving gate.
//!
//! Unless the box is **entitled** (on the wardnet DDNS provider and not
//! suspended), the two **premium** app surfaces — the user PWA (`/app/`) and the
//! admin mobile app (`/admin-app/`) — are short-circuited with a `403`
//! premium-required page, while the admin **website** (`/admin/`) and the
//! `/api/*` surface stay reachable so the operator can always (re)subscribe.
//! See [`crate::web::static_handler`].
//!
//! A box is not entitled for one of two reasons, both producing the same
//! `403`: it never subscribed at all (the default — `premium = false`), or a
//! prior wardnet subscription lapsed (`premium = true`, `suspended = true`).

use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;
use wardnetd_services::entitlement::Entitlement;

use crate::state::AppState;
use crate::tests::stubs::test_app_state;

/// An [`AppState`] whose shared entitlement is premium-enrolled and active.
fn premium_active_state() -> AppState {
    let entitlement = Entitlement::shared();
    entitlement.set_premium(true);
    test_app_state().with_entitlement(entitlement)
}

/// An [`AppState`] whose shared entitlement is premium-enrolled but suspended.
fn premium_suspended_state() -> AppState {
    let entitlement = Entitlement::shared();
    entitlement.set_premium(true);
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
async fn free_blocks_the_user_pwa() {
    // Default state: never subscribed / free BYO-domain — `premium = false`.
    let (status, body) = get(test_app_state(), "/app/").await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(
        String::from_utf8_lossy(&body).contains("Premium"),
        "the user PWA scope should serve the premium-required page when not entitled"
    );
}

#[tokio::test]
async fn free_blocks_the_admin_mobile_app() {
    let (status, _) = get(test_app_state(), "/admin-app/").await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn suspended_blocks_the_user_pwa() {
    let (status, body) = get(premium_suspended_state(), "/app/").await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(
        String::from_utf8_lossy(&body).contains("Premium"),
        "the user PWA scope should serve the premium-required page while suspended"
    );
}

#[tokio::test]
async fn suspended_blocks_the_admin_mobile_app() {
    let (status, _) = get(premium_suspended_state(), "/admin-app/").await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn not_entitled_keeps_the_admin_website_reachable() {
    // `/admin/` must never be gated — it's how the operator (re)subscribes. The
    // embedded `dist/` may be the `.info`-only sentinel in test builds, so this
    // is a 404 rather than a 200; the point is that it is *not* the 403 gate.
    for state in [test_app_state(), premium_suspended_state()] {
        let (status, _) = get(state, "/admin/").await;
        assert_ne!(
            status,
            StatusCode::FORBIDDEN,
            "the admin website must stay reachable regardless of entitlement"
        );
    }
}

#[tokio::test]
async fn not_entitled_keeps_the_api_reachable() {
    // `/api/*` is real routing, not the static fallback, so the gate never sees
    // it. An admin-gated endpoint answers `401` (unauthenticated) rather than the
    // `403` premium-required page — proving the gate didn't intercept it.
    for state in [test_app_state(), premium_suspended_state()] {
        let (status, _) = get(state, "/api/openapi.json").await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }
}

#[tokio::test]
async fn premium_active_does_not_block_the_user_pwa() {
    // Premium-enrolled and not suspended: the user PWA scope is served normally. With an
    // empty test `dist/` that's a 404, but crucially never the 403 gate.
    let (status, _) = get(premium_active_state(), "/app/").await;
    assert_ne!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn premium_active_does_not_block_the_admin_mobile_app() {
    let (status, _) = get(premium_active_state(), "/admin-app/").await;
    assert_ne!(status, StatusCode::FORBIDDEN);
}
