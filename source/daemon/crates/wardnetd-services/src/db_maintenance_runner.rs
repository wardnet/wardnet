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
    /// can exercise the day-rollover logic without sleeping for an hour.
    pub fn start_with_interval(
        maintenance_repo: Arc<dyn MaintenanceRepository>,
        tick_interval: Duration,
        parent: &tracing::Span,
    ) -> Self {
        Self::start_with_interval_and_day(maintenance_repo, tick_interval, today(), parent)
    }

    /// Internal constructor that accepts an explicit initial day so tests can
    /// set yesterday's date and have the first tick immediately fire a vacuum.
    fn start_with_interval_and_day(
        maintenance_repo: Arc<dyn MaintenanceRepository>,
        tick_interval: Duration,
        initial_day: chrono::NaiveDate,
        parent: &tracing::Span,
    ) -> Self {
        let cancel = CancellationToken::new();
        let span = tracing::info_span!(parent: parent, "db_maintenance_runner");

        let handle = tokio::spawn(
            runner_loop(maintenance_repo, tick_interval, cancel.clone(), initial_day)
                .instrument(span),
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
    initial_day: chrono::NaiveDate,
) {
    let mut ticker = tokio::time::interval(tick_interval);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    ticker.tick().await; // skip the immediate first tick

    let mut last_vacuum_day = initial_day;

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

pub(crate) async fn run_vacuum(maintenance_repo: &dyn MaintenanceRepository) {
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::Mutex;
    use std::time::Duration;

    use async_trait::async_trait;

    use wardnetd_data::repository::MaintenanceRepository;

    use super::{DbMaintenanceRunner, run_vacuum};

    // -----------------------------------------------------------------------
    // Mock maintenance repository
    // -----------------------------------------------------------------------

    struct MockMaintenance {
        result: anyhow::Result<u64>,
        calls: Mutex<u32>,
    }

    impl MockMaintenance {
        fn ok(reclaimed: u64) -> Arc<Self> {
            Arc::new(Self {
                result: Ok(reclaimed),
                calls: Mutex::new(0),
            })
        }

        fn err() -> Arc<Self> {
            Arc::new(Self {
                result: Err(anyhow::anyhow!("synthetic vacuum error")),
                calls: Mutex::new(0),
            })
        }

        fn call_count(&self) -> u32 {
            *self.calls.lock().unwrap()
        }
    }

    #[async_trait]
    impl MaintenanceRepository for MockMaintenance {
        async fn incremental_vacuum(&self) -> anyhow::Result<u64> {
            *self.calls.lock().unwrap() += 1;
            match &self.result {
                Ok(n) => Ok(*n),
                Err(e) => Err(anyhow::anyhow!("{e}")),
            }
        }
    }

    // -----------------------------------------------------------------------
    // run_vacuum unit tests — cover all three match arms
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn run_vacuum_logs_when_pages_reclaimed() {
        let repo = MockMaintenance::ok(42);
        run_vacuum(repo.as_ref()).await;
        assert_eq!(repo.call_count(), 1);
    }

    #[tokio::test]
    async fn run_vacuum_silent_when_nothing_reclaimed() {
        let repo = MockMaintenance::ok(0);
        run_vacuum(repo.as_ref()).await;
        assert_eq!(repo.call_count(), 1);
    }

    #[tokio::test]
    async fn run_vacuum_warns_and_does_not_panic_on_error() {
        let repo = MockMaintenance::err();
        run_vacuum(repo.as_ref()).await; // must not panic
        assert_eq!(repo.call_count(), 1);
    }

    // -----------------------------------------------------------------------
    // Runner integration tests
    // -----------------------------------------------------------------------

    /// Starting and immediately shutting down the runner must complete without
    /// panicking. Exercises `DbMaintenanceRunner::start` → `start_with_interval`
    /// → `start_with_interval_and_day` → `shutdown`, and the cancel branch of
    /// `runner_loop`.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn runner_shuts_down_cleanly() {
        let repo = MockMaintenance::ok(0);
        let runner = DbMaintenanceRunner::start(repo.clone(), &tracing::Span::none());
        runner.shutdown().await;
    }

    /// When the runner is started with yesterday's date as the initial day,
    /// the very first tick (after a short interval) crosses the day boundary
    /// and fires a vacuum.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn runner_fires_vacuum_on_day_rollover() {
        let repo = MockMaintenance::ok(5);
        let yesterday = chrono::Utc::now().date_naive() - chrono::Days::new(1);

        let runner = DbMaintenanceRunner::start_with_interval_and_day(
            repo.clone(),
            Duration::from_millis(20),
            yesterday,
            &tracing::Span::none(),
        );

        // Give the ticker enough time to fire at least once.
        tokio::time::sleep(Duration::from_millis(100)).await;
        runner.shutdown().await;

        assert!(
            repo.call_count() >= 1,
            "expected vacuum to fire on day rollover, call_count={}",
            repo.call_count()
        );
    }

    /// When the initial day is today, ticks must not fire a vacuum.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn runner_skips_vacuum_when_same_day() {
        let repo = MockMaintenance::ok(0);

        let runner = DbMaintenanceRunner::start_with_interval(
            repo.clone(),
            Duration::from_millis(20),
            &tracing::Span::none(),
        );

        tokio::time::sleep(Duration::from_millis(80)).await;
        runner.shutdown().await;

        assert_eq!(
            repo.call_count(),
            0,
            "vacuum must not fire when the day has not rolled over"
        );
    }
}
