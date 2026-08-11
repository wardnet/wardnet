//! Background update runner — periodic check, auto-install when enabled.

use std::sync::Arc;
use std::time::Duration;

use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
use tracing::Instrument;
use wardnet_common::auth::AuthContext;

use crate::auth_context;
use crate::update::service::UpdateService;

/// Background runner that polls for new releases.
///
/// Checks immediately on startup, then every `check_interval` with a ±10%
/// jitter. When auto-update is enabled and a newer release is available, the
/// runner kicks off an install through the service (which runs the download/
/// verify/swap pipeline and schedules a systemd restart).
pub struct UpdateRunner {
    cancel: CancellationToken,
    handle: tokio::task::JoinHandle<()>,
}

impl UpdateRunner {
    /// Start the runner. `parent` is the `wardnetd{version=...}` span.
    pub fn start(
        service: Arc<dyn UpdateService>,
        check_interval: Duration,
        parent: &tracing::Span,
    ) -> Self {
        let cancel = CancellationToken::new();
        let span = tracing::info_span!(parent: parent, "update_runner");
        let handle =
            tokio::spawn(runner_loop(service, check_interval, cancel.clone()).instrument(span));
        Self { cancel, handle }
    }

    /// Signal the runner to stop and await completion.
    pub async fn shutdown(self) {
        self.cancel.cancel();
        let _ = self.handle.await;
        tracing::info!("update runner shut down");
    }
}

async fn runner_loop(
    service: Arc<dyn UpdateService>,
    check_interval: Duration,
    cancel: CancellationToken,
) {
    let admin_ctx = AuthContext::system();

    // Initial check runs immediately.
    perform_tick(&service, &admin_ctx).await;

    let mut next = Instant::now() + jittered(check_interval);
    loop {
        tokio::select! {
            () = cancel.cancelled() => {
                tracing::info!("update runner cancellation received");
                break;
            }
            () = tokio::time::sleep_until(next) => {
                perform_tick(&service, &admin_ctx).await;
                next = Instant::now() + jittered(check_interval);
            }
        }
    }
}

async fn perform_tick(service: &Arc<dyn UpdateService>, admin_ctx: &AuthContext) {
    if let Err(e) = auth_context::with_context(admin_ctx.clone(), service.check()).await {
        tracing::warn!(error = %e, "update check failed: {e}");
        return;
    }
    match auth_context::with_context(admin_ctx.clone(), service.auto_install_if_due()).await {
        Ok(Some(handle)) => {
            tracing::info!(
                target = %handle.target_version,
                install_id = %handle.install_id,
                "auto-update install started: target={target}, install_id={install_id}",
                target = handle.target_version,
                install_id = handle.install_id,
            );
        }
        Ok(None) => {}
        Err(e) => {
            tracing::warn!(error = %e, "auto-update install attempt failed: {e}");
        }
    }
}

/// Apply a ±10% uniform jitter to the interval.
fn jittered(base: Duration) -> Duration {
    use rand::RngExt;
    let base_ms = u64::try_from(base.as_millis()).unwrap_or(u64::MAX);
    if base_ms == 0 {
        return base;
    }
    let spread = base_ms / 10;
    let min = base_ms.saturating_sub(spread);
    let max = base_ms.saturating_add(spread);
    let ms: u64 = rand::rng().random_range(min..=max);
    Duration::from_millis(ms)
}
