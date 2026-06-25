//! Tests for the health-gated soft watchdog (issue #214).

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

use async_trait::async_trait;

use wardnetd_services::health::{CheckOutcome, HealthCheck};
use wardnetd_services::{HealthMonitor, HealthSnapshot, HealthStatus};

use crate::watchdog::soft::should_ping;
use crate::watchdog::{Notifier, SoftWatchdogRunner};

/// Captures `WATCHDOG=1` pings instead of touching a real `NOTIFY_SOCKET`.
#[derive(Default)]
struct FakeNotifier {
    watchdog: AtomicU32,
}

impl FakeNotifier {
    fn watchdog_count(&self) -> u32 {
        self.watchdog.load(Ordering::SeqCst)
    }
}

impl Notifier for FakeNotifier {
    fn notify_ready(&self) {
        // READY=1 is sent from main.rs, not the runner under test — no-op.
    }
    fn ping_watchdog(&self) {
        self.watchdog.fetch_add(1, Ordering::SeqCst);
    }
}

/// A check that is always down — used to force overall DOWN.
struct AlwaysDown;

#[async_trait]
impl HealthCheck for AlwaysDown {
    fn name(&self) -> &'static str {
        "always-down"
    }
    async fn check(&self) -> CheckOutcome {
        CheckOutcome::down("forced down")
    }
}

fn snapshot(overall: HealthStatus) -> HealthSnapshot {
    HealthSnapshot {
        overall,
        components: Vec::new(),
        refreshed_at: Instant::now(),
    }
}

#[test]
fn should_ping_only_when_up_and_fresh() {
    let generous = Duration::from_mins(1);
    // Healthy + fresh ⇒ ping.
    assert!(should_ping(&snapshot(HealthStatus::Up), generous));
    // Down + fresh ⇒ withhold.
    assert!(!should_ping(&snapshot(HealthStatus::Down), generous));
    // Healthy but stale (zero-age window ⇒ never fresh) ⇒ withhold.
    assert!(!should_ping(&snapshot(HealthStatus::Up), Duration::ZERO));
}

#[tokio::test]
async fn pings_when_healthy_and_fresh() {
    // Empty monitor ⇒ overall UP after refresh, snapshot fresh.
    let monitor = Arc::new(HealthMonitor::new(1, Duration::from_secs(30)));
    monitor.refresh().await;
    let notifier = Arc::new(FakeNotifier::default());
    let parent = tracing::info_span!("test");
    let runner = SoftWatchdogRunner::start(
        monitor,
        notifier.clone(),
        Duration::from_millis(50),
        Duration::from_mins(1),
        &parent,
    );

    tokio::time::sleep(Duration::from_millis(220)).await;
    runner.shutdown().await;

    assert!(
        notifier.watchdog_count() >= 2,
        "expected periodic WATCHDOG pings while healthy, got {}",
        notifier.watchdog_count()
    );
}

#[tokio::test]
async fn withholds_when_unhealthy() {
    let mut monitor = HealthMonitor::new(1, Duration::from_secs(30));
    monitor.register(Arc::new(AlwaysDown));
    let monitor = Arc::new(monitor);
    monitor.refresh().await; // overall DOWN
    assert_eq!(monitor.snapshot().overall, HealthStatus::Down);

    let notifier = Arc::new(FakeNotifier::default());
    let parent = tracing::info_span!("test");
    let runner = SoftWatchdogRunner::start(
        monitor,
        notifier.clone(),
        Duration::from_millis(50),
        Duration::from_mins(1),
        &parent,
    );

    tokio::time::sleep(Duration::from_millis(180)).await;
    runner.shutdown().await;

    assert_eq!(
        notifier.watchdog_count(),
        0,
        "must withhold the WATCHDOG ping while DOWN so systemd restarts the service",
    );
}

#[tokio::test]
async fn withholds_when_snapshot_stale() {
    // Healthy, but a zero-age staleness window means every snapshot is stale —
    // models the case where the refresh loop has stopped advancing.
    let monitor = Arc::new(HealthMonitor::new(1, Duration::from_secs(30)));
    monitor.refresh().await;
    let notifier = Arc::new(FakeNotifier::default());
    let parent = tracing::info_span!("test");
    let runner = SoftWatchdogRunner::start(
        monitor,
        notifier.clone(),
        Duration::from_millis(50),
        Duration::ZERO,
        &parent,
    );

    tokio::time::sleep(Duration::from_millis(180)).await;
    runner.shutdown().await;

    assert_eq!(
        notifier.watchdog_count(),
        0,
        "must withhold the WATCHDOG ping when the snapshot is stale",
    );
}
