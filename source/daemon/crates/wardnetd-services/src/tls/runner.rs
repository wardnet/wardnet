//! Background runner that keeps the public TLS certificate current.
//!
//! Ticks every 12 hours (and once immediately at startup), calling
//! [`TlsService::ensure_certificate`] under an admin auth context. It holds only
//! `Arc<dyn TlsService>` — never a repository, provider, or ACME client — per
//! the runner contract in `.agents/architecture.md`. When DDNS is unconfigured
//! the service returns [`TlsStatus::NotConfigured`] before any ACME call, so the
//! runner is fully inert until a provider is registered.
//!
//! ## Retry
//!
//! A *failed* attempt does not wait for the next 12h tick: it retries on a short
//! exponential backoff ([`RETRY_MIN`] → [`RETRY_MAX`]), resetting on success.
//! Issuance failures are usually transient — most sharply, a freshly registered
//! network whose DNS record the cloud has not published yet will reject the ACME
//! challenge for a few seconds. Under a bare 12h cadence, losing that race cost
//! half a day without a certificate.
//!
//! ## Suspended state
//!
//! While the box is [suspended](Entitlement::is_suspended) the runner is fully
//! inert: there is no point renewing a public certificate for a box whose premium
//! surfaces are disabled, and the ACME DNS-01 challenge publishes through the
//! (also-suspended) wardnet provider. Renewal resumes automatically once the DDNS
//! runner's entitlement re-probe restores the box.

use std::sync::Arc;
use std::time::Duration;

use tokio_util::sync::CancellationToken;
use tracing::Instrument;
use uuid::Uuid;
use wardnet_common::auth::AuthContext;

use crate::auth_context;
use crate::entitlement::Entitlement;

use super::{TlsService, TlsStatus};

/// How often the runner re-checks the certificate. Issuance only happens when a
/// cert is missing or within the renewal window, so a 12h cadence is cheap and
/// leaves ample slack ahead of the 30-day renewal threshold.
const RENEWAL_INTERVAL: Duration = Duration::from_hours(12);

/// First retry delay after a failed issuance.
const RETRY_MIN: Duration = Duration::from_secs(30);

/// Ceiling for the retry backoff. Past this we are no longer chasing a transient
/// condition, and the 12h cadence takes back over on the next success.
const RETRY_MAX: Duration = Duration::from_mins(15);

/// Backoff after the CA rate-limits us (`urn:ietf:params:acme:error:rateLimited`,
/// e.g. "too many failed authorizations"). Let's Encrypt's failed-validation
/// limit is 5 per account/hostname/hour — retrying every [`RETRY_MAX`] keeps
/// tripping it and each rejected attempt is pure waste. One hour clears the
/// window.
const RATE_LIMIT_BACKOFF: Duration = Duration::from_hours(1);

/// Wakes the renewal runner into its retry backoff from OUTSIDE the runner —
/// the register-time provisioning task attempts issuance itself, and before
/// this handle existed its failure was invisible to the runner: the next
/// attempt was a full [`RENEWAL_INTERVAL`] (12h) away.
#[derive(Clone, Default)]
pub struct TlsRetryNudge(Arc<tokio::sync::Notify>);

impl TlsRetryNudge {
    /// Ask the runner to re-attempt issuance after [`RETRY_MIN`] (not
    /// immediately — an instant retry would lose the same race the caller
    /// just lost, and burn CA rate limit doing it).
    pub fn nudge(&self) {
        self.0.notify_one();
    }
}

/// Classification of a failed `ensure_certificate` for backoff purposes.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum FailureClass {
    /// Ordinary transient failure — exponential backoff.
    Transient,
    /// The CA told us to go away (`rateLimited`) — long fixed backoff.
    RateLimited,
}

/// Classify an issuance error by its message. The ACME problem type rides in
/// the error string (`urn:ietf:params:acme:error:rateLimited`, and Let's
/// Encrypt's human text says "too many ..."), which is all the surface the
/// `AppError` boundary preserves.
pub(super) fn classify_failure(message: &str) -> FailureClass {
    let lower = message.to_lowercase();
    if lower.contains("ratelimited") || lower.contains("too many") {
        FailureClass::RateLimited
    } else {
        FailureClass::Transient
    }
}

/// Delay before re-attempting issuance after `previous` failed.
///
/// Issuance failures are dominated by *transient* conditions: the cloud is
/// mid-deploy, the WAN is still coming up, or — the case that prompted this —
/// the network was registered seconds ago and its DNS record has not been
/// published yet, so the ACME challenge is rejected until it is. Waiting the
/// full [`RENEWAL_INTERVAL`] to find out costs half a day of no certificate for
/// a condition that typically clears in seconds.
pub(super) fn next_retry(previous: Option<Duration>) -> Duration {
    match previous {
        None => RETRY_MIN,
        Some(prev) => prev.saturating_mul(2).min(RETRY_MAX),
    }
}

/// Background TLS renewal runner. See [module docs](self).
pub struct TlsRenewalRunner {
    cancel: CancellationToken,
    handle: tokio::task::JoinHandle<()>,
}

impl TlsRenewalRunner {
    /// Start the runner under a child span of `parent`.
    pub fn start(
        tls: Arc<dyn TlsService>,
        entitlement: Arc<Entitlement>,
        nudge: TlsRetryNudge,
        parent: &tracing::Span,
    ) -> Self {
        let cancel = CancellationToken::new();
        let span = tracing::info_span!(parent: parent, "tls_renewal_runner");
        let handle =
            tokio::spawn(runner_loop(tls, entitlement, nudge, cancel.clone()).instrument(span));
        Self { cancel, handle }
    }

    /// Cancel the runner and wait for the task to finish.
    pub async fn shutdown(self) {
        self.cancel.cancel();
        let _ = self.handle.await;
        tracing::info!("TLS renewal runner shut down");
    }
}

async fn runner_loop(
    tls: Arc<dyn TlsService>,
    entitlement: Arc<Entitlement>,
    nudge: TlsRetryNudge,
    cancel: CancellationToken,
) {
    let admin_ctx = AuthContext::Admin {
        admin_id: Uuid::nil(),
    };

    // `None` while healthy: the next attempt is a full `RENEWAL_INTERVAL` away.
    // `Some(d)` after a failure: retry in `d`, doubling up to `RETRY_MAX`, so a
    // transient failure costs seconds rather than the full 12h cadence.
    let mut retry_in: Option<Duration> = None;
    // First pass runs immediately → check once at startup.
    let mut delay = Duration::ZERO;

    loop {
        tokio::select! {
            () = cancel.cancelled() => {
                tracing::info!("TLS renewal runner cancellation received");
                break;
            }
            // An out-of-band issuance failed (the register-time provisioning
            // task): schedule a retry at the backoff floor instead of sleeping
            // out the remainder of a 12h tick. Deliberately NOT an immediate
            // attempt — the caller's failure is seconds old, and an instant
            // replay would lose the same race while spending CA rate limit.
            () = nudge.0.notified() => {
                let next = next_retry(retry_in);
                retry_in = Some(next);
                tracing::info!(
                    retry_in_secs = next.as_secs(),
                    "TLS renewal: nudged after an out-of-band failure, retrying in {}s",
                    next.as_secs(),
                );
                delay = next;
            }
            () = tokio::time::sleep(delay) => {
                // Fully inert while suspended — renewing a cert for a box with
                // its premium surfaces disabled is pointless, and the ACME
                // challenge would publish through the suspended wardnet provider.
                // The DDNS runner's re-probe is what restores entitlement.
                if entitlement.is_suspended() {
                    tracing::debug!("TLS renewal: suspended, skipping");
                    // Suspension is not a failure: don't build up a retry backoff
                    // for it. The re-probe restores us on the normal cadence.
                    delay = RENEWAL_INTERVAL;
                    continue;
                }

                match ensure(tls.as_ref(), &admin_ctx).await {
                    None => {
                        retry_in = None;
                        delay = RENEWAL_INTERVAL;
                    }
                    Some(FailureClass::RateLimited) => {
                        // The CA is refusing work from us; hammering it on the
                        // exponential ladder is what GOT us rate-limited. Sit
                        // out the window.
                        retry_in = Some(RATE_LIMIT_BACKOFF);
                        tracing::warn!(
                            retry_in_secs = RATE_LIMIT_BACKOFF.as_secs(),
                            "TLS renewal: CA rate limit hit, backing off {}s",
                            RATE_LIMIT_BACKOFF.as_secs(),
                        );
                        delay = RATE_LIMIT_BACKOFF;
                    }
                    Some(FailureClass::Transient) => {
                        let next = next_retry(retry_in);
                        retry_in = Some(next);
                        tracing::info!(
                            retry_in_secs = next.as_secs(),
                            "TLS renewal: retrying in {}s rather than waiting for the next cycle",
                            next.as_secs(),
                        );
                        delay = next;
                    }
                }
            }
        }
    }
}

/// Run one `ensure_certificate` under the admin context, logging the outcome.
/// Never fatal — a failed ACME call must not take the daemon down.
///
/// Returns `false` when the attempt failed and is worth retrying sooner than the
/// next renewal cycle.
async fn ensure(tls: &dyn TlsService, admin_ctx: &AuthContext) -> Option<FailureClass> {
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
        Err(error) => {
            tracing::warn!(%error, "TLS renewal failed: {error}");
            return Some(classify_failure(&error.to_string()));
        }
    }
    None
}
