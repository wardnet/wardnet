use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use uuid::Uuid;
use wardnet_common::anomaly::{Anomaly, AnomalyReport, AnomalyStatus, AnomalyType};
use wardnet_common::dns::Blocklist;

use crate::anomaly::detector::AnomalyDetector;
use crate::dns_filter::DnsFilterService;

/// How often the blocklist sweep runs.
///
/// Well under the refresh backoff's `5m` floor, so the anomaly appears
/// promptly once the counter crosses, and cheap: the sweep is a couple of
/// indexed reads over a handful of rows.
const SWEEP_INTERVAL: Duration = Duration::from_mins(5);

/// Raises an anomaly for a blocklist that keeps failing to refresh.
///
/// This is a **preventive** detector: it reads the persisted
/// `consecutive_failures` counter rather than reacting to the failure events
/// themselves. That is deliberate. The alert has to fire once, on the
/// transition past the threshold — and a crossing observed from events alone
/// is not reliable, because a lost write or a daemon restart mid-sequence
/// makes `failures == threshold` unobservable. Reading current state has no
/// crossing to miss, and covers lists that have *never* succeeded identically
/// to lists that were healthy and regressed.
///
/// The once-only property comes from the anomaly lifecycle rather than from
/// anything here: this detector reports the condition on every sweep while it
/// holds, and the service deduplicates.
pub struct BlocklistRefreshFailingDetector {
    dns_filter: Arc<dyn DnsFilterService>,
}

impl BlocklistRefreshFailingDetector {
    #[must_use]
    pub fn new(dns_filter: Arc<dyn DnsFilterService>) -> Self {
        Self { dns_filter }
    }

    /// Configured threshold, or `None` when alerting is switched off.
    async fn threshold(&self) -> anyhow::Result<Option<u32>> {
        let config = self.dns_filter.get_filter_config().await?.config;
        Ok(match config.blocklist_failure_alert_threshold {
            0 => None,
            n => Some(n),
        })
    }

    /// Find a blocklist by id within the profile recorded in `details`.
    ///
    /// Blocklists are profile-scoped, so the id alone is not enough to look
    /// one up — which is why `detect` records `profile_id` alongside it.
    async fn find_blocklist(
        &self,
        profile_id: Uuid,
        blocklist_id: Uuid,
    ) -> anyhow::Result<Option<Blocklist>> {
        let listing = match self.dns_filter.list_blocklists(profile_id).await {
            Ok(listing) => listing,
            // The profile itself is gone, so the blocklist is too.
            Err(crate::error::AppError::NotFound(_)) => return Ok(None),
            Err(e) => return Err(anyhow::anyhow!(e)),
        };
        Ok(listing
            .blocklists
            .into_iter()
            .find(|b| b.id == blocklist_id))
    }

    fn message(blocklist: &Blocklist) -> String {
        let failures = blocklist.consecutive_failures;
        let name = &blocklist.name;
        match blocklist.last_error.as_deref() {
            Some(error) => {
                format!("Blocklist \"{name}\" has failed to refresh {failures} times: {error}")
            }
            None => format!("Blocklist \"{name}\" has failed to refresh {failures} times"),
        }
    }

    fn report(profile_id: Uuid, blocklist: &Blocklist) -> AnomalyReport {
        AnomalyReport::new(
            AnomalyType::BlocklistRefreshFailing,
            Self::message(blocklist),
        )
        .with_subject(blocklist.id.to_string())
        .with_details(serde_json::json!({
            // Read back by `reevaluate`, and used by the web surfaces to deep
            // link to the profile page the list lives on.
            "profile_id": profile_id.to_string(),
            "consecutive_failures": blocklist.consecutive_failures,
            "last_error": blocklist.last_error,
        }))
    }
}

#[async_trait]
impl AnomalyDetector for BlocklistRefreshFailingDetector {
    fn anomaly_type(&self) -> AnomalyType {
        AnomalyType::BlocklistRefreshFailing
    }

    fn interval(&self) -> Option<Duration> {
        Some(SWEEP_INTERVAL)
    }

    async fn detect(&self) -> anyhow::Result<Vec<AnomalyReport>> {
        let Some(threshold) = self.threshold().await? else {
            return Ok(Vec::new());
        };

        let profiles = self.dns_filter.list_profiles().await?.profiles;
        let mut reports = Vec::new();
        for profile in profiles {
            let blocklists = self
                .dns_filter
                .list_blocklists(profile.id)
                .await?
                .blocklists;
            reports.extend(
                blocklists
                    .iter()
                    // A disabled list is not refreshed, so its stale counter is
                    // not a live problem.
                    .filter(|b| b.enabled && b.consecutive_failures >= threshold)
                    .map(|b| Self::report(profile.id, b)),
            );
        }
        Ok(reports)
    }

    async fn reevaluate(&self, anomaly: &Anomaly) -> anyhow::Result<AnomalyStatus> {
        let (Some(subject), Some(details)) = (anomaly.subject_id.as_deref(), &anomaly.details)
        else {
            // Nothing to look up. Close it rather than leaving an entry nobody
            // can ever act on.
            return Ok(AnomalyStatus::Resolved);
        };
        let (Ok(blocklist_id), Some(Ok(profile_id))) = (
            Uuid::parse_str(subject),
            details["profile_id"].as_str().map(Uuid::parse_str),
        ) else {
            return Ok(AnomalyStatus::Resolved);
        };

        let Some(blocklist) = self.find_blocklist(profile_id, blocklist_id).await? else {
            // Deleted, or moved out of the profile: no longer our problem.
            return Ok(AnomalyStatus::Resolved);
        };

        // Alerting switched off since the anomaly opened. Without this the
        // anomaly would sit on the dashboard forever: setting the threshold to
        // `0` is documented as turning the alert off, but it only stops
        // `detect` from re-reporting — a failing list's counter never returns
        // to zero on its own, so nothing else would ever close it.
        let Some(threshold) = self.threshold().await? else {
            return Ok(AnomalyStatus::Resolved);
        };

        // Any successful refresh zeroes the counter — that is the recovery
        // signal, and it is also what a URL fix or a manual refresh produces.
        // A list that has been disabled since is likewise no longer failing,
        // and a threshold raised above where this list sits means the
        // condition that opened the anomaly no longer holds.
        if !blocklist.enabled || blocklist.consecutive_failures < threshold {
            return Ok(AnomalyStatus::Resolved);
        }
        Ok(AnomalyStatus::Open)
    }
}
