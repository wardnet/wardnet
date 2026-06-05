//! Daemon-owned TLS serving primitives for `main.rs`.
//!
//! `wardnetd` terminates TLS itself (no Caddy). The `:443` listener is **always
//! bound** — at boot with a throwaway placeholder self-signed cert — and a
//! shared `provisioned` flag gates a 503 guard on every `:443` route until a
//! real cert is loaded. The pre-provisioning admin surface is plain HTTP on
//! `:7411` (unguarded). `:80` 308-redirects to HTTPS.
//!
//! This module owns the `axum-server` dependency (the services crate stays
//! serving-agnostic) and provides the [`CertActivator`] impl that the TLS
//! service injects: [`CertActivatorImpl::activate`] hot-swaps the live cert via
//! `RustlsConfig::reload_from_pem` and flips the `provisioned` flag.

use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use axum::Router;
use axum::extract::Request;
use axum::http::{HeaderMap, StatusCode, Uri, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum_server::Handle;
use axum_server::tls_rustls::RustlsConfig;
use tokio_util::sync::CancellationToken;
use tracing::Instrument;
use wardnetd_services::CertActivator;

/// How long in-flight `:443` connections get to drain on shutdown.
const GRACEFUL_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(10);

/// SNI / SAN placed on the placeholder cert. Never validated by a client (every
/// `:443` route returns 503 until a real cert loads), so the name is cosmetic.
const PLACEHOLDER_SAN: &str = "wardnet.invalid";

/// Install the aws-lc-rs crypto provider as the process default.
///
/// Both `ring` and `aws-lc-rs` are in the dependency tree, so rustls 0.23
/// cannot auto-pick a provider — without this the first `ServerConfig` build
/// panics at runtime (it compiles clean). Idempotent: a redundant call (e.g.
/// from a test) is ignored.
pub fn install_crypto_provider() {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
}

/// Generate a throwaway self-signed cert + key (PEM) for the placeholder `:443`
/// config used before a real cert is issued.
pub(crate) fn generate_placeholder_pem() -> anyhow::Result<(Vec<u8>, Vec<u8>)> {
    let key_pair = rcgen::KeyPair::generate()?;
    let params = rcgen::CertificateParams::new(vec![PLACEHOLDER_SAN.to_owned()])?;
    let cert = params.self_signed(&key_pair)?;
    Ok((
        cert.pem().into_bytes(),
        key_pair.serialize_pem().into_bytes(),
    ))
}

/// Build the shared `:443` [`RustlsConfig`] and `provisioned` flag.
///
/// Seeds from `seed` (the stored real cert) when present — `provisioned = true`
/// — otherwise from a freshly generated placeholder cert with `provisioned =
/// false`. The returned config is cloned into both the `:443` listener and the
/// [`CertActivatorImpl`], which share its internal `ArcSwap` for lock-free
/// reloads.
pub async fn build_tls_state(
    seed: Option<(Vec<u8>, Vec<u8>)>,
) -> anyhow::Result<(RustlsConfig, Arc<AtomicBool>)> {
    let (provisioned, cert, key) = if let Some((cert, key)) = seed {
        (true, cert, key)
    } else {
        let (cert, key) = generate_placeholder_pem()?;
        (false, cert, key)
    };
    let config = RustlsConfig::from_pem(cert, key).await?;
    Ok((config, Arc::new(AtomicBool::new(provisioned))))
}

/// Hot-swaps the live `:443` certificate and lifts the provisioning gate.
/// Injected into the TLS service as `Arc<dyn CertActivator>`.
pub struct CertActivatorImpl {
    config: RustlsConfig,
    provisioned: Arc<AtomicBool>,
}

impl CertActivatorImpl {
    #[must_use]
    pub fn new(config: RustlsConfig, provisioned: Arc<AtomicBool>) -> Self {
        Self {
            config,
            provisioned,
        }
    }
}

#[async_trait]
impl CertActivator for CertActivatorImpl {
    async fn activate(&self, chain_pem: Vec<u8>, key_pem: Vec<u8>) -> anyhow::Result<()> {
        self.config.reload_from_pem(chain_pem, key_pem).await?;
        // Release pairs with the guard's Acquire load: a reader that observes
        // `true` is guaranteed to see the reloaded cert. Renewal re-activates
        // with the flag already `true` — idempotent.
        self.provisioned.store(true, Ordering::Release);
        tracing::info!("activated TLS certificate on :443; provisioning gate lifted");
        Ok(())
    }
}

/// Wrap `app` with the 503 guard: until `provisioned` is set, every request is
/// short-circuited with a 503 pointing at the plain-HTTP fallback. Applied only
/// to the `:443` app — `:7411` is never guarded.
pub fn guarded_https_app(app: Router, provisioned: Arc<AtomicBool>) -> Router {
    app.layer(axum::middleware::from_fn(
        move |req: Request, next: Next| {
            let provisioned = provisioned.clone();
            async move {
                if provisioned.load(Ordering::Acquire) {
                    next.run(req).await
                } else {
                    unprovisioned_response()
                }
            }
        },
    ))
}

fn unprovisioned_response() -> Response {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        "TLS not provisioned yet — use the plain-HTTP admin endpoint at \
         http://<lan>:7411\n",
    )
        .into_response()
}

/// Spawn the always-bound `:443` HTTPS listener serving `app` (already wrapped
/// by [`guarded_https_app`]). Graceful shutdown is driven off `shutdown`. A bind
/// failure is logged, not fatal — `:7411` keeps serving.
pub fn spawn_https_listener(
    addr: SocketAddr,
    app: Router,
    config: RustlsConfig,
    shutdown: &CancellationToken,
    parent: &tracing::Span,
) -> tokio::task::JoinHandle<()> {
    let handle = Handle::new();
    {
        let handle = handle.clone();
        let shutdown = shutdown.clone();
        tokio::spawn(async move {
            shutdown.cancelled().await;
            handle.graceful_shutdown(Some(GRACEFUL_SHUTDOWN_TIMEOUT));
        });
    }

    let span = tracing::info_span!(parent: parent, "https_server");
    tokio::spawn(
        async move {
            tracing::info!(%addr, "HTTPS listener starting");
            if let Err(e) = axum_server::bind_rustls(addr, config)
                .handle(handle)
                .serve(app.into_make_service_with_connect_info::<SocketAddr>())
                .await
            {
                tracing::error!(error = %e, %addr, "HTTPS listener failed");
            }
        }
        .instrument(span),
    )
}

/// Spawn the `:80` listener that 308-redirects every request to HTTPS on the
/// same host. A bind failure is logged, not fatal.
pub fn spawn_http_redirect_listener(
    addr: SocketAddr,
    https_port: u16,
    shutdown: &CancellationToken,
    parent: &tracing::Span,
) -> tokio::task::JoinHandle<()> {
    let app = redirect_router(https_port);
    let shutdown = shutdown.clone();
    let span = tracing::info_span!(parent: parent, "http_redirect_server");
    tokio::spawn(
        async move {
            let listener = match tokio::net::TcpListener::bind(addr).await {
                Ok(listener) => listener,
                Err(e) => {
                    tracing::error!(error = %e, %addr, "failed to bind :80 redirect listener");
                    return;
                }
            };
            tracing::info!(%addr, "HTTP→HTTPS redirect listener starting");
            let result = axum::serve(listener, app.into_make_service())
                .with_graceful_shutdown(async move { shutdown.cancelled().await })
                .await;
            if let Err(e) = result {
                tracing::error!(error = %e, %addr, "HTTP redirect listener failed");
            }
        }
        .instrument(span),
    )
}

/// A router whose every path 308-redirects to `https://{host}{path}` (adding the
/// HTTPS port when it isn't the default 443). The canonical-FQDN rewrite for
/// short names is C8's job; this is a generic same-host upgrade.
fn redirect_router(https_port: u16) -> Router {
    Router::new().fallback(move |headers: HeaderMap, uri: Uri| async move {
        redirect_to_https(https_port, &headers, &uri)
    })
}

pub(crate) fn redirect_to_https(https_port: u16, headers: &HeaderMap, uri: &Uri) -> Response {
    let host = headers
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let host_no_port = host.split(':').next().unwrap_or("");
    if host_no_port.is_empty() {
        return (StatusCode::BAD_REQUEST, "missing Host header\n").into_response();
    }
    let authority = if https_port == 443 {
        host_no_port.to_owned()
    } else {
        format!("{host_no_port}:{https_port}")
    };
    let path = uri.path_and_query().map_or("/", |p| p.as_str());
    let location = format!("https://{authority}{path}");
    (
        StatusCode::PERMANENT_REDIRECT,
        [(header::LOCATION, location)],
    )
        .into_response()
}
