//! `DoT` listener lifecycle runner (issue #912).
//!
//! Starts and stops the `:853` [`DotServer`] so it runs exactly when
//! Private DNS is enabled **and** an issued certificate is live. The enable
//! side arrives as a [`WardnetEvent::PrivateDnsChanged`]; certificate
//! activation has no event (the renewal runner hot-swaps it through
//! `CertActivator`), so the runner also rechecks on a slow tick — the
//! daemon can boot with Private DNS enabled minutes before the first
//! issuance completes, and the listener must come up on its own once the
//! cert lands.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use tracing::Instrument;
use wardnet_common::auth::AuthContext;
use wardnet_common::event::WardnetEvent;

use crate::auth_context;
use crate::event::EventPublisher;
use crate::private_dns::{DotServer, PrivateDnsService, cert_ready};
use crate::tls::TlsService;

/// How often the runner re-derives the desired state outside of events —
/// the window in which a freshly-issued certificate brings the listener up.
const RECHECK_INTERVAL: Duration = Duration::from_secs(30);

/// Owns the spawned `DoT`-lifecycle loop and its cancellation handle. Drop
/// via [`Self::shutdown`] to stop the loop (and the listener) cleanly.
pub struct DotRunner {
    cancel: CancellationToken,
    handle: tokio::task::JoinHandle<()>,
}

impl DotRunner {
    pub fn start(
        private_dns: Arc<dyn PrivateDnsService>,
        tls: Arc<dyn TlsService>,
        server: Arc<dyn DotServer>,
        events: &dyn EventPublisher,
        parent: &tracing::Span,
    ) -> Self {
        let cancel = CancellationToken::new();
        let span = tracing::info_span!(parent: parent, "dot_runner");
        let event_rx = events.subscribe();

        let handle = tokio::spawn(
            runner_loop(private_dns, tls, server, event_rx, cancel.clone()).instrument(span),
        );

        Self { cancel, handle }
    }

    pub async fn shutdown(self) {
        self.cancel.cancel();
        let _ = self.handle.await;
        tracing::info!("DoT runner shut down");
    }
}

/// Whether the listener should currently be running: Private DNS enabled
/// and an issued certificate live. Both reads run under an admin
/// [`auth_context`], like every background runner. Read failures resolve to
/// `false` — better a listener that comes up one tick late than one serving
/// with unknown state.
///
/// Gates on [`PrivateDnsService::is_enabled`] (a single config read), not
/// the full `status()`, so a transient DDNS/config error that only affects
/// `status()`'s unused `domain` field can't stop a healthy enabled listener.
async fn desired_state(
    private_dns: &Arc<dyn PrivateDnsService>,
    tls: &Arc<dyn TlsService>,
    admin_ctx: &AuthContext,
) -> bool {
    let enabled =
        match auth_context::with_context(admin_ctx.clone(), private_dns.is_enabled()).await {
            Ok(enabled) => enabled,
            Err(e) => {
                tracing::error!(error = %e, "failed to read Private DNS status");
                return false;
            }
        };
    if !enabled {
        return false;
    }
    match auth_context::with_context(admin_ctx.clone(), tls.status()).await {
        Ok(status) => cert_ready(&status),
        Err(e) => {
            tracing::error!(error = %e, "failed to read TLS status");
            false
        }
    }
}

/// Idempotent transition to `should_run`, mirroring the DNS runner's
/// start/stop block.
async fn reconcile(server: &Arc<dyn DotServer>, should_run: bool) {
    if should_run && !server.is_running() {
        tracing::info!("Private DNS is enabled and the certificate is live, starting DoT server");
        if let Err(e) = server.start().await {
            tracing::error!(error = %e, "failed to start DoT server");
        }
    } else if !should_run
        && server.is_running()
        && let Err(e) = server.stop().await
    {
        tracing::error!(error = %e, "failed to stop DoT server");
    }
}

async fn runner_loop(
    private_dns: Arc<dyn PrivateDnsService>,
    tls: Arc<dyn TlsService>,
    server: Arc<dyn DotServer>,
    mut event_rx: broadcast::Receiver<WardnetEvent>,
    cancel: CancellationToken,
) {
    let admin_ctx = AuthContext::system();

    let mut recheck = tokio::time::interval(RECHECK_INTERVAL);
    recheck.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            () = cancel.cancelled() => {
                tracing::info!("DoT runner cancellation received");
                break;
            }
            // First tick fires immediately — this doubles as the startup
            // reconcile, so there is no separate pre-loop block to keep in
            // sync with the tick path.
            _ = recheck.tick() => {
                let should_run = desired_state(&private_dns, &tls, &admin_ctx).await;
                reconcile(&server, should_run).await;
            }
            result = event_rx.recv() => {
                match result {
                    // Enable/disable/grant changes and entitlement flips both
                    // re-derive the full desired state rather than trusting
                    // the event payload — the checks are cheap config reads.
                    Ok(WardnetEvent::PrivateDnsChanged { .. } | WardnetEvent::EntitlementChanged { .. }) => {
                        let should_run = desired_state(&private_dns, &tls, &admin_ctx).await;
                        reconcile(&server, should_run).await;
                    }
                    // Revocation: terminate the device's live sessions at once
                    // (the token is otherwise only checked at handshake). This
                    // does not change should-run, so no reconcile is needed.
                    Ok(WardnetEvent::PrivateDnsGrantRevoked { device_id, .. }) => {
                        server.disconnect_device(device_id);
                    }
                    Ok(_) => {}
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(skipped = n, "DoT runner lagged behind event bus");
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        tracing::info!("DoT runner: event bus closed");
                        break;
                    }
                }
            }
        }
    }

    if server.is_running()
        && let Err(e) = server.stop().await
    {
        tracing::error!(error = %e, "failed to stop DoT server during shutdown");
    }
}
