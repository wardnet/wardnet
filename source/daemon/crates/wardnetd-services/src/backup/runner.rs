//! Background runner that garbage-collects old `.bak-*` snapshots.
//!
//! Same shape as [`crate::update::runner::UpdateRunner`] — holds a
//! cancellation token, owns a `JoinHandle`, exposes `start()` + `shutdown()`.
//! Fires immediately on launch, then every `interval`.

use std::sync::Arc;
use std::time::Duration;

use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
use tracing::Instrument;
use uuid::Uuid;
use wardnet_common::auth::AuthContext;

use crate::auth_context;
use crate::backup::service::BackupService;

/// How long a `.bak-*` snapshot is retained before the cleanup runner
/// deletes it. Long enough for an operator to realise a restore went
/// wrong and manually recover from the `.bak-<ts>` siblings; short
/// enough that we don't pile up snapshots indefinitely on an SD card.
pub const DEFAULT_SNAPSHOT_RETENTION: Duration = Duration::from_hours(24);

/// Background runner that periodically trims stale `.bak-*` snapshots
/// out of the database directory.
///
/// Runs as a child of the root `wardnetd{version=...}` span so every
/// log line it emits carries the daemon version — same observability
/// rule every other background component follows.
pub struct BackupCleanupRunner {
    cancel: CancellationToken,
    handle: tokio::task::JoinHandle<()>,
}

impl BackupCleanupRunner {
    /// Start the cleanup loop. `parent` is the `wardnetd{version=...}`
    /// root span captured in `main.rs`.
    pub fn start(
        service: Arc<dyn BackupService>,
        interval: Duration,
        retain: Duration,
        parent: &tracing::Span,
    ) -> Self {
        let cancel = CancellationToken::new();
        let span = tracing::info_span!(parent: parent, "backup_cleanup_runner");
        let handle =
            tokio::spawn(runner_loop(service, interval, retain, cancel.clone()).instrument(span));
        Self { cancel, handle }
    }

    /// Signal the runner to stop and await completion.
    pub async fn shutdown(self) {
        self.cancel.cancel();
        let _ = self.handle.await;
        tracing::info!("backup cleanup runner shut down");
    }
}

async fn runner_loop(
    service: Arc<dyn BackupService>,
    interval: Duration,
    retain: Duration,
    cancel: CancellationToken,
) {
    let admin_ctx = AuthContext::Admin {
        admin_id: Uuid::nil(),
    };

    perform_tick(&service, retain, &admin_ctx).await;

    let mut next = Instant::now() + interval;
    loop {
        tokio::select! {
            () = cancel.cancelled() => {
                tracing::info!("backup cleanup runner cancellation received");
                break;
            }
            () = tokio::time::sleep_until(next) => {
                perform_tick(&service, retain, &admin_ctx).await;
                next = Instant::now() + interval;
            }
        }
    }
}

async fn perform_tick(service: &Arc<dyn BackupService>, retain: Duration, admin_ctx: &AuthContext) {
    match auth_context::with_context(admin_ctx.clone(), service.cleanup_old_snapshots(retain)).await
    {
        Ok(deleted) => {
            if deleted > 0 {
                tracing::debug!(
                    deleted,
                    "backup cleanup tick completed: deleted={deleted}",
                    deleted = deleted,
                );
            }
        }
        Err(e) => {
            tracing::warn!(error = %e, "backup cleanup tick failed: {e}");
        }
    }
}
