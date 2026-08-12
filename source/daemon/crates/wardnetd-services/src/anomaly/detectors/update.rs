use async_trait::async_trait;
use wardnet_common::anomaly::{Anomaly, AnomalyStatus, AnomalyType};

use crate::anomaly::detector::AnomalyDetector;

/// A daemon self-update that failed.
///
/// Reactive only — `UpdateFailed` is published from the update service's
/// failure paths, and there is no ambient state to sweep for.
///
/// Resolves once the running release has reached the version the failed
/// attempt was targeting. Comparing the `CalVer` strings directly is valid
/// because the format (`YYYY.MM.NN`, zero-padded) sorts lexicographically in
/// release order, and it catches both the eventual success of the same
/// upgrade and a later one that overtook it.
pub struct UpdateFailedDetector {
    running_version: String,
}

impl UpdateFailedDetector {
    #[must_use]
    pub fn new(running_version: impl Into<String>) -> Self {
        Self {
            running_version: running_version.into(),
        }
    }
}

#[async_trait]
impl AnomalyDetector for UpdateFailedDetector {
    fn anomaly_type(&self) -> AnomalyType {
        AnomalyType::UpdateFailed
    }

    async fn reevaluate(&self, anomaly: &Anomaly) -> anyhow::Result<AnomalyStatus> {
        let target = anomaly
            .details
            .as_ref()
            .and_then(|d| d["target_version"].as_str().map(str::to_owned));

        // Without a recorded target there is nothing to compare against, and a
        // running daemon is evidence enough that the box is not stuck.
        let Some(target) = target else {
            return Ok(AnomalyStatus::Resolved);
        };

        Ok(if self.running_version >= target {
            AnomalyStatus::Resolved
        } else {
            AnomalyStatus::Open
        })
    }
}
