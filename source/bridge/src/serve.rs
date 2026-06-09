//! Serve an axum [`Router`] over a single already-accepted (and possibly
//! TLS-terminated) byte stream via hyper.
//!
//! The bridge cannot use `axum::serve` for its public listeners: it must first
//! strip the PROXY protocol header (and, on `:8443`, peek the SNI and terminate
//! TLS) before any HTTP is spoken. So each connection is driven through hyper
//! by hand. The real client address — recovered from the PROXY header — is
//! injected as [`ConnectInfo`] so handlers (and the per-IP rate limiter) see the
//! true client rather than the L4 proxy.

use std::net::SocketAddr;

use axum::Router;
use axum::extract::ConnectInfo;
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto::Builder;
use hyper_util::service::TowerToHyperService;
use tokio::io::{AsyncRead, AsyncWrite};

/// Serve `router` over `io` for one connection, attributing every request to
/// `client_addr` via [`ConnectInfo`].
///
/// # Errors
/// Returns an error if the HTTP connection terminates abnormally.
pub async fn connection<IO>(io: IO, router: Router, client_addr: SocketAddr) -> anyhow::Result<()>
where
    IO: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    // Inject the real client address so `ConnectInfo<SocketAddr>` resolves to it.
    let app = router.layer(axum::Extension(ConnectInfo(client_addr)));
    let service = TowerToHyperService::new(app);

    Builder::new(TokioExecutor::new())
        .serve_connection_with_upgrades(TokioIo::new(io), service)
        .await
        .map_err(|e| anyhow::anyhow!("connection error: {e}"))
}
