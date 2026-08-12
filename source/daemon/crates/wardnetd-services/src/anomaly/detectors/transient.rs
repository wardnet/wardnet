use std::time::Duration;

use async_trait::async_trait;
use wardnet_common::anomaly::{Anomaly, AnomalyStatus, AnomalyType};

use crate::anomaly::detector::AnomalyDetector;

/// How long an unverifiable condition keeps asserting itself.
///
/// Long enough that a genuinely recurring problem keeps the entry alive
/// (each fresh observation pushes `last_seen_at` forward), short enough that
/// a one-off does not sit on the dashboard for days.
const DEFAULT_STALE_AFTER: Duration = Duration::from_hours(6);

/// A detector for conditions with no authoritative "is it still true?" check.
///
/// Two of the catalogue's entries are one-shot observations of something that
/// already happened, with nothing left to interrogate afterwards:
///
/// * `RouteTableLost` — the kernel routing table is rebuilt by the routing
///   reconcile, and the daemon has no way to ask "was table 100 missing
///   earlier?" after the fact.
/// * `DhcpConflict` — two hosts claimed one address at a moment in time. There
///   is no lasting artefact to re-inspect. (No production publisher raises
///   this today; the event is only ever constructed in tests. The mapping is
///   here so the path is live the moment one lands.)
///
/// Rather than inventing a fake check, such a detector expires: it stays open
/// while it keeps being observed and resolves once it has been quiet for
/// [`AnomalyDetector::stale_after`]. `reevaluate` is therefore an
/// unconditional `Open` — the service applies the staleness rule before it
/// ever asks.
///
/// This is deliberately one parameterised type rather than a file per entry:
/// the behaviour is identical and duplicating it would invite the two copies
/// to drift.
pub struct TransientDetector {
    anomaly_type: AnomalyType,
    stale_after: Duration,
}

impl TransientDetector {
    #[must_use]
    pub const fn new(anomaly_type: AnomalyType) -> Self {
        Self {
            anomaly_type,
            stale_after: DEFAULT_STALE_AFTER,
        }
    }

    /// Override the expiry window. Used by tests to avoid waiting six hours.
    #[must_use]
    pub const fn with_stale_after(mut self, stale_after: Duration) -> Self {
        self.stale_after = stale_after;
        self
    }
}

#[async_trait]
impl AnomalyDetector for TransientDetector {
    fn anomaly_type(&self) -> AnomalyType {
        self.anomaly_type
    }

    fn stale_after(&self) -> Option<Duration> {
        Some(self.stale_after)
    }

    async fn reevaluate(&self, _anomaly: &Anomaly) -> anyhow::Result<AnomalyStatus> {
        // Never reached while the condition is fresh; the staleness rule above
        // is what eventually closes these.
        Ok(AnomalyStatus::Open)
    }
}
