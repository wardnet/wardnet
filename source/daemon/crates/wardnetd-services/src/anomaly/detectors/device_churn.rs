use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use wardnet_common::anomaly::{Anomaly, AnomalyReport, AnomalyStatus, AnomalyType};
use wardnet_common::device_event::DeviceEventKind;

use crate::anomaly::detector::AnomalyDetector;
use crate::device_event::DeviceEventService;

/// How often the sweep recounts address changes.
const SWEEP_INTERVAL: Duration = Duration::from_mins(10);

/// How far back address changes are counted.
const WINDOW_HOURS: i64 = 24;

/// How many address changes in [`WINDOW_HOURS`] stop looking like a device
/// moving.
///
/// Measured over a day on a live 55-device box, the distribution is cleanly
/// bimodal: four addresses above 238 changes/day, seventeen at or below 20, and
/// *nothing at all* between 21 and 100. The threshold sits in that empty band,
/// so it is robust rather than tuned — 2.5× above the noisiest honest device
/// and ~5× below the quietest offender.
///
/// Deliberately not derived from the DHCP lease, unlike the renewal-storm
/// threshold: discovery observes ARP, not just leases, so tying this to the
/// lease duration would invent a relationship that does not exist.
const MAX_CHANGES_PER_WINDOW: i64 = 50;

/// Raises an anomaly for a MAC changing address far more often than a real
/// device does.
///
/// **Preventive**: the condition is an aggregate over the timeline, so a
/// station that has been churning since before this process started is reported
/// just as readily as one that started a minute ago.
///
/// Distinct from the in-memory flap guard in device discovery, and deliberately
/// kept that way. That guard is *protective*: it counts **distinct** addresses
/// inside a 60-second window and stops trusting a MAC that claims more than a
/// handful, because something answering ARP for addresses it does not own is
/// lying about all of them. This is *diagnostic*: it counts **transitions** over
/// a day and tells the admin. A MAC ping-ponging between exactly two addresses
/// hundreds of times a day is invisible to the guard — two distinct addresses
/// never exceeds its limit — and is exactly what this catches.
pub struct DeviceAddressChurnDetector {
    events: Arc<dyn DeviceEventService>,
}

impl DeviceAddressChurnDetector {
    #[must_use]
    pub fn new(events: Arc<dyn DeviceEventService>) -> Self {
        Self { events }
    }

    fn report(mac: &str, changes: i64) -> AnomalyReport {
        AnomalyReport::new(
            AnomalyType::DeviceAddressChurn,
            format!(
                "Device {mac} changed address {changes} times in the last {WINDOW_HOURS} hours"
            ),
        )
        // One open anomaly per station, so a second churning MAC raises its own
        // entry rather than being folded into the first one's.
        .with_subject(mac.to_owned())
        .with_details(serde_json::json!({
            "mac": mac,
            "changes": changes,
            "threshold": MAX_CHANGES_PER_WINDOW,
            "window_hours": WINDOW_HOURS,
        }))
    }
}

#[async_trait]
impl AnomalyDetector for DeviceAddressChurnDetector {
    fn anomaly_type(&self) -> AnomalyType {
        AnomalyType::DeviceAddressChurn
    }

    fn interval(&self) -> Option<Duration> {
        Some(SWEEP_INTERVAL)
    }

    async fn detect(&self) -> anyhow::Result<Vec<AnomalyReport>> {
        let since = Utc::now() - chrono::Duration::hours(WINDOW_HOURS);

        Ok(self
            .events
            .count_by_mac_since(DeviceEventKind::IpChanged, since)
            .await?
            .into_iter()
            .filter(|(_, changes)| *changes > MAX_CHANGES_PER_WINDOW)
            .map(|(mac, changes)| Self::report(&mac, changes))
            .collect())
    }

    async fn reevaluate(&self, anomaly: &Anomaly) -> anyhow::Result<AnomalyStatus> {
        let Some(mac) = anomaly.subject_id.as_deref() else {
            return Ok(AnomalyStatus::Resolved);
        };

        let since = Utc::now() - chrono::Duration::hours(WINDOW_HOURS);
        let changes = self
            .events
            .count_for_mac_since(mac, DeviceEventKind::IpChanged, since)
            .await?;

        if changes > MAX_CHANGES_PER_WINDOW {
            Ok(AnomalyStatus::Open)
        } else {
            Ok(AnomalyStatus::Resolved)
        }
    }
}
