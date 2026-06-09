//! The `:8080` listener: the ACME **HTTP-01** challenge responder plus a liveness
//! `/health` endpoint. Nothing else — the control-plane API is served only over
//! the TLS-terminated `:8443` path, never in plaintext here.
//!
//! Challenge tokens live in the shared `acme_http_challenge` table, so this
//! responder answers Let's Encrypt's validation regardless of which host issued
//! the order. The lookup is **public and unauthenticated**, so the token is
//! shape-guarded (base64url, bounded length) before any DB query — it must never
//! become a database-amplification probe.

use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use tokio::net::TcpListener;

use crate::proxy_protocol::{self, Inspected};
use crate::repository::TlsRepository;
use crate::serve;

/// Maximum length of an ACME token we will look up (real tokens are ~43 chars).
const MAX_TOKEN_LEN: usize = 128;
/// Time allowed to read the PROXY header before giving up on a connection.
const PROXY_READ_TIMEOUT: Duration = Duration::from_secs(5);

/// Run the `:8080` HTTP-01 responder. Never returns under normal operation.
///
/// # Errors
/// Returns an error only if the listener cannot be bound.
pub async fn run(addr: &str, repo: Arc<dyn TlsRepository>) -> anyhow::Result<()> {
    let listener = TcpListener::bind(addr).await?;
    tracing::info!(addr, "HTTP-01 responder listening");
    let router = build_router(repo);

    loop {
        let (mut stream, peer) = listener.accept().await?;
        let router = router.clone();
        tokio::spawn(async move {
            // Tolerant PROXY parse: a health probe may connect directly (no header).
            let client_addr = match tokio::time::timeout(
                PROXY_READ_TIMEOUT,
                proxy_protocol::read_optional(&mut stream),
            )
            .await
            {
                Ok(Ok(Inspected::Header(Some(addr)))) => addr,
                Ok(Ok(Inspected::Header(None) | Inspected::Direct)) => peer,
                Ok(Err(e)) => {
                    tracing::debug!(error = %e, "malformed PROXY header on :8080");
                    return;
                }
                Err(_) => {
                    tracing::debug!("timed out reading PROXY header on :8080");
                    return;
                }
            };

            if let Err(e) = serve::connection(stream, router, client_addr).await {
                tracing::debug!(error = %e, "HTTP-01 connection error");
            }
        });
    }
}

fn build_router(repo: Arc<dyn TlsRepository>) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/.well-known/acme-challenge/{token}", get(challenge))
        .fallback(not_found)
        .with_state(repo)
}

/// Liveness probe — always `200 ok` once the process is serving.
async fn health() -> &'static str {
    "ok"
}

/// Serve the key authorization for a live HTTP-01 `token`, or `404`.
async fn challenge(
    State(repo): State<Arc<dyn TlsRepository>>,
    Path(token): Path<String>,
) -> Response {
    // Shape-guard before touching the DB: an unauthenticated public lookup must
    // not be turnable into a DB probe.
    if !is_valid_acme_token(&token) {
        return StatusCode::NOT_FOUND.into_response();
    }
    match repo.get_challenge(&token).await {
        Ok(Some(key_authorization)) => (StatusCode::OK, key_authorization).into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(e) => {
            tracing::warn!(error = %e, "challenge lookup failed");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

async fn not_found() -> StatusCode {
    StatusCode::NOT_FOUND
}

/// Validate that `token` looks like an ACME HTTP-01 token (base64url, bounded).
fn is_valid_acme_token(token: &str) -> bool {
    !token.is_empty()
        && token.len() <= MAX_TOKEN_LEN
        && token
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

#[cfg(test)]
mod tests;
