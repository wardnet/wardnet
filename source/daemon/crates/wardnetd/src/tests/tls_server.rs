//! Tests for [`crate::tls_server`]: placeholder-cert generation + `RustlsConfig`
//! build (which also exercises the crypto-provider install), the serving
//! identity (`is_provisioned` / `canonical_fqdn`) produced by
//! `build_serving_control` and flipped by `ServingControl::activate`, the 503
//! guard transition, and the `:80` redirect (provisioned: same-host HTTPS upgrade
//! and short-name → canonical-FQDN rewrite; unprovisioned: downgrade to the
//! plain-HTTP admin port with the root landing on `/admin/`).

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::Once;
use std::sync::atomic::{AtomicBool, Ordering};

use axum::Router;
use axum::body::Body;
use axum::extract::Request;
use axum::http::{HeaderMap, StatusCode, Uri, header};
use axum::routing::get;
use axum_server::tls_rustls::RustlsConfig;
use tower::ServiceExt;

use crate::tls_server::{
    ServingIdentity, build_serving_control, generate_placeholder_pem, guarded_https_app,
    install_crypto_provider, redirect_router, redirect_to_https,
};

static CRYPTO_INIT: Once = Once::new();

/// Install the crypto provider exactly once across the (parallel) test binary —
/// a second `install_default()` returns `Err`, so guard it behind `Once`.
fn ensure_crypto() {
    CRYPTO_INIT.call_once(install_crypto_provider);
}

/// A controllable [`ServingIdentity`] for the guard test — lets a test flip the
/// provisioning state without a real cert reload.
#[derive(Default)]
struct FakeServing {
    provisioned: AtomicBool,
    fqdn: Mutex<Option<String>>,
}

impl ServingIdentity for FakeServing {
    fn is_provisioned(&self) -> bool {
        self.provisioned.load(Ordering::Acquire)
    }
    fn canonical_fqdn(&self) -> Option<Arc<String>> {
        self.fqdn.lock().unwrap().clone().map(Arc::new)
    }
}

#[tokio::test]
async fn placeholder_cert_builds_a_valid_rustls_config() {
    ensure_crypto();
    // Covers placeholder generation + that the crypto provider is installed
    // (RustlsConfig::from_pem builds a ServerConfig, which panics without a
    // provider).
    let (cert, key) = generate_placeholder_pem().unwrap();
    let config = RustlsConfig::from_pem(cert, key).await;
    assert!(config.is_ok(), "placeholder PEM must build a RustlsConfig");
}

#[tokio::test]
async fn placeholder_serving_control_is_unprovisioned() {
    ensure_crypto();
    let (_config, control) = build_serving_control(None, None).await.unwrap();
    assert!(!control.is_provisioned());
    assert_eq!(control.canonical_fqdn(), None);
}

#[tokio::test]
async fn seeded_serving_control_is_provisioned_with_domain() {
    ensure_crypto();
    let (cert, key) = generate_placeholder_pem().unwrap();
    let (_config, control) =
        build_serving_control(Some((cert, key)), Some("home.example.net".to_owned()))
            .await
            .unwrap();
    assert!(control.is_provisioned());
    assert_eq!(
        control.canonical_fqdn(),
        Some(Arc::new("home.example.net".to_owned()))
    );
}

#[tokio::test]
async fn activate_sets_domain_and_lifts_gate() {
    use wardnetd_services::CertActivator;
    ensure_crypto();
    let (_config, control) = build_serving_control(None, None).await.unwrap();
    assert!(!control.is_provisioned());

    let (cert, key) = generate_placeholder_pem().unwrap();
    control
        .activate(cert, key, "home.example.net".to_owned())
        .await
        .unwrap();

    assert!(control.is_provisioned());
    assert_eq!(
        control.canonical_fqdn(),
        Some(Arc::new("home.example.net".to_owned()))
    );
}

fn test_app() -> Router {
    Router::new().route("/ping", get(|| async { "pong" }))
}

/// A [`ServingIdentity`] that panics when its canonical FQDN is read — used to
/// inject a realistic handler panic into the `:80` redirect router (the redirect
/// fallback calls `canonical_fqdn()` on every request).
struct PanicServing;

impl ServingIdentity for PanicServing {
    fn is_provisioned(&self) -> bool {
        false
    }
    fn canonical_fqdn(&self) -> Option<Arc<String>> {
        panic!("simulated serving-identity panic")
    }
}

/// The `:80` HTTP→HTTPS redirect listener must isolate handler panics as a `500`
/// — like the main API — instead of letting them unwind the connection task. A
/// panic in the redirect path (here, reading the canonical FQDN) must not
/// propagate past the service.
#[tokio::test]
async fn redirect_router_isolates_handler_panics() {
    let app = redirect_router(443, 7411, Arc::new(PanicServing));

    let req = Request::builder()
        .method("GET")
        .uri("/")
        .header(header::HOST, "home.example.net")
        .body(Body::empty())
        .unwrap();

    let resp = app
        .oneshot(req)
        .await
        .expect("redirect handler panic must not propagate past the service");

    assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn guard_returns_503_until_provisioned() {
    let serving = Arc::new(FakeServing::default());
    let app = guarded_https_app(test_app(), serving.clone());

    let resp = app
        .clone()
        .oneshot(Request::builder().uri("/ping").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);

    // Flip the flag → the same app now passes through.
    serving.provisioned.store(true, Ordering::Release);
    let resp = app
        .oneshot(Request::builder().uri("/ping").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[test]
fn redirect_downgrades_root_to_admin_port_when_unprovisioned() {
    // No cert exists for any name yet, so the friendly-name root lands on the
    // plain-HTTP admin site — the only surface reachable before a subscription.
    let mut headers = HeaderMap::new();
    headers.insert(header::HOST, "wardnet.lan".parse().unwrap());
    let uri: Uri = "/".parse().unwrap();

    let resp = redirect_to_https(443, 7411, None, &headers, &uri);
    // Provisioning is mutable (a cert can appear), so the downgrade is a 307.
    assert_eq!(resp.status(), StatusCode::TEMPORARY_REDIRECT);
    assert_eq!(
        resp.headers().get(header::LOCATION).unwrap(),
        "http://wardnet.lan:7411/admin/"
    );
}

#[test]
fn redirect_unprovisioned_preserves_root_query_onto_admin() {
    let mut headers = HeaderMap::new();
    headers.insert(header::HOST, "wardnet".parse().unwrap());
    let uri: Uri = "/?ref=setup".parse().unwrap();

    let resp = redirect_to_https(443, 7411, None, &headers, &uri);
    assert_eq!(
        resp.headers().get(header::LOCATION).unwrap(),
        "http://wardnet:7411/admin/?ref=setup"
    );
}

#[test]
fn redirect_unprovisioned_preserves_premium_surface_path() {
    // An explicit `/app/` (or `/admin-app/`) path is passed through untouched so
    // the plain-HTTP app router can answer with the premium-required page.
    let mut headers = HeaderMap::new();
    headers.insert(header::HOST, "wardnet".parse().unwrap());
    let uri: Uri = "/app/dashboard".parse().unwrap();

    let resp = redirect_to_https(443, 7411, None, &headers, &uri);
    assert_eq!(
        resp.headers().get(header::LOCATION).unwrap(),
        "http://wardnet:7411/app/dashboard"
    );
}

#[test]
fn redirect_rewrites_short_name_to_canonical_fqdn() {
    let mut headers = HeaderMap::new();
    headers.insert(header::HOST, "wardnet".parse().unwrap());
    let uri: Uri = "/dashboard".parse().unwrap();

    let resp = redirect_to_https(
        443,
        7411,
        Some(Arc::new("home.example.net".to_owned())),
        &headers,
        &uri,
    );
    // A rewrite to the mutable canonical FQDN is a 307, not a cacheable 308.
    assert_eq!(resp.status(), StatusCode::TEMPORARY_REDIRECT);
    assert_eq!(
        resp.headers().get(header::LOCATION).unwrap(),
        "https://home.example.net/dashboard"
    );
}

#[test]
fn redirect_rewrite_includes_non_default_https_port() {
    let mut headers = HeaderMap::new();
    headers.insert(header::HOST, "wardnet".parse().unwrap());
    let uri: Uri = "/".parse().unwrap();

    let resp = redirect_to_https(
        8443,
        7411,
        Some(Arc::new("home.example.net".to_owned())),
        &headers,
        &uri,
    );
    assert_eq!(
        resp.headers().get(header::LOCATION).unwrap(),
        "https://home.example.net:8443/"
    );
}

#[test]
fn redirect_keeps_same_host_when_already_canonical() {
    let mut headers = HeaderMap::new();
    headers.insert(header::HOST, "home.example.net".parse().unwrap());
    let uri: Uri = "/".parse().unwrap();

    // Host already equals the canonical FQDN → plain upgrade (permanent 308), no rewrite churn.
    let resp = redirect_to_https(
        443,
        7411,
        Some(Arc::new("home.example.net".to_owned())),
        &headers,
        &uri,
    );
    assert_eq!(resp.status(), StatusCode::PERMANENT_REDIRECT);
    assert_eq!(
        resp.headers().get(header::LOCATION).unwrap(),
        "https://home.example.net/"
    );
}

#[test]
fn redirect_same_host_upgrade_includes_non_default_https_port() {
    // The same-host upgrade arm (host already the canonical FQDN) must also append
    // a non-default HTTPS port — regression guard distinct from the rewrite arm.
    let mut headers = HeaderMap::new();
    headers.insert(header::HOST, "home.example.net".parse().unwrap());
    let uri: Uri = "/x".parse().unwrap();

    let resp = redirect_to_https(
        8443,
        7411,
        Some(Arc::new("home.example.net".to_owned())),
        &headers,
        &uri,
    );
    assert_eq!(resp.status(), StatusCode::PERMANENT_REDIRECT);
    assert_eq!(
        resp.headers().get(header::LOCATION).unwrap(),
        "https://home.example.net:8443/x"
    );
}

#[test]
fn redirect_missing_host_is_bad_request() {
    let headers = HeaderMap::new();
    let uri: Uri = "/".parse().unwrap();
    let resp = redirect_to_https(
        443,
        7411,
        Some(Arc::new("home.example.net".to_owned())),
        &headers,
        &uri,
    );
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[test]
fn redirect_preserves_ipv6_literal_host() {
    // A bracketed IPv6 Host with a port must keep its brackets and drop only the
    // port — `split(':')` would otherwise mangle it to "[". Exercised on the
    // unprovisioned downgrade, which shares the host-parsing path.
    let mut headers = HeaderMap::new();
    headers.insert(header::HOST, "[2001:db8::1]:80".parse().unwrap());
    let uri: Uri = "/x".parse().unwrap();

    let resp = redirect_to_https(443, 7411, None, &headers, &uri);
    assert_eq!(resp.status(), StatusCode::TEMPORARY_REDIRECT);
    assert_eq!(
        resp.headers().get(header::LOCATION).unwrap(),
        "http://[2001:db8::1]:7411/x"
    );
}
