//! Tests for [`crate::tls_server`]: placeholder-cert generation + `RustlsConfig`
//! build (which also exercises the crypto-provider install), the
//! provisioned/unprovisioned state of `build_tls_state`, the 503 guard
//! transition, and the `:80`→`:443` redirect.

use std::sync::Arc;
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
    build_tls_state, generate_placeholder_pem, guarded_https_app, install_crypto_provider,
    redirect_to_https,
};

static CRYPTO_INIT: Once = Once::new();

/// Install the crypto provider exactly once across the (parallel) test binary —
/// a second `install_default()` returns `Err`, so guard it behind `Once`.
fn ensure_crypto() {
    CRYPTO_INIT.call_once(install_crypto_provider);
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
async fn build_tls_state_placeholder_is_unprovisioned() {
    ensure_crypto();
    let (_config, provisioned) = build_tls_state(None).await.unwrap();
    assert!(!provisioned.load(Ordering::Acquire));
}

#[tokio::test]
async fn build_tls_state_with_seed_is_provisioned() {
    ensure_crypto();
    let (cert, key) = generate_placeholder_pem().unwrap();
    let (_config, provisioned) = build_tls_state(Some((cert, key))).await.unwrap();
    assert!(provisioned.load(Ordering::Acquire));
}

fn test_app() -> Router {
    Router::new().route("/ping", get(|| async { "pong" }))
}

#[tokio::test]
async fn guard_returns_503_until_provisioned() {
    let provisioned = Arc::new(AtomicBool::new(false));
    let app = guarded_https_app(test_app(), provisioned.clone());

    let resp = app
        .clone()
        .oneshot(Request::builder().uri("/ping").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);

    // Flip the flag → the same app now passes through.
    provisioned.store(true, Ordering::Release);
    let resp = app
        .oneshot(Request::builder().uri("/ping").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[test]
fn redirect_upgrades_to_https_same_host() {
    let mut headers = HeaderMap::new();
    headers.insert(header::HOST, "home.example.net".parse().unwrap());
    let uri: Uri = "/setup?step=2".parse().unwrap();

    let resp = redirect_to_https(443, &headers, &uri);
    assert_eq!(resp.status(), StatusCode::PERMANENT_REDIRECT);
    assert_eq!(
        resp.headers().get(header::LOCATION).unwrap(),
        "https://home.example.net/setup?step=2"
    );
}

#[test]
fn redirect_includes_non_default_https_port() {
    let mut headers = HeaderMap::new();
    headers.insert(header::HOST, "home.example.net:80".parse().unwrap());
    let uri: Uri = "/".parse().unwrap();

    let resp = redirect_to_https(8443, &headers, &uri);
    assert_eq!(
        resp.headers().get(header::LOCATION).unwrap(),
        "https://home.example.net:8443/"
    );
}
