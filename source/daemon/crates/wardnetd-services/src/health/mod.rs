//! Health-monitor subsystem (issue #214).
//!
//! A Spring-Actuator-style aggregator: components register lightweight
//! [`HealthCheck`]s, the [`HealthMonitor`] re-runs them all on a fixed tick,
//! and the resulting [`HealthSnapshot`] feeds two consumers:
//!
//! * the unauthenticated `GET /health` endpoint (200 when UP, 503 when DOWN);
//! * the **health-gated soft watchdog**, which only sends
//!   `sd_notify(WATCHDOG=1)` while overall health is UP and the snapshot is
//!   fresh — so a wedged subsystem (process alive, but DNS/DB deadlocked)
//!   triggers a proportionate systemd *service* restart rather than a full
//!   `/dev/watchdog` host reboot.
//!
//! The hardware `/dev/watchdog` pet is deliberately **never** gated on this
//! subsystem — see [`crate::system::watchdog_ops`] and the ADR. This module
//! only reports status; recovery policy lives in the runners.
//!
//! ## Isolation
//!
//! Each [`HealthCheck::check`] is wrapped in a [`tokio::time::timeout`] on
//! every refresh, so a hung probe is recorded as `Down { detail: "timeout" }`
//! instead of stalling the whole cycle. Checks run **concurrently** on the
//! existing runtime (`futures::future::join_all`), never on OS threads — every
//! check must be genuinely async, wrapping any unavoidable blocking work in
//! `spawn_blocking` (as the `SQLite` layer already does). There is deliberately
//! no segregated health runtime; the ADR records it as a future option.

pub mod checks;

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use arc_swap::ArcSwap;
use async_trait::async_trait;
use serde::Serialize;

/// Outcome of a single [`HealthCheck::check`] invocation.
///
/// `Down` carries a short human-readable `detail` (e.g. `"connection refused"`,
/// `"timeout"`) surfaced on the `/health` endpoint and in logs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckOutcome {
    /// The probed component is healthy.
    Up,
    /// The probed component is unhealthy; `detail` explains why.
    Down {
        /// Short diagnostic string, safe to expose unauthenticated.
        detail: String,
    },
}

impl CheckOutcome {
    /// Convenience constructor for a failed outcome from any displayable error.
    pub fn down(detail: impl Into<String>) -> Self {
        Self::Down {
            detail: detail.into(),
        }
    }

    #[must_use]
    fn is_up(&self) -> bool {
        matches!(self, Self::Up)
    }
}

/// Debounced, aggregated status of a component or of the daemon overall.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum HealthStatus {
    /// Healthy.
    Up,
    /// Unhealthy.
    Down,
}

/// A single component's debounced status, as it appears in a [`HealthSnapshot`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ComponentHealth {
    /// The check's stable name (e.g. `"database"`, `"dns"`).
    pub name: String,
    /// Debounced status after applying the consecutive-failure threshold.
    pub status: HealthStatus,
    /// Present only while `status == Down`; the most recent failure detail.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// Immutable point-in-time view of daemon health, swapped atomically into the
/// [`HealthMonitor`] on every refresh and read lock-free by both the
/// `/health` handler and the soft watchdog.
#[derive(Debug, Clone)]
pub struct HealthSnapshot {
    /// `Down` if **any** component is `Down`, else `Up`.
    pub overall: HealthStatus,
    /// Per-component debounced statuses, in registration order.
    pub components: Vec<ComponentHealth>,
    /// Monotonic stamp of when this snapshot was produced. The soft watchdog
    /// treats a snapshot older than `2 × refresh_interval` as stale (the
    /// refresh loop itself has stalled) and withholds the watchdog ping.
    pub refreshed_at: Instant,
}

impl HealthSnapshot {
    /// Whether this snapshot is fresh enough to act on — younger than
    /// `max_age`. A stale snapshot means the refresh loop stopped running.
    #[must_use]
    pub fn is_fresh(&self, max_age: Duration) -> bool {
        self.refreshed_at.elapsed() < max_age
    }
}

/// A pluggable health probe. Implementations adapt a subsystem (DB, DNS, …)
/// into a cheap async readiness check.
///
/// Mirrors the trait-object pattern used by [`crate::system::SystemPowerOps`]:
/// `Send + Sync`, `#[async_trait]`, registered onto the [`HealthMonitor`] as
/// `Arc<dyn HealthCheck>`. `check()` must be cheap, non-blocking, and must
/// never panic — return [`CheckOutcome::down`] on any error instead.
#[async_trait]
pub trait HealthCheck: Send + Sync {
    /// Stable, unique name for this check. Used as the component key in the
    /// snapshot and as the debounce-counter key, so it must not change at
    /// runtime — hence `&'static str`.
    fn name(&self) -> &'static str;

    /// Probe the component. Implementations should keep this well under the
    /// configured per-check timeout.
    async fn check(&self) -> CheckOutcome;
}

/// Aggregator over registered [`HealthCheck`]s.
///
/// Holds the checks plus an [`ArcSwap`] of the latest [`HealthSnapshot`] for
/// lock-free reads (the same pattern the local-DNS `AuthoritativeView` uses).
/// [`refresh`](Self::refresh) is the sole writer and is driven by
/// `HealthMonitorRunner`.
pub struct HealthMonitor {
    checks: Vec<Arc<dyn HealthCheck>>,
    snapshot: ArcSwap<HealthSnapshot>,
    /// Consecutive-failure counters keyed by check name. Only mutated by
    /// `refresh`, which runs serially under the monitor runner.
    failure_counts: Mutex<HashMap<String, u32>>,
    failure_threshold: u32,
    check_timeout: Duration,
}

impl HealthMonitor {
    /// Build an empty monitor. The initial snapshot is optimistic (UP, no
    /// components, freshly stamped) so a reader between construction and the
    /// first [`refresh`](Self::refresh) sees a sane fresh value; startup runs
    /// one refresh before signalling readiness.
    #[must_use]
    pub fn new(failure_threshold: u32, check_timeout: Duration) -> Self {
        let initial = HealthSnapshot {
            overall: HealthStatus::Up,
            components: Vec::new(),
            refreshed_at: Instant::now(),
        };
        Self {
            checks: Vec::new(),
            snapshot: ArcSwap::from_pointee(initial),
            failure_counts: Mutex::new(HashMap::new()),
            failure_threshold: failure_threshold.max(1),
            check_timeout,
        }
    }

    /// Register a check. Call during wiring, before the monitor is shared as
    /// an `Arc`.
    pub fn register(&mut self, check: Arc<dyn HealthCheck>) {
        self.checks.push(check);
    }

    /// Latest snapshot, read lock-free.
    #[must_use]
    pub fn snapshot(&self) -> Arc<HealthSnapshot> {
        self.snapshot.load_full()
    }

    /// Re-run every check concurrently, apply the consecutive-failure
    /// debounce, recompute the overall status, and publish a fresh snapshot.
    ///
    /// Each check is bounded by `check_timeout`; an overrun counts as a
    /// failure with detail `"timeout"`.
    pub async fn refresh(&self) {
        // Run all checks concurrently on the current runtime. Each future
        // resolves to (name, CheckOutcome); a timeout maps to a Down outcome.
        let futures = self.checks.iter().map(|check| {
            let check = check.clone();
            let timeout = self.check_timeout;
            async move {
                let name = check.name().to_owned();
                let outcome = match tokio::time::timeout(timeout, check.check()).await {
                    Ok(outcome) => outcome,
                    Err(_) => CheckOutcome::down("timeout"),
                };
                (name, outcome)
            }
        });
        let results = futures::future::join_all(futures).await;

        let mut counts = self
            .failure_counts
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let mut components = Vec::with_capacity(results.len());
        let mut overall = HealthStatus::Up;

        for (name, outcome) in results {
            let entry = counts.entry(name.clone()).or_insert(0);
            let component = if outcome.is_up() {
                // Recovery is immediate: a single success clears the streak.
                *entry = 0;
                ComponentHealth {
                    name,
                    status: HealthStatus::Up,
                    detail: None,
                }
            } else {
                *entry = entry.saturating_add(1);
                let detail = match &outcome {
                    CheckOutcome::Down { detail } => detail.clone(),
                    CheckOutcome::Up => unreachable!("is_up() was false"),
                };
                if *entry >= self.failure_threshold {
                    // Debounced to DOWN.
                    overall = HealthStatus::Down;
                    ComponentHealth {
                        name,
                        status: HealthStatus::Down,
                        detail: Some(detail),
                    }
                } else {
                    // Failing, but not yet over the threshold — still UP, but
                    // log the transient so a flapping probe is visible.
                    tracing::debug!(
                        check = %name,
                        consecutive = *entry,
                        threshold = self.failure_threshold,
                        detail = %detail,
                        "health check failing (debouncing): check={name}, {consecutive}/{threshold}",
                        name = name,
                        consecutive = *entry,
                        threshold = self.failure_threshold,
                    );
                    ComponentHealth {
                        name,
                        status: HealthStatus::Up,
                        detail: None,
                    }
                }
            };
            components.push(component);
        }

        drop(counts);

        if overall == HealthStatus::Down {
            let down: Vec<&str> = components
                .iter()
                .filter(|c| c.status == HealthStatus::Down)
                .map(|c| c.name.as_str())
                .collect();
            tracing::warn!(
                components = ?down,
                "health overall DOWN: components={down:?}",
                down = down,
            );
        }

        self.snapshot.store(Arc::new(HealthSnapshot {
            overall,
            components,
            refreshed_at: Instant::now(),
        }));
    }
}
