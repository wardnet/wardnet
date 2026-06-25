//! Test that [`HealthMonitorRunner`] actually drives `refresh()` on its tick.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use async_trait::async_trait;

use wardnetd_services::HealthMonitor;
use wardnetd_services::health::{CheckOutcome, HealthCheck};

use crate::health_runner::HealthMonitorRunner;

/// Counts how many times it was probed.
struct CountingCheck {
    count: Arc<AtomicU32>,
}

#[async_trait]
impl HealthCheck for CountingCheck {
    fn name(&self) -> &'static str {
        "counting"
    }
    async fn check(&self) -> CheckOutcome {
        self.count.fetch_add(1, Ordering::SeqCst);
        CheckOutcome::Up
    }
}

#[tokio::test]
async fn runner_refreshes_on_each_tick() {
    let count = Arc::new(AtomicU32::new(0));
    let mut monitor = HealthMonitor::new(1, Duration::from_secs(30));
    monitor.register(Arc::new(CountingCheck {
        count: count.clone(),
    }));
    let monitor = Arc::new(monitor);

    let parent = tracing::info_span!("test");
    let runner =
        HealthMonitorRunner::start_with_interval(monitor, Duration::from_millis(50), &parent);

    tokio::time::sleep(Duration::from_millis(220)).await;
    runner.shutdown().await;

    assert!(
        count.load(Ordering::SeqCst) >= 2,
        "expected the runner to refresh repeatedly, got {} probes",
        count.load(Ordering::SeqCst)
    );
}

/// The default-interval constructor refreshes at least once (the first tick
/// fires immediately) and shuts down cleanly. Covers `start`, which production
/// uses, without waiting out the 5 s cadence.
#[tokio::test]
async fn default_interval_runner_refreshes_and_shuts_down() {
    let count = Arc::new(AtomicU32::new(0));
    let mut monitor = HealthMonitor::new(1, Duration::from_secs(30));
    monitor.register(Arc::new(CountingCheck {
        count: count.clone(),
    }));
    let monitor = Arc::new(monitor);

    let parent = tracing::info_span!("test");
    let runner = HealthMonitorRunner::start(monitor, &parent);

    // `interval` fires its first tick immediately, so one refresh lands well
    // before the 5 s cadence would.
    tokio::time::sleep(Duration::from_millis(100)).await;
    runner.shutdown().await;

    assert!(
        count.load(Ordering::SeqCst) >= 1,
        "expected an immediate first refresh, got {} probes",
        count.load(Ordering::SeqCst)
    );
}
