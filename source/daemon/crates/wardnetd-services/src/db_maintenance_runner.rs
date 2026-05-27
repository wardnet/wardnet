//! Daily database-maintenance runner.
//!
//! Calls [`MaintenanceRepository::incremental_vacuum`] once per calendar day
//! to return freed `SQLite` pages to the filesystem. Fires independently of
//! any domain-level feature flag so it benefits **all** retention-driven
//! tables (DNS query log, future event logs, audit trails), not just the ones
//! whose per-feature runner happens to be active.
//!
//! The daily cadence is enforced by checking the calendar date on every hourly
//! tick rather than sleeping for 24 hours, so the runner stays responsive to
//! cancellation without a 24-hour drain delay.

use std::sync::Arc;
use std::time::Duration;

use tokio_util::sync::CancellationToken;
use tracing::Instrument;

use wardnetd_data::repository::MaintenanceRepository;

/// How often the runner wakes to check whether a day has rolled over.
pub const TICK_INTERVAL: Duration = Duration::from_hours(1);

/// Background task that runs [`MaintenanceRepository::incremental_vacuum`]
/// once per calendar day.
pub struct DbMaintenanceRunner {
    cancel: CancellationToken,
    handle: tokio::task::JoinHandle<()>,
}

impl DbMaintenanceRunner {
    /// Start with the default hourly tick. Production entry point.
    pub fn start(maintenance_repo: Arc<dyn MaintenanceRepository>, parent: &tracing::Span) -> Self {
        Self::start_with_interval(maintenance_repo, TICK_INTERVAL, parent)
    }

    /// Start with a custom tick interval. Tests pass short intervals so they
    /// can exercise the day-rollover logic without sleeping.
    pub fn start_with_interval(
        maintenance_repo: Arc<dyn MaintenanceRepository>,
        tick_interval: Duration,
        parent: &tracing::Span,
    ) -> Self {
        let cancel = CancellationToken::new();
        let span = tracing::info_span!(parent: parent, "db_maintenance_runner");

        let handle = tokio::spawn(
            runner_loop(maintenance_repo, tick_interval, cancel.clone()).instrument(span),
        );

        Self { cancel, handle }
    }

    /// Cancel the runner and wait for it to stop.
    pub async fn shutdown(self) {
        self.cancel.cancel();
        let _ = self.handle.await;
        tracing::info!("DB maintenance runner shut down");
    }
}

async fn runner_loop(
    maintenance_repo: Arc<dyn MaintenanceRepository>,
    tick_interval: Duration,
    cancel: CancellationToken,
) {
    let mut ticker = tokio::time::interval(tick_interval);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    ticker.tick().await; // skip the immediate first tick

    let mut last_vacuum_day = today();

    loop {
        tokio::select! {
            () = cancel.cancelled() => {
                tracing::info!("DB maintenance runner cancellation received");
                break;
            }
            _ = ticker.tick() => {
                let today_now = today();
                if today_now != last_vacuum_day {
                    last_vacuum_day = today_now;
                    run_vacuum(maintenance_repo.as_ref()).await;
                }
            }
        }
    }
}

async fn run_vacuum(maintenance_repo: &dyn MaintenanceRepository) {
    match maintenance_repo.incremental_vacuum().await {
        Ok(reclaimed) if reclaimed > 0 => {
            tracing::info!(
                reclaimed_pages = reclaimed,
                "incremental vacuum reclaimed pages: reclaimed_pages={reclaimed}",
                reclaimed = reclaimed,
            );
        }
        Ok(_) => {}
        Err(e) => {
            tracing::warn!(error = %e, "incremental vacuum failed: {e}");
        }
    }
}

fn today() -> chrono::NaiveDate {
    chrono::Utc::now().date_naive()
}
