//! Daily database-maintenance runner.
//!
//! Calls [`MaintenanceService::run_incremental_vacuum`] once per calendar day
//! to return freed `SQLite` pages to the filesystem. Fires independently of
//! any domain-level feature flag so it benefits **all** retention-driven
//! tables (DNS query log, future event logs, audit trails), not just the ones
//! whose per-feature runner happens to be active.
//!
//! Like every background component, it calls the auth-gated service under an
//! admin [`crate::auth_context`] rather than holding a repository directly.
//!
//! The daily cadence is enforced by checking the calendar date on every hourly
//! tick rather than sleeping for 24 hours, so the runner stays responsive to
//! cancellation without a 24-hour drain delay.

use std::sync::Arc;
use std::time::Duration;

use tokio_util::sync::CancellationToken;
use tracing::Instrument;
use uuid::Uuid;
use wardnet_common::auth::AuthContext;

use crate::auth_context;
use crate::maintenance::MaintenanceService;

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
    pub fn start(maintenance: Arc<dyn MaintenanceService>, parent: &tracing::Span) -> Self {
        Self::start_with_interval(maintenance, TICK_INTERVAL, parent)
    }

    /// Start with a custom tick interval. Tests pass short intervals so they
    /// can exercise the day-rollover logic without sleeping for an hour.
    pub fn start_with_interval(
        maintenance: Arc<dyn MaintenanceService>,
        tick_interval: Duration,
        parent: &tracing::Span,
    ) -> Self {
        Self::start_with_interval_and_day(maintenance, tick_interval, today(), parent)
    }

    /// Internal constructor that accepts an explicit initial day so tests can
    /// set yesterday's date and have the first tick immediately fire a vacuum.
    pub(crate) fn start_with_interval_and_day(
        maintenance: Arc<dyn MaintenanceService>,
        tick_interval: Duration,
        initial_day: chrono::NaiveDate,
        parent: &tracing::Span,
    ) -> Self {
        let cancel = CancellationToken::new();
        let span = tracing::info_span!(parent: parent, "db_maintenance_runner");

        let handle = tokio::spawn(
            runner_loop(maintenance, tick_interval, cancel.clone(), initial_day).instrument(span),
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
    maintenance: Arc<dyn MaintenanceService>,
    tick_interval: Duration,
    cancel: CancellationToken,
    initial_day: chrono::NaiveDate,
) {
    let admin_ctx = AuthContext::Admin {
        admin_id: Uuid::nil(),
    };

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
                    run_vacuum(maintenance.as_ref(), &admin_ctx).await;
                }
            }
        }
    }
}

pub(crate) async fn run_vacuum(maintenance: &dyn MaintenanceService, admin_ctx: &AuthContext) {
    match auth_context::with_context(admin_ctx.clone(), maintenance.run_incremental_vacuum()).await
    {
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
