use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use wardnet_common::anomaly::{Anomaly, AnomalyReport, AnomalyStatus, AnomalyType};

use crate::anomaly::detector::AnomalyDetector;
use crate::dhcp::DhcpService;

/// How often the sweep recounts renewals.
///
/// The condition is a 24-hour aggregate, so it moves slowly; sweeping faster
/// would only re-run the same `GROUP BY` against an answer that has barely
/// changed.
const SWEEP_INTERVAL: Duration = Duration::from_mins(10);

/// How far back renewals are counted.
///
/// Deliberately a day rather than an hour. Measured over a week on a live box,
/// a threshold of four renewals *per hour* fires for two dozen different MACs,
/// nearly all of them single-hour blips from phones waking from sleep. What
/// separates a broken client from a busy one is persistence, and a 24-hour
/// window expresses that without a second "sustained for N hours" rule to tune.
const WINDOW_HOURS: i64 = 24;

/// How many times over the protocol's expectation counts as a storm.
///
/// A client renews at `T1`, so a healthy one renews `86400/T1` times a day.
/// Twenty times that is far above any client renewing early — a sleeping phone
/// re-DHCPing on every wake reaches roughly 10× — and far below the observed
/// fault, which ran at 1,250×.
const EXCESS_FACTOR: f64 = 20.0;

/// The fewest renewals in a day that can ever be a storm.
///
/// With a very long lease the derived threshold falls below a handful, where a
/// single retry pair would raise an anomaly. This floor keeps the detector from
/// alerting on noise no matter how the lease is configured.
const MIN_THRESHOLD_PER_DAY: i64 = 12;

/// Renewals per day that `lease_duration_secs` implies is excessive.
///
/// Wardnet does not send DHCP options 58/59, so clients fall back to RFC 2131's
/// default `T1 = lease/2` (issue #1339). The threshold is derived from the
/// configured lease rather than hardcoded because a constant does not merely
/// lose precision when the lease changes — it inverts. At the default 86400s
/// lease a healthy client renews twice a day, but at a 3600s lease `T1` is 30
/// minutes and a healthy client renews 48 times a day, so a constant tuned for
/// the former would fire for *every* device on the latter.
fn threshold_per_day(lease_duration_secs: u32) -> i64 {
    let t1_secs = f64::from(lease_duration_secs) / 2.0;
    if t1_secs <= 0.0 {
        return MIN_THRESHOLD_PER_DAY;
    }
    let expected_per_day = 86_400.0 / t1_secs;
    #[expect(
        clippy::cast_possible_truncation,
        reason = "renewal counts are small integers; the ceiling of a rate is well inside i64"
    )]
    let threshold = (EXCESS_FACTOR * expected_per_day).ceil() as i64;
    threshold.max(MIN_THRESHOLD_PER_DAY)
}

/// Raises an anomaly for a DHCP client re-requesting its lease far more often
/// than its lease duration implies.
///
/// **Preventive**, for the reason the blocklist and DNS-upstream detectors are:
/// the condition is an aggregate over rows already written, so reading it has
/// no transition to miss. A client that has been stuck since before this
/// process started is reported just as readily as one that broke a minute ago —
/// which matters here, because the motivating fault ran unnoticed for months.
///
/// A client in this state is not renewing, it is retrying: it never registers
/// our reply, so it asks again at the protocol's retry cadence instead of at
/// `T1`. It keeps working — the lease is granted every time — which is exactly
/// why nothing surfaced it before.
pub struct DhcpRenewalStormDetector {
    dhcp: Arc<dyn DhcpService>,
}

impl DhcpRenewalStormDetector {
    #[must_use]
    pub fn new(dhcp: Arc<dyn DhcpService>) -> Self {
        Self { dhcp }
    }

    /// Renewals-per-day that currently counts as a storm, read fresh so a
    /// changed lease duration takes effect without a restart.
    async fn threshold(&self) -> anyhow::Result<i64> {
        let config = self.dhcp.get_dhcp_config().await?;
        Ok(threshold_per_day(config.lease_duration_secs))
    }

    fn report(mac: &str, renewals: i64, threshold: i64) -> AnomalyReport {
        AnomalyReport::new(
            AnomalyType::DhcpRenewalStorm,
            format!(
                "Device {mac} renewed its DHCP lease {renewals} times in the last \
                 {WINDOW_HOURS} hours"
            ),
        )
        // The MAC is the subject: one open anomaly per station, so a second
        // misbehaving client raises its own entry. Keyed by MAC rather than
        // device id because `dhcp_lease_log` is mac-keyed and a lease can
        // exist before any device row does.
        .with_subject(mac.to_owned())
        .with_details(serde_json::json!({
            "mac": mac,
            "renewals": renewals,
            "threshold": threshold,
            "window_hours": WINDOW_HOURS,
        }))
    }
}

#[async_trait]
impl AnomalyDetector for DhcpRenewalStormDetector {
    fn anomaly_type(&self) -> AnomalyType {
        AnomalyType::DhcpRenewalStorm
    }

    fn interval(&self) -> Option<Duration> {
        Some(SWEEP_INTERVAL)
    }

    async fn detect(&self) -> anyhow::Result<Vec<AnomalyReport>> {
        let threshold = self.threshold().await?;
        let since = Utc::now() - chrono::Duration::hours(WINDOW_HOURS);

        Ok(self
            .dhcp
            .renewal_counts_since(since)
            .await?
            .into_iter()
            .filter(|(_, renewals)| *renewals > threshold)
            .map(|(mac, renewals)| Self::report(&mac, renewals, threshold))
            .collect())
    }

    async fn reevaluate(&self, anomaly: &Anomaly) -> anyhow::Result<AnomalyStatus> {
        let Some(mac) = anomaly.subject_id.as_deref() else {
            // Nothing to recount. Close it rather than leaving an entry nobody
            // can act on.
            return Ok(AnomalyStatus::Resolved);
        };

        let threshold = self.threshold().await?;
        let since = Utc::now() - chrono::Duration::hours(WINDOW_HOURS);
        let renewals = self.dhcp.renewal_count_for_mac_since(mac, since).await?;

        if renewals > threshold {
            Ok(AnomalyStatus::Open)
        } else {
            Ok(AnomalyStatus::Resolved)
        }
    }
}
