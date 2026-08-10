use std::sync::Arc;
use std::time::Duration;

use tokio::time::interval;
use tokio_util::sync::CancellationToken;
use tracing::Instrument;
use wardnet_common::auth::AuthContext;

use wardnetd_services::auth_context;
use wardnetd_services::system::SystemService;

/// Cadence between heartbeat writes. Sets the worst-case error of
/// the `last_seen_at` timestamp surfaced for an unclean shutdown.
pub const HEARTBEAT_INTERVAL: Duration = Duration::from_mins(1);

/// Background task that refreshes `last_seen_at` in `system_config`.
///
/// On boot, [`wardnetd_data::repository::SystemConfigRepository::detect_previous_shutdown`]
/// classifies the prior run by inspecting `last_shutdown_state` and
/// `last_seen_at`. The heartbeat runner is what keeps `last_seen_at`
/// current while the daemon is alive — without it, an unclean
/// shutdown's reported timestamp would only ever be the boot time,
/// not the moment the daemon was actually killed.
///
/// Failures are logged and never escalate: a transient `SQLite` write
/// error must not bring the daemon down. The runner retries on the
/// next tick.
pub struct HeartbeatRunner {
    cancel: CancellationToken,
    handle: tokio::task::JoinHandle<()>,
}

impl HeartbeatRunner {
    /// Start the runner. Logs land under a child of `parent` named
    /// `heartbeat`, matching the span hierarchy described in
    /// `.agents/observability.md`.
    pub fn start(system: Arc<dyn SystemService>, parent: &tracing::Span) -> Self {
        Self::start_with_interval(system, HEARTBEAT_INTERVAL, parent)
    }

    /// Start with a custom interval — used by tests to drive the loop
    /// faster than once a minute.
    pub fn start_with_interval(
        system: Arc<dyn SystemService>,
        tick: Duration,
        parent: &tracing::Span,
    ) -> Self {
        let cancel = CancellationToken::new();
        let span = tracing::info_span!(parent: parent, "heartbeat");

        let handle = tokio::spawn(heartbeat_loop(system, tick, cancel.clone()).instrument(span));

        Self { cancel, handle }
    }

    /// Cancel the loop and wait for the task to exit.
    pub async fn shutdown(self) {
        self.cancel.cancel();
        let _ = self.handle.await;
        tracing::info!("heartbeat runner shut down");
    }
}

async fn heartbeat_loop(system: Arc<dyn SystemService>, tick: Duration, cancel: CancellationToken) {
    let admin_ctx = AuthContext::system();

    // Fire one heartbeat immediately on startup so the initial
    // `last_seen_at` is no older than the boot moment.
    if let Err(e) = auth_context::with_context(admin_ctx.clone(), system.record_heartbeat()).await {
        tracing::warn!(error = %e, "initial heartbeat write failed: {e}");
    }

    let mut ticker = interval(tick);
    // Skip the immediate-fire built into `interval` since we already
    // wrote one above.
    ticker.tick().await;

    loop {
        tokio::select! {
            () = cancel.cancelled() => break,
            _ = ticker.tick() => {}
        }

        if let Err(e) =
            auth_context::with_context(admin_ctx.clone(), system.record_heartbeat()).await
        {
            tracing::warn!(error = %e, "heartbeat write failed: {e}");
        }
    }
}
