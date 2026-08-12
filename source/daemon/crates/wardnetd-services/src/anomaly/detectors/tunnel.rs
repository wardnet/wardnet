use std::sync::Arc;

use async_trait::async_trait;
use uuid::Uuid;
use wardnet_common::anomaly::{Anomaly, AnomalyStatus, AnomalyType};
use wardnet_common::tunnel::TunnelStatus;

use crate::anomaly::detector::AnomalyDetector;
use crate::error::AppError;
use crate::tunnel::TunnelService;

/// What the tunnel behind an anomaly currently looks like.
enum SubjectTunnel {
    /// Deleted, or the anomaly's subject was never a tunnel id.
    Gone,
    Status(TunnelStatus),
}

async fn subject_tunnel(
    tunnels: &Arc<dyn TunnelService>,
    anomaly: &Anomaly,
) -> anyhow::Result<SubjectTunnel> {
    let Some(id) = anomaly
        .subject_id
        .as_deref()
        .and_then(|s| Uuid::parse_str(s).ok())
    else {
        return Ok(SubjectTunnel::Gone);
    };
    match tunnels.get_tunnel(id).await {
        Ok(tunnel) => Ok(SubjectTunnel::Status(tunnel.status)),
        Err(AppError::NotFound(_)) => Ok(SubjectTunnel::Gone),
        Err(e) => Err(anyhow::anyhow!(e)),
    }
}

/// A tunnel that could not be brought up.
///
/// Reactive only — every bring-up failure already funnels through one place
/// in `TunnelService` and is published as `TunnelStartFailed`, so there is
/// nothing to sweep for.
///
/// Resolves when the tunnel reaches `Up`, or when it is deleted. Note that a
/// tunnel left `Down` keeps the anomaly open: a bring-up that failed and was
/// never fixed is still a live problem, and "down" is exactly the state a
/// failed bring-up leaves behind.
pub struct TunnelStartFailedDetector {
    tunnels: Arc<dyn TunnelService>,
}

impl TunnelStartFailedDetector {
    #[must_use]
    pub fn new(tunnels: Arc<dyn TunnelService>) -> Self {
        Self { tunnels }
    }
}

#[async_trait]
impl AnomalyDetector for TunnelStartFailedDetector {
    fn anomaly_type(&self) -> AnomalyType {
        AnomalyType::TunnelStartFailed
    }

    async fn reevaluate(&self, anomaly: &Anomaly) -> anyhow::Result<AnomalyStatus> {
        Ok(match subject_tunnel(&self.tunnels, anomaly).await? {
            SubjectTunnel::Gone | SubjectTunnel::Status(TunnelStatus::Up) => {
                AnomalyStatus::Resolved
            }
            SubjectTunnel::Status(_) => AnomalyStatus::Open,
        })
    }
}

/// A tunnel that is configured and present but not carrying traffic — the
/// handshake has gone stale, or its kernel interface vanished.
///
/// Reactive only, fed by `TunnelReconnecting` and by the one `TunnelDown`
/// reason that is not a deliberate tear-down.
///
/// Resolves when the tunnel reaches `Up`, when it is deleted, **and** when it
/// reaches `Down`. The last case is the difference from
/// [`TunnelStartFailedDetector`]: this anomaly asserts "it should be working
/// and isn't", which stops being true the moment an admin deliberately stops
/// the tunnel.
pub struct TunnelUnhealthyDetector {
    tunnels: Arc<dyn TunnelService>,
}

impl TunnelUnhealthyDetector {
    #[must_use]
    pub fn new(tunnels: Arc<dyn TunnelService>) -> Self {
        Self { tunnels }
    }
}

#[async_trait]
impl AnomalyDetector for TunnelUnhealthyDetector {
    fn anomaly_type(&self) -> AnomalyType {
        AnomalyType::TunnelUnhealthy
    }

    async fn reevaluate(&self, anomaly: &Anomaly) -> anyhow::Result<AnomalyStatus> {
        Ok(match subject_tunnel(&self.tunnels, anomaly).await? {
            SubjectTunnel::Gone | SubjectTunnel::Status(TunnelStatus::Up | TunnelStatus::Down) => {
                AnomalyStatus::Resolved
            }
            SubjectTunnel::Status(TunnelStatus::Connecting | TunnelStatus::Reconnecting) => {
                AnomalyStatus::Open
            }
        })
    }
}
