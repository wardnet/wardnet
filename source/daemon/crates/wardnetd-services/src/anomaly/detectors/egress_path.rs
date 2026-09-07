use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use wardnet_common::anomaly::{Anomaly, AnomalyReport, AnomalyStatus, AnomalyType};
use wardnet_common::egress_path::{PathHealth, ProbeStage};

use crate::anomaly::detector::AnomalyDetector;
use crate::egress_path::health::EgressPathHealth;

/// How often the sweep reads the prober's snapshot.
///
/// Reading an `ArcSwap` costs nothing, so the cadence is chosen for how promptly
/// an admin should learn. The prober itself runs every 60s, so anything faster
/// only shortens the last leg.
const SWEEP_INTERVAL: Duration = Duration::from_mins(1);

/// Which failure this detector owns.
///
/// The two stages are separate anomaly types rather than one type with the
/// stage in `details` because an `AnomalyType` carries the remediation hint, and
/// these two hints have nothing in common: "nothing is getting out" and "large
/// packets are being dropped" send an admin to entirely different places.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Watches {
    /// The TCP handshake does not complete.
    Connect,
    /// The handshake completes and the transfer does not — the MSS/MTU
    /// signature, and the state a connect-only probe reports as healthy.
    Transfer,
}

impl Watches {
    const fn anomaly_type(self) -> AnomalyType {
        match self {
            Self::Connect => AnomalyType::EgressPathUnreachable,
            Self::Transfer => AnomalyType::EgressPathDegraded,
        }
    }

    /// Does this path currently exhibit the condition this detector owns?
    ///
    /// The two are mutually exclusive by construction: a path whose connect
    /// fails has no transfer stage to judge, so it raises exactly one anomaly
    /// rather than both.
    fn holds(self, health: &PathHealth) -> bool {
        match self {
            Self::Connect => health.outcome.connect_failed(),
            Self::Transfer => health.outcome.degraded(),
        }
    }

    const fn stage(self) -> ProbeStage {
        match self {
            Self::Connect => ProbeStage::Connect,
            Self::Transfer => ProbeStage::Transfer,
        }
    }
}

/// Raises an anomaly for an egress path the prober cannot get traffic over.
///
/// **Preventive**, reading state the prober already maintains: a path that has
/// been broken since before this process started is reported just as readily as
/// one that broke a minute ago.
///
/// An **empty snapshot raises nothing**. Nothing measured is not the same as
/// everything down, and the first round has not necessarily completed when the
/// first sweep runs.
pub struct EgressPathDetector {
    health: Arc<EgressPathHealth>,
    watches: Watches,
}

impl EgressPathDetector {
    /// A detector for paths that cannot connect at all.
    #[must_use]
    pub fn unreachable(health: Arc<EgressPathHealth>) -> Self {
        Self {
            health,
            watches: Watches::Connect,
        }
    }

    /// A detector for paths that connect and then stall on large segments.
    #[must_use]
    pub fn degraded(health: Arc<EgressPathHealth>) -> Self {
        Self {
            health,
            watches: Watches::Transfer,
        }
    }

    fn report(&self, health: &PathHealth) -> AnomalyReport {
        let label = &health.label;
        let message = match self.watches {
            Watches::Connect => format!("No connections are completing over {label}"),
            Watches::Transfer => {
                format!("{label} connects but cannot carry a full-size transfer")
            }
        };
        let error = match self.watches {
            Watches::Connect => health.outcome.connect.error.clone(),
            Watches::Transfer => health
                .outcome
                .transfer
                .as_ref()
                .and_then(|t| t.error.clone()),
        };

        AnomalyReport::new(self.watches.anomaly_type(), message)
            // The egress path is the subject — a tunnel's id, or `direct`. One
            // open anomaly per path, so a second failing path raises its own.
            .with_subject(health.path.clone())
            .with_details(serde_json::json!({
                "path": health.path,
                "label": health.label,
                "stage": self.watches.stage().as_str(),
                "error": error,
            }))
    }
}

#[async_trait]
impl AnomalyDetector for EgressPathDetector {
    fn anomaly_type(&self) -> AnomalyType {
        self.watches.anomaly_type()
    }

    fn interval(&self) -> Option<Duration> {
        Some(SWEEP_INTERVAL)
    }

    async fn detect(&self) -> anyhow::Result<Vec<AnomalyReport>> {
        Ok(self
            .health
            .snapshot()
            .iter()
            .filter(|health| self.watches.holds(health))
            .map(|health| self.report(health))
            .collect())
    }

    async fn reevaluate(&self, anomaly: &Anomaly) -> anyhow::Result<AnomalyStatus> {
        let Some(path) = anomaly.subject_id.as_deref() else {
            return Ok(AnomalyStatus::Resolved);
        };

        // A path absent from the snapshot has left the set we probe — the
        // tunnel was deleted, or it is no longer up, in which case
        // `TunnelUnhealthy` owns it. Either way this condition no longer holds.
        match self.health.get(path) {
            Some(health) if self.watches.holds(&health) => Ok(AnomalyStatus::Open),
            _ => Ok(AnomalyStatus::Resolved),
        }
    }
}
