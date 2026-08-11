//! Daily device-retention runner (issue #1181).
//!
//! Deletes unmanaged devices absent for longer than
//! [`DEVICE_RETENTION_DAYS`](super::discovery::DEVICE_RETENTION_DAYS), once per
//! calendar day.
//!
//! Without this the `devices` table only ever grows: a row is created on
//! discovery and never removed. MAC randomization sharpens it — a phone that
//! re-randomizes on every join mints a fresh row each time, so the rows that
//! will *never* return under the same MAC are exactly the ones accumulating
//! fastest.
//!
//! Only **unmanaged** devices are eligible, which is what makes the delete safe:
//! `managed = false` implies no admin artefact references the device, so a
//! prune cannot revoke a Private-DNS grant or a Remote peer credential out from
//! under a working configuration. Managed devices are never auto-deleted at any
//! age.
//!
//! Kept separate from [`DbMaintenanceRunner`](crate::db_maintenance_runner)
//! rather than folded into it: that runner is database-level (vacuum,
//! checkpoint, optimize) and takes only a `MaintenanceService`, whereas this is
//! a domain-level policy over a specific table.
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
use crate::device::discovery::DeviceDiscoveryService;

/// How often the runner wakes to check whether a day has rolled over.
pub const TICK_INTERVAL: Duration = Duration::from_hours(1);

/// Background task that prunes long-absent unmanaged devices once per calendar
/// day.
pub struct DeviceRetentionRunner {
    cancel: CancellationToken,
    handle: tokio::task::JoinHandle<()>,
}

impl DeviceRetentionRunner {
    /// Start with the default hourly tick. Production entry point.
    pub fn start(discovery: Arc<dyn DeviceDiscoveryService>, parent: &tracing::Span) -> Self {
        Self::start_with_interval(discovery, TICK_INTERVAL, parent)
    }

    /// Start with a custom tick interval. Tests pass short intervals so they
    /// can exercise the day-rollover logic without sleeping for an hour.
    pub fn start_with_interval(
        discovery: Arc<dyn DeviceDiscoveryService>,
        tick_interval: Duration,
        parent: &tracing::Span,
    ) -> Self {
        Self::start_with_interval_and_day(discovery, tick_interval, today(), parent)
    }

    /// Internal constructor that accepts an explicit initial day so tests can
    /// set yesterday's date and have the first tick immediately fire a prune.
    pub(crate) fn start_with_interval_and_day(
        discovery: Arc<dyn DeviceDiscoveryService>,
        tick_interval: Duration,
        initial_day: chrono::NaiveDate,
        parent: &tracing::Span,
    ) -> Self {
        let cancel = CancellationToken::new();
        let span = tracing::info_span!(parent: parent, "device_retention_runner");

        let handle = tokio::spawn(
            runner_loop(discovery, tick_interval, cancel.clone(), initial_day).instrument(span),
        );

        Self { cancel, handle }
    }

    /// Cancel the runner and wait for it to stop.
    pub async fn shutdown(self) {
        self.cancel.cancel();
        let _ = self.handle.await;
        tracing::info!("Device retention runner shut down");
    }
}

async fn runner_loop(
    discovery: Arc<dyn DeviceDiscoveryService>,
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

    let mut last_prune_day = initial_day;

    loop {
        tokio::select! {
            () = cancel.cancelled() => {
                tracing::info!("Device retention runner cancellation received");
                break;
            }
            _ = ticker.tick() => {
                let today_now = today();
                if today_now != last_prune_day {
                    last_prune_day = today_now;
                    prune(discovery.as_ref(), &admin_ctx).await;
                }
            }
        }
    }
}

/// Run one retention pass.
///
/// Failures are logged and swallowed: a prune that cannot run is a table that
/// keeps growing for another day, not a reason to tear down the runner. The
/// day-rollover gate has already advanced, so the retry lands on the next
/// calendar day rather than on the next hourly tick.
///
/// `pub(crate)` so tests can drive a single pass without the ticker.
pub(crate) async fn prune(discovery: &dyn DeviceDiscoveryService, admin_ctx: &AuthContext) {
    // Per-device detail is logged inside the service, which is where the MACs
    // and timestamps are in scope; this only reports the outcome of the pass.
    if let Err(e) =
        auth_context::with_context(admin_ctx.clone(), discovery.prune_unmanaged_devices()).await
    {
        tracing::warn!(error = %e, "device retention prune failed: {e}");
    }
}

fn today() -> chrono::NaiveDate {
    chrono::Utc::now().date_naive()
}
