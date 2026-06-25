//! Unit tests for the [`HealthMonitor`] aggregation + debounce logic.

use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use async_trait::async_trait;

use crate::health::{CheckOutcome, HealthCheck, HealthMonitor, HealthStatus};

/// Long timeout so probes never trip the per-check timeout in tests that
/// aren't exercising it.
const NO_TIMEOUT: Duration = Duration::from_secs(30);

/// A check that returns a scripted sequence of outcomes; once exhausted it
/// repeats the final scripted value.
struct ScriptedCheck {
    name: &'static str,
    outcomes: Mutex<VecDeque<CheckOutcome>>,
    last: CheckOutcome,
}

impl ScriptedCheck {
    fn new(name: &'static str, sequence: Vec<CheckOutcome>) -> Self {
        let last = sequence.last().cloned().unwrap_or(CheckOutcome::Up);
        Self {
            name,
            outcomes: Mutex::new(sequence.into()),
            last,
        }
    }
}

#[async_trait]
impl HealthCheck for ScriptedCheck {
    fn name(&self) -> &'static str {
        self.name
    }

    async fn check(&self) -> CheckOutcome {
        self.outcomes
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| self.last.clone())
    }
}

/// A check that always reports the given outcome.
struct ConstCheck {
    name: &'static str,
    outcome: CheckOutcome,
}

impl ConstCheck {
    fn up(name: &'static str) -> Self {
        Self {
            name,
            outcome: CheckOutcome::Up,
        }
    }

    fn down(name: &'static str, detail: &str) -> Self {
        Self {
            name,
            outcome: CheckOutcome::down(detail),
        }
    }
}

#[async_trait]
impl HealthCheck for ConstCheck {
    fn name(&self) -> &'static str {
        self.name
    }

    async fn check(&self) -> CheckOutcome {
        self.outcome.clone()
    }
}

/// A check that sleeps past any sane per-check timeout.
struct SlowCheck;

#[async_trait]
impl HealthCheck for SlowCheck {
    fn name(&self) -> &'static str {
        "slow"
    }

    async fn check(&self) -> CheckOutcome {
        tokio::time::sleep(Duration::from_millis(500)).await;
        CheckOutcome::Up
    }
}

fn status_of(monitor: &HealthMonitor, name: &str) -> Option<HealthStatus> {
    monitor
        .snapshot()
        .components
        .iter()
        .find(|c| c.name == name)
        .map(|c| c.status)
}

#[tokio::test]
async fn aggregation_all_up_yields_overall_up() {
    let mut monitor = HealthMonitor::new(1, NO_TIMEOUT);
    monitor.register(Arc::new(ConstCheck::up("a")));
    monitor.register(Arc::new(ConstCheck::up("b")));
    monitor.refresh().await;

    let snapshot = monitor.snapshot();
    assert_eq!(snapshot.overall, HealthStatus::Up);
    assert_eq!(snapshot.components.len(), 2);
    assert!(
        snapshot
            .components
            .iter()
            .all(|c| c.status == HealthStatus::Up)
    );
}

#[tokio::test]
async fn overall_down_when_any_component_down() {
    // threshold = 1 ⇒ a single failure flips the component immediately.
    let mut monitor = HealthMonitor::new(1, NO_TIMEOUT);
    monitor.register(Arc::new(ConstCheck::up("good")));
    monitor.register(Arc::new(ConstCheck::down("bad", "boom")));
    monitor.refresh().await;

    let snapshot = monitor.snapshot();
    assert_eq!(snapshot.overall, HealthStatus::Down);
    assert_eq!(status_of(&monitor, "good"), Some(HealthStatus::Up));
    assert_eq!(status_of(&monitor, "bad"), Some(HealthStatus::Down));
    // Detail is surfaced only on the DOWN component.
    let bad = snapshot
        .components
        .iter()
        .find(|c| c.name == "bad")
        .unwrap();
    assert_eq!(bad.detail.as_deref(), Some("boom"));
}

#[tokio::test]
async fn debounce_requires_y_consecutive_failures() {
    // threshold = 3: the component stays UP for the first two failures and
    // only flips DOWN on the third consecutive one.
    let mut monitor = HealthMonitor::new(3, NO_TIMEOUT);
    monitor.register(Arc::new(ConstCheck::down("flappy", "nope")));

    monitor.refresh().await;
    assert_eq!(status_of(&monitor, "flappy"), Some(HealthStatus::Up));
    assert_eq!(monitor.snapshot().overall, HealthStatus::Up);

    monitor.refresh().await;
    assert_eq!(status_of(&monitor, "flappy"), Some(HealthStatus::Up));

    monitor.refresh().await;
    assert_eq!(status_of(&monitor, "flappy"), Some(HealthStatus::Down));
    assert_eq!(monitor.snapshot().overall, HealthStatus::Down);
}

#[tokio::test]
async fn recovers_immediately_and_resets_streak_on_success() {
    // Down, Down (→DOWN at threshold 2), then Up (→recover), then a single
    // Down must NOT re-trip, proving the streak reset.
    let mut monitor = HealthMonitor::new(2, NO_TIMEOUT);
    monitor.register(Arc::new(ScriptedCheck::new(
        "svc",
        vec![
            CheckOutcome::down("d1"),
            CheckOutcome::down("d2"),
            CheckOutcome::Up,
            CheckOutcome::down("d3"),
        ],
    )));

    monitor.refresh().await; // d1 → 1/2, still UP
    assert_eq!(status_of(&monitor, "svc"), Some(HealthStatus::Up));
    monitor.refresh().await; // d2 → 2/2, DOWN
    assert_eq!(status_of(&monitor, "svc"), Some(HealthStatus::Down));
    monitor.refresh().await; // Up → recover immediately, streak reset
    assert_eq!(status_of(&monitor, "svc"), Some(HealthStatus::Up));
    monitor.refresh().await; // d3 → 1/2 again, still UP (streak was reset)
    assert_eq!(status_of(&monitor, "svc"), Some(HealthStatus::Up));
    assert_eq!(monitor.snapshot().overall, HealthStatus::Up);
}

#[tokio::test]
async fn checks_run_concurrently_and_timeout_counts_as_failure() {
    // threshold = 1, tiny per-check timeout: the slow check is recorded as a
    // timeout failure without stalling the (fast) sibling check.
    let mut monitor = HealthMonitor::new(1, Duration::from_millis(50));
    monitor.register(Arc::new(ConstCheck::up("fast")));
    monitor.register(Arc::new(SlowCheck));
    monitor.refresh().await;

    let snapshot = monitor.snapshot();
    assert_eq!(snapshot.overall, HealthStatus::Down);
    assert_eq!(status_of(&monitor, "fast"), Some(HealthStatus::Up));
    let slow = snapshot
        .components
        .iter()
        .find(|c| c.name == "slow")
        .unwrap();
    assert_eq!(slow.status, HealthStatus::Down);
    assert_eq!(slow.detail.as_deref(), Some("timeout"));
}

#[tokio::test]
async fn snapshot_freshness_tracks_refresh() {
    let monitor = HealthMonitor::new(1, NO_TIMEOUT);
    monitor.refresh().await;
    let snapshot = monitor.snapshot();
    // Just-refreshed snapshots are fresh against a generous window and stale
    // against a zero window.
    assert!(snapshot.is_fresh(Duration::from_secs(10)));
    assert!(!snapshot.is_fresh(Duration::ZERO));
}
