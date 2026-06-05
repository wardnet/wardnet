//! Background runner that keeps the public TLS certificate current.
//!
//! Ticks every 12 hours (and once immediately at startup), calling
//! [`TlsService::ensure_certificate`] under an admin auth context. It holds only
//! `Arc<dyn TlsService>` — never a repository, provider, or ACME client — per
//! the runner contract in `.agents/architecture.md`. When DDNS is unconfigured
//! the service returns [`TlsStatus::NotConfigured`] before any ACME call, so the
//! runner is fully inert until a provider is registered.

use std::sync::Arc;
use std::time::Duration;

use tokio_util::sync::CancellationToken;
use tracing::Instrument;
use uuid::Uuid;
use wardnet_common::auth::AuthContext;

use crate::auth_context;

use super::{TlsService, TlsStatus};

/// How often the runner re-checks the certificate. Issuance only happens when a
/// cert is missing or within the renewal window, so a 12h cadence is cheap and
/// leaves ample slack ahead of the 30-day renewal threshold.
const RENEWAL_INTERVAL: Duration = Duration::from_hours(12);

/// Background TLS renewal runner. See [module docs](self).
pub struct TlsRenewalRunner {
    cancel: CancellationToken,
    handle: tokio::task::JoinHandle<()>,
}

impl TlsRenewalRunner {
    /// Start the runner under a child span of `parent`.
    pub fn start(tls: Arc<dyn TlsService>, parent: &tracing::Span) -> Self {
        let cancel = CancellationToken::new();
        let span = tracing::info_span!(parent: parent, "tls_renewal_runner");
        let handle = tokio::spawn(runner_loop(tls, cancel.clone()).instrument(span));
        Self { cancel, handle }
    }

    /// Cancel the runner and wait for the task to finish.
    pub async fn shutdown(self) {
        self.cancel.cancel();
        let _ = self.handle.await;
        tracing::info!("TLS renewal runner shut down");
    }
}

async fn runner_loop(tls: Arc<dyn TlsService>, cancel: CancellationToken) {
    let admin_ctx = AuthContext::Admin {
        admin_id: Uuid::nil(),
    };

    let mut interval = tokio::time::interval(RENEWAL_INTERVAL);
    // First `tick()` resolves immediately → check once at startup. `Delay`
    // spaces the next tick a full interval after the work finishes rather than
    // bursting to catch up.
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            () = cancel.cancelled() => {
                tracing::info!("TLS renewal runner cancellation received");
                break;
            }
            _ = interval.tick() => {
                ensure(tls.as_ref(), &admin_ctx).await;
            }
        }
    }
}

/// Run one `ensure_certificate` under the admin context, logging the outcome.
/// Never fatal — a failed ACME call must not take the daemon down.
async fn ensure(tls: &dyn TlsService, admin_ctx: &AuthContext) {
    match auth_context::with_context(admin_ctx.clone(), tls.ensure_certificate()).await {
        Ok(TlsStatus::NotConfigured) => {
            tracing::debug!("TLS renewal: DDNS unconfigured, nothing to do");
        }
        Ok(TlsStatus::Pending { domain }) => {
            tracing::debug!(%domain, "TLS renewal: certificate pending for {domain}");
        }
        // `ensure_certificate` acts on a renewal-due cert rather than returning
        // `NeedsRenewal` (that's a `status()`-only signal), but match exhaustively.
        Ok(TlsStatus::NeedsRenewal { domain, not_after }) => {
            tracing::debug!(
                %domain,
                %not_after,
                "TLS renewal: certificate for {domain} due for renewal (not_after={not_after})"
            );
        }
        Ok(TlsStatus::UpToDate { domain, not_after }) => {
            tracing::debug!(
                %domain,
                %not_after,
                "TLS renewal: certificate for {domain} up to date (not_after={not_after})"
            );
        }
        Ok(TlsStatus::Issued { domain, not_after }) => {
            tracing::info!(
                %domain,
                %not_after,
                "TLS certificate issued/renewed for {domain} (not_after={not_after})"
            );
        }
        Err(error) => tracing::warn!(%error, "TLS renewal failed: {error}"),
    }
}
