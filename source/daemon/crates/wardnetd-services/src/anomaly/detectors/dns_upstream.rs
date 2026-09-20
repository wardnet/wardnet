use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use wardnet_common::anomaly::{Anomaly, AnomalyReport, AnomalyStatus, AnomalyType};

use crate::anomaly::detector::AnomalyDetector;
use crate::dns::UpstreamHealth;

/// How often the sweep reads the prober's snapshot.
///
/// Reading an `ArcSwap` costs nothing, so the cadence is chosen for how
/// promptly an admin should learn rather than for expense. The prober itself
/// probes every 30s and debounces two consecutive misses before flagging an
/// upstream, so the condition is already at least ~60s old by the time it can
/// be seen here — sweeping faster than that would only shorten the last leg.
const SWEEP_INTERVAL: Duration = Duration::from_mins(1);

/// Raises an anomaly for an upstream DNS server the reachability prober has
/// taken out of rotation.
///
/// **Preventive**, for the same reason the blocklist detector is: reachability
/// is a *state* the prober already maintains, so reading it has no transition
/// to miss. An event-driven version would have to observe the exact moment an
/// upstream flipped, and a daemon restart mid-outage would hide it — whereas a
/// sweep that reads current state reports a server that has been down since
/// before this process started just as readily as one that failed a minute
/// ago.
///
/// Firing once per outage rather than once per sweep is the anomaly service's
/// deduplication doing its job, not anything here: this reports the condition
/// on every sweep for as long as it holds.
pub struct DnsUpstreamUnreachableDetector {
    health: Arc<UpstreamHealth>,
}

impl DnsUpstreamUnreachableDetector {
    #[must_use]
    pub fn new(health: Arc<UpstreamHealth>) -> Self {
        Self { health }
    }

    fn report(address: &str) -> AnomalyReport {
        AnomalyReport::new(
            AnomalyType::DnsUpstreamUnreachable,
            format!("Upstream DNS server {address} is not responding"),
        )
        // The address is the subject: one open anomaly per upstream, so a
        // second failing server raises its own entry rather than being
        // folded into the first one's.
        .with_subject(address.to_owned())
        .with_details(serde_json::json!({ "address": address }))
    }
}

#[async_trait]
impl AnomalyDetector for DnsUpstreamUnreachableDetector {
    fn anomaly_type(&self) -> AnomalyType {
        AnomalyType::DnsUpstreamUnreachable
    }

    fn interval(&self) -> Option<Duration> {
        Some(SWEEP_INTERVAL)
    }

    async fn detect(&self) -> anyhow::Result<Vec<AnomalyReport>> {
        Ok(self
            .health
            .unreachable()
            .iter()
            .map(|address| Self::report(address))
            .collect())
    }

    async fn reevaluate(&self, anomaly: &Anomaly) -> anyhow::Result<AnomalyStatus> {
        let Some(address) = anomaly.subject_id.as_deref() else {
            // Nothing to look up. Close it rather than leaving an entry
            // nobody can act on.
            return Ok(AnomalyStatus::Resolved);
        };

        // `is_unreachable` is false both when the upstream has recovered and
        // when it has left the snapshot entirely — removed from the config,
        // or DNS switched off/recursive so the prober published nothing.
        // Every one of those means the condition that opened this anomaly no
        // longer holds, so they all resolve.
        if self.health.is_unreachable(address) {
            Ok(AnomalyStatus::Open)
        } else {
            Ok(AnomalyStatus::Resolved)
        }
    }
}
