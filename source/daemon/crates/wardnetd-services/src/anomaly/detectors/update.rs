use async_trait::async_trait;
use wardnet_common::anomaly::{Anomaly, AnomalyStatus, AnomalyType};

use crate::anomaly::detector::AnomalyDetector;
use crate::update::service::is_newer;

/// A daemon self-update that failed.
///
/// Reactive only — `UpdateFailed` is published from the update service's
/// failure paths, and there is no ambient state to sweep for.
///
/// Resolves once the running release has reached the version the failed
/// attempt was targeting, catching both the eventual success of the same
/// upgrade and a later one that overtook it.
///
/// The comparison goes through [`is_newer`], **not** a string compare. Plain
/// `CalVer` does sort lexicographically, but releases carry `-beta.N` and
/// `-edge.N` pre-release suffixes (see `build-daemon.yml`'s
/// `WARDNET_RELEASE_VERSION_OVERRIDE`), and those do not: `"…edge.9"` sorts
/// above `"…edge.10"`, so a box running `edge.9` would treat a failed update
/// to `edge.10` as already-applied, discard the alert, and push a bogus
/// "Problem resolved".
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

        // "the box has caught up" is the negation of "the target is still
        // ahead of us", which is exactly what `is_newer` answers.
        Ok(if is_newer(&target, &self.running_version) {
            AnomalyStatus::Open
        } else {
            AnomalyStatus::Resolved
        })
    }
}
