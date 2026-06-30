//! Daemon-owned TLS serving primitives for `main.rs`.
//!
//! `wardnetd` terminates TLS itself (no Caddy). The `:443` listener is **always
//! bound** — at boot with a throwaway placeholder self-signed cert — and a 503
//! guard short-circuits every `:443` route until a real cert is loaded. The
//! pre-provisioning admin surface is plain HTTP on `:7411` (unguarded). `:80`
//! 308-redirects to HTTPS.
//!
//! ## Serving identity
//!
//! The mutable serving state — *which domain's cert is currently live on `:443`*
//! — is encapsulated by [`ServingControl`], which exposes it through the
//! [`ServingIdentity`] read-trait (`is_provisioned` / `canonical_fqdn`) rather
//! than as a raw shared flag. The unauthenticated `:443` guard and `:80` redirect
//! depend on `Arc<dyn ServingIdentity>` and **call methods** — they never read
//! shared memory directly nor elevate to an admin context to call a service. The
//! authoritative copy of the served domain still lives in `system_config`
//! (`tls_cert_domain`, owned by `TlsService`); `ServingControl` is the hot-path
//! projection of it.
//!
//! `ServingControl` also implements the [`CertActivator`] write-seam the TLS
//! service injects: [`ServingControl::activate`] hot-swaps the live cert via
//! `RustlsConfig::reload_from_pem` and records the served domain (a `Some` domain
//! ⟺ provisioned, so the 503 gate and the redirect target move together).

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use arc_swap::ArcSwapOption;
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

/// Read-only view of the live serving identity, consulted by the unauthenticated
/// `:443` guard and `:80` redirect. Method-based so listeners never touch the
/// underlying shared cell directly; mockable in tests.
pub trait ServingIdentity: Send + Sync {
    /// Whether a real (non-placeholder) certificate is live on `:443`.
    fn is_provisioned(&self) -> bool;
    /// The domain whose cert is currently served — the canonical short-name
    /// redirect target. `None` while only the placeholder cert is loaded. Returns
    /// a shared `Arc` (cheap atomic refcount, no heap copy) so the per-request
    /// `:80` path doesn't allocate when no rewrite is needed.
    fn canonical_fqdn(&self) -> Option<Arc<String>>;
}

/// Owns the mutable `:443` serving identity: the live [`RustlsConfig`] plus the
/// domain whose cert it carries. A `Some` domain ⟺ provisioned, so the 503 gate
/// and the redirect target are flipped together by a single [`Self::activate`].
///
/// Injected into the TLS service as `Arc<dyn CertActivator>` (write seam) and
/// into the listeners as `Arc<dyn ServingIdentity>` (read seam) — the one object
/// is the single owner of this state.
pub struct ServingControl {
    config: RustlsConfig,
    /// The currently-served domain; `None` ⟺ placeholder cert ⟺ unprovisioned.
    served_domain: ArcSwapOption<String>,
}

impl ServingControl {
    #[must_use]
    pub fn new(config: RustlsConfig, served_domain: Option<String>) -> Self {
        Self {
            config,
            served_domain: ArcSwapOption::from(served_domain.map(Arc::new)),
        }
    }
}

impl ServingIdentity for ServingControl {
    fn is_provisioned(&self) -> bool {
        self.served_domain.load().is_some()
    }

    fn canonical_fqdn(&self) -> Option<Arc<String>> {
        // `load_full` clones only the `Arc` (atomic increment), not the `String`.
        self.served_domain.load_full()
    }
}

#[async_trait]
impl CertActivator for ServingControl {
    async fn activate(
        &self,
        chain_pem: Vec<u8>,
        key_pem: Vec<u8>,
        fqdn: String,
    ) -> anyhow::Result<()> {
        // Reload the cert *before* publishing the domain: a reader that observes
        // `Some(domain)` is then guaranteed to also see the matching cert.
        self.config.reload_from_pem(chain_pem, key_pem).await?;
        self.served_domain.store(Some(Arc::new(fqdn.clone())));
        tracing::info!(
            %fqdn,
            "activated TLS certificate on :443 for {fqdn}; provisioning gate lifted"
        );
        Ok(())
    }

    async fn deactivate(&self) -> anyhow::Result<()> {
        // Close the gate *before* swapping the cert: a reader that still observes
        // `Some(domain)` must always see a real cert, so clear the domain first
        // (503 guard re-engages) and only then drop the placeholder in. The
        // mirror of `activate`'s reload-then-publish ordering.
        self.served_domain.store(None);
        let (cert, key) = generate_placeholder_pem()?;
        self.config.reload_from_pem(cert, key).await?;
        tracing::info!(
            "deactivated TLS on :443; reverted to placeholder cert, provisioning gate closed"
        );
        Ok(())
    }
}

/// Build the `:443` [`RustlsConfig`] and its [`ServingControl`].
///
/// Seeds from `seed` (the stored real cert) when present — provisioned, with
/// `served_domain` set to `seed_domain` — otherwise from a freshly generated
/// placeholder cert (unprovisioned, `served_domain = None`). The `RustlsConfig`
/// is cloned into both the `:443` listener and the `ServingControl`, which share
/// its internal `ArcSwap` for lock-free reloads.
pub async fn build_serving_control(
    seed: Option<(Vec<u8>, Vec<u8>)>,
    seed_domain: Option<String>,
) -> anyhow::Result<(RustlsConfig, Arc<ServingControl>)> {
    let (served_domain, cert, key) = if let Some((cert, key)) = seed {
        (seed_domain, cert, key)
    } else {
        let (cert, key) = generate_placeholder_pem()?;
        (None, cert, key)
    };
    let config = RustlsConfig::from_pem(cert, key).await?;
    let control = Arc::new(ServingControl::new(config.clone(), served_domain));
    Ok((config, control))
}

/// Wrap `app` with the 503 guard: until a real cert is provisioned, every request
/// is short-circuited with a 503 pointing at the plain-HTTP fallback. Applied only
/// to the `:443` app — `:7411` is never guarded.
pub fn guarded_https_app(app: Router, serving: Arc<dyn ServingIdentity>) -> Router {
    app.layer(axum::middleware::from_fn(
        move |req: Request, next: Next| {
            let serving = serving.clone();
            async move {
                if serving.is_provisioned() {
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
    listener: std::net::TcpListener,
    app: Router,
    config: RustlsConfig,
    shutdown: &CancellationToken,
    parent: &tracing::Span,
) -> tokio::task::JoinHandle<()> {
    let addr = listener
        .local_addr()
        .unwrap_or_else(|_| SocketAddr::from(([0, 0, 0, 0], 0)));
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
            // Serve the already-bound listener (bound synchronously by the
            // caller before READY=1) rather than binding here, so systemd's
            // readiness signal can't precede the bind. `from_tcp_rustls`
            // adopts the std listener and can fail on the conversion.
            let server = match axum_server::from_tcp_rustls(listener, config) {
                Ok(server) => server,
                Err(e) => {
                    tracing::error!(error = %e, %addr, "failed to adopt :443 listener for TLS; HTTPS unavailable: {e}");
                    return;
                }
            };
            if let Err(e) = server
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

/// Spawn the `:80` listener that 308-redirects every request to HTTPS. When a
/// canonical FQDN is provisioned, short-name requests are rewritten to it;
/// otherwise the redirect is a same-host upgrade. A bind failure is logged, not
/// fatal.
pub fn spawn_http_redirect_listener(
    listener: tokio::net::TcpListener,
    https_port: u16,
    serving: Arc<dyn ServingIdentity>,
    shutdown: &CancellationToken,
    parent: &tracing::Span,
) -> tokio::task::JoinHandle<()> {
    let addr = listener
        .local_addr()
        .unwrap_or_else(|_| SocketAddr::from(([0, 0, 0, 0], 0)));
    let app = redirect_router(https_port, serving);
    let shutdown = shutdown.clone();
    let span = tracing::info_span!(parent: parent, "http_redirect_server");
    tokio::spawn(
        async move {
            // Listener is bound synchronously by the caller before READY=1.
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

/// A router whose every path 308-redirects to HTTPS. When the serving identity
/// has a canonical FQDN and the request arrived under a different host (a short
/// or LAN name like `wardnet`, `wardnet.lan`, or the bare LAN IP), the redirect
/// rewrites the host to the canonical FQDN so the client lands on the name with a
/// valid cert. Otherwise it is a same-host upgrade.
pub(crate) fn redirect_router(https_port: u16, serving: Arc<dyn ServingIdentity>) -> Router {
    Router::new()
        .fallback(move |headers: HeaderMap, uri: Uri| {
            let serving = serving.clone();
            async move { redirect_to_https(https_port, serving.canonical_fqdn(), &headers, &uri) }
        })
        // Panic isolation, same as the main API router: a panic in the redirect
        // path must surface as a logged 500, never unwind the `:80` listener.
        .layer(wardnetd_api::api::catch_panic_layer())
}

pub(crate) fn redirect_to_https(
    https_port: u16,
    canonical_fqdn: Option<Arc<String>>,
    headers: &HeaderMap,
    uri: &Uri,
) -> Response {
    let host = headers
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    // Strip the optional `:port`. IPv6 literals are bracketed (`[::1]` /
    // `[::1]:443`), so a naive `split(':')` would mangle them — keep the bracketed
    // address and drop only a trailing port. `wardnetd` runs on non-Pi hosts too,
    // so this isn't purely theoretical.
    let host_no_port = if host.starts_with('[') {
        host.find(']').map_or(host, |end| &host[..=end])
    } else {
        host.split(':').next().unwrap_or("")
    };
    if host_no_port.is_empty() {
        return (StatusCode::BAD_REQUEST, "missing Host header\n").into_response();
    }
    // Rewrite short/LAN names to the canonical FQDN (the name with a valid cert);
    // fall back to a same-host upgrade when no FQDN is provisioned or the request
    // already targets it. The same-host upgrade is genuinely permanent (308), but
    // a rewrite points at a *mutable* target (the canonical FQDN can change when
    // the DDNS provider/domain changes), so it must be a 307 the browser won't
    // cache permanently.
    let (target_host, status) = match canonical_fqdn {
        Some(fqdn) if fqdn.as_str() != host_no_port => {
            (fqdn.as_str().to_owned(), StatusCode::TEMPORARY_REDIRECT)
        }
        _ => (host_no_port.to_owned(), StatusCode::PERMANENT_REDIRECT),
    };
    let authority = if https_port == 443 {
        target_host
    } else {
        format!("{target_host}:{https_port}")
    };
    let path = uri.path_and_query().map_or("/", |p| p.as_str());
    let location = format!("https://{authority}{path}");
    (status, [(header::LOCATION, location)]).into_response()
}
