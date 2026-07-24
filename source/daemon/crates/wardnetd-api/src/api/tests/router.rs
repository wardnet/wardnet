//! Tests for the top-level application router built by [`crate::api::router`].
//!
//! These tests verify that the full router can be constructed and that every
//! registered route is reachable (i.e. returns something other than 404).
//!
//! We send HTTP `OPTIONS` requests because no route registers an `OPTIONS`
//! handler, so the request never reaches (and cannot panic) a stub service. A
//! matched path returns `405 Method Not Allowed` (the path exists, the method
//! doesn't); an unmatched path falls through to the static-file fallback (a
//! `308` redirect into `/app/`). The two are distinguishable by status.
//!
//! There is deliberately **no** CORS layer (see [`crate::api::router`]): every
//! served web surface is same-origin, and answering a cross-origin preflight
//! or emitting `Access-Control-Allow-Origin` would reopen the CWE-352 hole
//! against the IP-authenticated self-service endpoints. These tests assert that
//! no `access-control-allow-origin` header is ever emitted.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;
use wardnetd_services::entitlement::Entitlement;

use crate::tests::stubs::test_app_state;

/// Build the full application router from [`crate::api::router`].
fn full_router() -> axum::Router {
    crate::api::router(test_app_state())
}

/// Build the full application router with a premium-entitled `AppState`, for
/// tests exercising the static-file fallback's content-serving logic (which
/// the default not-entitled state would short-circuit with the `403`
/// premium-required gate before ever reaching it).
fn full_router_entitled() -> axum::Router {
    let entitlement = Entitlement::shared();
    entitlement.set_premium(true);
    crate::api::router(test_app_state().with_entitlement(entitlement))
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
/// With no CORS layer to intercept it, an `OPTIONS` to a registered path
/// returns `405 Method Not Allowed` (the path matched; no `OPTIONS` handler is
/// registered) rather than falling through to the static-file fallback. That
/// `405` is the proof the route exists. We additionally assert that the
/// response carries **no** `access-control-allow-origin` header, i.e. the
/// router never answers a cross-origin preflight (CWE-352 regression guard).
#[tokio::test]
async fn all_api_routes_are_reachable() {
    // Every route declared in api::router(). We use a dummy UUID for path
    // parameters — the route match only cares about the pattern, not the
    // value.
    let fake_uuid = "00000000-0000-0000-0000-000000000000";

    let routes: Vec<(&str, String)> = vec![
        ("POST", "/api/auth/login".to_owned()),
        ("POST", "/api/auth/logout".to_owned()),
        ("GET", "/api/devices".to_owned()),
        ("GET", "/api/devices/me".to_owned()),
        ("PUT", "/api/devices/me/rule".to_owned()),
        ("GET", "/api/devices/me/dns-events/stream".to_owned()),
        ("POST", "/api/devices/me/dns-events/ack".to_owned()),
        ("PATCH", "/api/devices/me/dns-capture".to_owned()),
        ("POST", "/api/devices/me/rule-requests".to_owned()),
        ("GET", "/api/devices/me/rule-requests".to_owned()),
        ("GET", "/api/rule-requests".to_owned()),
        ("PATCH", format!("/api/rule-requests/{fake_uuid}")),
        ("GET", format!("/api/devices/{fake_uuid}")),
        ("PUT", format!("/api/devices/{fake_uuid}")),
        ("GET", "/api/info".to_owned()),
        ("GET", "/api/setup/status".to_owned()),
        ("POST", "/api/setup".to_owned()),
        ("GET", "/api/system/status".to_owned()),
        ("GET", "/api/system/logs/stream".to_owned()),
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
            StatusCode::METHOD_NOT_ALLOWED,
            "OPTIONS for {method} {path} returned {status} (expected 405, proving the \
             path is registered and no CORS layer answers the preflight)"
        );

        // The router must never answer a cross-origin preflight: no
        // Access-Control-Allow-Origin header may be emitted (CWE-352 guard).
        assert!(
            !resp.headers().contains_key("access-control-allow-origin"),
            "cross-origin preflight was answered for {method} {path}"
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
        ("GET", "/api/system/logs/stream"),
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
/// through to the static file handler, which treats an unmatched path as a
/// legacy root path and permanently redirects it into `/app/`. This verifies
/// the fallback is wired up correctly. (With no CORS layer intercepting it, the
/// `OPTIONS` reaches the fallback and gets the same 308 as any other method.)
#[tokio::test]
async fn unknown_path_hits_fallback() {
    let status = options_status(full_router(), "/api/nonexistent/path").await;
    assert_eq!(status, StatusCode::PERMANENT_REDIRECT);
}

/// Paths outside every app surface — the user PWA's pre-move root tree — are
/// permanently redirected into `/app/`, preserving path and query so old
/// bookmarks, deep links, and installed start URLs keep landing; a slashless
/// scope root is canonicalized to its trailing-slash URL so the served
/// document is always inside its PWA scope. The redirects answer even when
/// the box is not entitled (their targets are what's gated), and carry a
/// bounded Cache-Control so a cached 308 can't permanently mask a path a
/// future release starts serving.
#[tokio::test]
async fn legacy_root_paths_redirect_into_app_scope() {
    for (path, target) in [
        ("/", "/app/"),
        ("/dns", "/app/dns"),
        ("/routing?device=ab", "/app/routing?device=ab"),
        ("/app", "/app/"),
        ("/app?x=1", "/app/?x=1"),
        ("/admin-app", "/admin-app/"),
        ("/admin", "/admin/"),
    ] {
        for router in [full_router(), full_router_entitled()] {
            let req = Request::builder()
                .method("GET")
                .uri(path)
                .body(Body::empty())
                .expect("valid request");
            let resp = router.oneshot(req).await.expect("router should respond");
            assert_eq!(
                resp.status(),
                StatusCode::PERMANENT_REDIRECT,
                "GET {path} should 308 into the /app/ scope"
            );
            let location = resp
                .headers()
                .get(axum::http::header::LOCATION)
                .and_then(|v| v.to_str().ok());
            assert_eq!(location, Some(target), "GET {path} Location header");
            let cache_control = resp
                .headers()
                .get(axum::http::header::CACHE_CONTROL)
                .and_then(|v| v.to_str().ok());
            assert_eq!(
                cache_control,
                Some("max-age=86400"),
                "GET {path} must bound how long the 308 may be cached"
            );
        }
    }
}

/// The `.info` sentinel embedded in each web tree (it keeps the `dist/`
/// directory tracked for rust-embed; see `source/*/dist/.info`) must never be
/// served. A GET to it returns 404, not the SPA shell.
#[tokio::test]
async fn info_sentinel_is_not_served() {
    for path in ["/app/.info", "/admin/.info", "/admin-app/.info"] {
        let req = Request::builder()
            .method("GET")
            .uri(path)
            .body(Body::empty())
            .expect("valid request");
        let status = full_router_entitled()
            .oneshot(req)
            .await
            .expect("router should respond")
            .status();
        assert_eq!(
            status,
            StatusCode::NOT_FOUND,
            "GET {path} should be 404 but returned {status}"
        );
    }
}

/// CWE-352 regression guard for the self-service, IP-authenticated endpoints.
///
/// Those endpoints (`PUT /api/devices/me/rule`, `PATCH
/// /api/devices/me/dns-capture`, the DNS-events stream/ack, rule-requests)
/// authenticate the caller by TCP peer IP — ambient authority a `SameSite` cookie
/// cannot protect. The daemon must therefore never help a cross-origin page
/// reach them: a cross-origin preflight must not be answered, and no actual
/// response may carry `Access-Control-Allow-Origin`. This test pins both so a
/// future re-introduction of a permissive/wildcard CORS layer fails CI.
#[tokio::test]
async fn cross_origin_requests_get_no_cors_headers() {
    // A preflight for the exact request the exploit used: PUT + JSON. Without a
    // CORS layer the browser gets a 405 with no allow-origin, so it never sends
    // the real PUT.
    let preflight = Request::builder()
        .method("OPTIONS")
        .uri("/api/devices/me/rule")
        .header("Origin", "http://attacker.example")
        .header("Access-Control-Request-Method", "PUT")
        .header("Access-Control-Request-Headers", "content-type")
        .body(Body::empty())
        .expect("valid request");
    let resp = full_router()
        .oneshot(preflight)
        .await
        .expect("router should respond");
    assert_eq!(
        resp.status(),
        StatusCode::METHOD_NOT_ALLOWED,
        "cross-origin preflight must not be answered with a success status"
    );
    assert!(
        !resp.headers().contains_key("access-control-allow-origin"),
        "preflight must not grant a cross-origin allow-origin"
    );

    // An actual cross-origin GET is still sent by the browser, but the response
    // must not carry allow-origin, so the attacker page cannot read it.
    let get = Request::builder()
        .method("GET")
        .uri("/api/info")
        .header("Origin", "http://attacker.example")
        .body(Body::empty())
        .expect("valid request");
    let resp = full_router()
        .oneshot(get)
        .await
        .expect("router should respond");
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(
        !resp.headers().contains_key("access-control-allow-origin"),
        "cross-origin GET response must not be readable cross-origin"
    );
}
