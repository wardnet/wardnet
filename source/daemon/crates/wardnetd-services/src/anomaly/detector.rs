use std::time::Duration;

use async_trait::async_trait;
use wardnet_common::anomaly::{Anomaly, AnomalyReport, AnomalyStatus, AnomalyType};

/// Knows how to find, and how to un-find, one class of anomaly.
///
/// Exactly one detector owns each [`AnomalyType`]. A detector answers two
/// questions, in either of the subsystem's two modes:
///
/// * **Preventive** — `detect` sweeps for the condition on a schedule the
///   detector itself sets via [`AnomalyDetector::interval`]. Use this when the
///   condition is a *state* you can go and look at, like a blocklist's failure
///   count. State polling has no crossing to miss: a restart or a lost write
///   cannot hide it.
/// * **Reactive** — nothing to sweep; the condition arrives as a domain event
///   and the listener submits it. Such detectors leave `interval` at `None`
///   and only implement `reevaluate`.
///
/// `reevaluate` is what closes the loop. Without it an anomaly opened once
/// would stay open forever, and the admin would never learn the problem went
/// away. Where no authoritative check exists, a detector says so by declaring
/// [`AnomalyDetector::stale_after`] instead.
///
/// Errors are `anyhow` because this is an infrastructure-facing boundary — the
/// service maps them to `AppError`. A detector that returns `Err` is logged
/// and skipped for that pass; it never fails the cycle for other detectors.
#[async_trait]
pub trait AnomalyDetector: Send + Sync {
    /// The catalogue entry this detector owns.
    fn anomaly_type(&self) -> AnomalyType;

    /// How often the engine should run [`AnomalyDetector::detect`].
    ///
    /// `None` — the default — means reactive-only: this detector is never
    /// swept, and its anomalies arrive through `AnomalyService::submit`.
    fn interval(&self) -> Option<Duration> {
        None
    }

    /// Auto-resolve an open anomaly that has not been re-observed within this
    /// window.
    ///
    /// For conditions with no authoritative "is it still true?" check — a
    /// routing table that vanished, a DHCP address conflict — this is the
    /// honest answer: we cannot know, so we stop asserting it after a while
    /// rather than leaving a stale entry on the dashboard forever. Checked
    /// before [`AnomalyDetector::reevaluate`], which such a detector can then
    /// leave as an unconditional `Open`.
    fn stale_after(&self) -> Option<Duration> {
        None
    }

    /// Sweep for the condition and report every current occurrence.
    ///
    /// Reports are submitted through the normal deduplicating path, so
    /// returning the same occurrence on every sweep is expected and cheap —
    /// it refreshes the existing anomaly rather than raising a new one.
    async fn detect(&self) -> anyhow::Result<Vec<AnomalyReport>> {
        Ok(Vec::new())
    }

    /// Does the condition behind an already-open anomaly still hold?
    async fn reevaluate(&self, anomaly: &Anomaly) -> anyhow::Result<AnomalyStatus>;
}
