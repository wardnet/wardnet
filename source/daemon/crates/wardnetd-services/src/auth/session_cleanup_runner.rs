//! Background runner that periodically purges expired sessions **and expired
//! enrolment tokens**.
//!
//! Reads always filter on `expires_at > now`, so neither an expired session nor
//! a stale invitation is ever honoured — but the rows would otherwise
//! accumulate indefinitely. This runner calls the auth-gated
//! [`AuthService::cleanup_expired_sessions`] and
//! [`UserService::cleanup_expired_enrolments`] on a fixed interval (hourly in
//! production) to reclaim that dead storage.
//!
//! The two are swept together because they expire on the same kind of schedule
//! and neither is urgent; a second runner would double the timers for no gain.
//! One failing must not skip the other, so each is handled independently.
//!
//! Lifecycle mirrors the other daemon runners: spawned at boot, stopped via
//! the cancellation token on shutdown. Like [`DnsQueryLogRunner`], the
//! immediate first interval tick is consumed so the first cleanup fires one
//! interval after startup rather than at boot.
//!
//! [`DnsQueryLogRunner`]: crate::dns::query_log_runner::DnsQueryLogRunner

use std::sync::Arc;
use std::time::Duration;

use tokio_util::sync::CancellationToken;
use tracing::Instrument;
use wardnet_common::auth::AuthContext;

use crate::auth_context;
use crate::user::UserService;
use crate::AuthService;

/// Production cleanup interval.
const CLEANUP_INTERVAL: Duration = Duration::from_hours(1);

/// Background runner purging expired sessions via [`AuthService`].
pub struct SessionCleanupRunner {
    cancel: CancellationToken,
    handle: tokio::task::JoinHandle<()>,
}

impl SessionCleanupRunner {
    /// Start the runner with the production ([`CLEANUP_INTERVAL`]) cadence.
    ///
    /// The `parent` span is used as the parent for the
    /// `session_cleanup_runner` child span, ensuring all log output includes
    /// the root version field.
    pub fn start(
        service: Arc<dyn AuthService>,
        users: Arc<dyn UserService>,
        parent: &tracing::Span,
    ) -> Self {
        Self::start_with_interval(service, users, CLEANUP_INTERVAL, parent)
    }

    /// Start the runner with a custom interval. Production callers use
    /// [`Self::start`]; tests pass a shorter interval to exercise the loop
    /// without waiting.
    pub fn start_with_interval(
        service: Arc<dyn AuthService>,
        users: Arc<dyn UserService>,
        interval: Duration,
        parent: &tracing::Span,
    ) -> Self {
        let cancel = CancellationToken::new();
        let span = tracing::info_span!(parent: parent, "session_cleanup_runner");

        let handle =
            tokio::spawn(runner_loop(service, users, interval, cancel.clone()).instrument(span));

        Self { cancel, handle }
    }

    /// Cancel the background task and wait for it to finish.
    pub async fn shutdown(self) {
        self.cancel.cancel();
        let _ = self.handle.await;
        tracing::info!("session cleanup runner shut down");
    }
}

async fn runner_loop(
    service: Arc<dyn AuthService>,
    users: Arc<dyn UserService>,
    interval: Duration,
    cancel: CancellationToken,
) {
    let admin_ctx = AuthContext::system();

    let mut ticker = tokio::time::interval(interval);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    ticker.tick().await; // skip the immediate first tick

    loop {
        tokio::select! {
            () = cancel.cancelled() => {
                tracing::info!("session cleanup runner cancellation received");
                break;
            }
            _ = ticker.tick() => {
                match auth_context::with_context(
                    admin_ctx.clone(),
                    service.cleanup_expired_sessions(),
                )
                .await
                {
                    Ok(deleted) if deleted > 0 => {
                        tracing::info!(deleted, "purged expired sessions: deleted={deleted}");
                    }
                    Ok(_) => {
                        tracing::debug!("no expired sessions to purge");
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "failed to purge expired sessions: error={e}");
                    }
                }

                // Handled separately so a session-cleanup failure does not
                // leave stale invitations sitting there, and vice versa.
                match auth_context::with_context(
                    admin_ctx.clone(),
                    users.cleanup_expired_enrolments(),
                )
                .await
                {
                    Ok(deleted) if deleted > 0 => {
                        tracing::info!(
                            deleted,
                            "purged expired enrolment tokens: deleted={deleted}"
                        );
                    }
                    Ok(_) => {
                        tracing::debug!("no expired enrolment tokens to purge");
                    }
                    Err(e) => {
                        tracing::error!(
                            error = %e,
                            "failed to purge expired enrolment tokens: error={e}"
                        );
                    }
                }
            }
        }
    }
}
