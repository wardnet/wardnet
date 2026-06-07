//! Reservation sweep — reaps abandoned name reservations.
//!
//! Registration is a two-database saga: the `names` row lives in the global
//! naming authority, the install row in this bridge's regional DB. A crash
//! between *reserve* and *confirm* leaves a `reserved` names row (and possibly a
//! regional install orphan). This sweep deletes expired `reserved` rows for
//! **this region** and the matching regional install rows, so a crashed
//! registration never permanently leaks a name.

use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;

use crate::repository::{InstallRepository, NameRepository};

/// How often the sweep runs.
const SWEEP_INTERVAL: Duration = Duration::from_mins(1);

/// Run one sweep pass: delete expired reservations for `region` from the global
/// `names` table, then delete the freed reservations' regional install rows in a
/// single batched statement.
///
/// Returns the number of reservations reaped. Deletes are idempotent, so a
/// partially-applied previous pass (or a release that already ran) is harmless.
///
/// # Errors
/// Propagates a failure to query the global `names` table. A failure deleting the
/// orphan install rows is logged and swallowed (best-effort) — the names rows are
/// already gone, so the pass still counts them reaped.
pub async fn sweep_once(
    names: &dyn NameRepository,
    installs: &dyn InstallRepository,
    region: &str,
) -> anyhow::Result<usize> {
    let expired = names.sweep_expired(Utc::now(), region).await?;

    if !expired.is_empty() {
        if let Err(e) = installs.delete_many(&expired).await {
            tracing::error!(
                count = expired.len(),
                error = %e,
                "failed to delete orphan installs for swept reservations"
            );
        }
        tracing::info!(
            region = %region,
            count = expired.len(),
            "swept expired name reservations"
        );
    }

    Ok(expired.len())
}

/// Background loop driving [`sweep_once`] every [`SWEEP_INTERVAL`]. Never
/// returns; spawn it as a detached task.
///
/// Each pass runs in its own task so a *panic* inside `sweep_once` is isolated
/// (surfaced as a `JoinError`) and the loop keeps ticking — a background-task
/// panic must not stop the sweep nor take down live tunnels. A returned error is
/// logged and retried on the next tick.
pub async fn run(
    names: Arc<dyn NameRepository>,
    installs: Arc<dyn InstallRepository>,
    region: String,
) {
    let mut interval = tokio::time::interval(SWEEP_INTERVAL);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        interval.tick().await;

        let names = Arc::clone(&names);
        let installs = Arc::clone(&installs);
        let region_for_pass = region.clone();
        let pass = tokio::spawn(async move {
            sweep_once(names.as_ref(), installs.as_ref(), &region_for_pass).await
        });

        match pass.await {
            Ok(Ok(_)) => {}
            Ok(Err(e)) => {
                tracing::error!(region = %region, error = %e, "reservation sweep failed");
            }
            Err(join_err) => {
                tracing::error!(
                    region = %region,
                    error = %join_err,
                    "reservation sweep pass panicked — continuing"
                );
            }
        }
    }
}
