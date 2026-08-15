use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use uuid::Uuid;
use wardnet_common::anomaly::{
    Anomaly, AnomalyFilter, AnomalyQueryStatus, AnomalyReport, AnomalyStatus, AnomalyType,
    ReevaluateSummary,
};
use wardnetd_data::repository::sqlite::format_ts;
use wardnetd_data::repository::{AnomalyRepository, NewAnomaly};

use crate::anomaly::registry::AnomalyDetectorRegistry;
use crate::auth_context;
use crate::error::AppError;
use crate::push::PushService;

/// Ceiling on a single detector call, so one hung detector cannot stall the
/// whole pass. A timeout is treated as "no verdict": the anomaly stays open.
const DEFAULT_DETECTOR_TIMEOUT: Duration = Duration::from_secs(30);

/// Owns the anomaly lifecycle: opening, deduplicating, notifying, resolving.
///
/// Every method is admin-gated. Background callers (the engine, the listener)
/// run under a nil-admin context, the same way every other runner does.
#[async_trait]
pub trait AnomalyService: Send + Sync {
    /// Anomalies matching `filter`, newest first.
    async fn list(&self, filter: AnomalyFilter, limit: u32) -> Result<Vec<Anomaly>, AppError>;

    /// Record an observation.
    ///
    /// Idempotent per `(type, subject_id)`: the first observation opens an
    /// anomaly and notifies the admins; every later one refreshes the existing
    /// entry and notifies nobody. This is the single choke point that makes
    /// alerting edge-triggered, and it is why callers can submit freely on
    /// every tick without thinking about it.
    async fn submit(&self, report: AnomalyReport) -> Result<(), AppError>;

    /// Close an anomaly, notifying the admins if its open was notified.
    async fn resolve(&self, id: Uuid) -> Result<(), AppError>;

    /// Resolve every open anomaly of `anomaly_type` about `subject_id`.
    ///
    /// The event-driven counterpart to [`Self::resolve`]. Some conditions stop
    /// holding because of an explicit admin action rather than because a
    /// detector noticed a state change — and for those the *action* is the
    /// only unambiguous signal. Stopping a tunnel is the motivating case: the
    /// resulting `TunnelStatus::Down` is indistinguishable from a broken
    /// tunnel, so only the tear-down event itself can say an admin meant it.
    async fn resolve_subject(
        &self,
        anomaly_type: AnomalyType,
        subject_id: &str,
    ) -> Result<(), AppError>;

    /// Run one detector's preventive sweep and submit whatever it reports.
    async fn run_detector(&self, anomaly_type: AnomalyType) -> Result<(), AppError>;

    /// Ask every open anomaly's detector whether it still holds, and resolve
    /// the ones that do not.
    async fn reevaluate_all(&self) -> Result<ReevaluateSummary, AppError>;

    /// Per-detector sweep cadences, for the engine's scheduler.
    fn schedule(&self) -> Vec<(AnomalyType, Duration)>;
}

pub struct AnomalyServiceImpl {
    repo: Arc<dyn AnomalyRepository>,
    registry: Arc<AnomalyDetectorRegistry>,
    push: Arc<dyn PushService>,
    detector_timeout: Duration,
}

impl AnomalyServiceImpl {
    #[must_use]
    pub fn new(
        repo: Arc<dyn AnomalyRepository>,
        registry: Arc<AnomalyDetectorRegistry>,
        push: Arc<dyn PushService>,
    ) -> Self {
        Self {
            repo,
            registry,
            push,
            detector_timeout: DEFAULT_DETECTOR_TIMEOUT,
        }
    }

    /// Override the per-detector timeout. Used by configuration and by tests.
    #[must_use]
    pub const fn with_detector_timeout(mut self, timeout: Duration) -> Self {
        self.detector_timeout = timeout;
        self
    }

    /// Has this anomaly been quiet for longer than its detector tolerates?
    ///
    /// Applied before `reevaluate` so a detector with no authoritative check
    /// never has to fake one.
    fn is_stale(anomaly: &Anomaly, stale_after: Option<Duration>) -> bool {
        let Some(stale_after) = stale_after else {
            return false;
        };
        let Ok(window) = chrono::Duration::from_std(stale_after) else {
            return false;
        };
        Utc::now() - anomaly.last_seen_at > window
    }

    /// Ask a detector whether an anomaly still holds, bounded by the timeout.
    ///
    /// Both a timeout and an error mean "no verdict", which resolves to
    /// leaving the anomaly open: silently closing a problem we failed to
    /// check would be the worst outcome available.
    async fn verdict(&self, anomaly: &Anomaly) -> AnomalyStatus {
        let Some(detector) = self.registry.get(anomaly.anomaly_type) else {
            // Disabled or unregistered: leave its anomalies alone rather than
            // force-closing entries nothing understands any more.
            return AnomalyStatus::Open;
        };

        if Self::is_stale(anomaly, detector.stale_after()) {
            return AnomalyStatus::Resolved;
        }

        match tokio::time::timeout(self.detector_timeout, detector.reevaluate(anomaly)).await {
            Ok(Ok(status)) => status,
            Ok(Err(error)) => {
                tracing::warn!(
                    %error,
                    anomaly_id = %anomaly.id,
                    anomaly_type = %anomaly.anomaly_type.as_str(),
                    "anomaly: reevaluate failed, leaving open: type={type}",
                    r#type = anomaly.anomaly_type.as_str(),
                );
                AnomalyStatus::Open
            }
            Err(_) => {
                tracing::warn!(
                    anomaly_id = %anomaly.id,
                    anomaly_type = %anomaly.anomaly_type.as_str(),
                    "anomaly: reevaluate timed out, leaving open: type={type}",
                    r#type = anomaly.anomaly_type.as_str(),
                );
                AnomalyStatus::Open
            }
        }
    }

    /// Deliver the open notification and record that we did.
    ///
    /// A delivery failure is logged, not propagated: the anomaly is already
    /// persisted and visible on the dashboard, and failing the submit would
    /// lose it entirely. `notified_at` is only stamped on success, so the
    /// recovery notice stays correctly gated.
    async fn notify_opened(&self, anomaly: &Anomaly) {
        if let Err(error) = self.push.notify_anomaly_opened(anomaly).await {
            tracing::warn!(%error, anomaly_id = %anomaly.id, "anomaly: failed to notify admins of a new anomaly");
            return;
        }
        if let Err(error) = self
            .repo
            .mark_notified(anomaly.id, &format_ts(Utc::now()))
            .await
        {
            tracing::warn!(%error, anomaly_id = %anomaly.id, "anomaly: failed to record notified state");
        }
    }
}

#[async_trait]
impl AnomalyService for AnomalyServiceImpl {
    async fn list(&self, filter: AnomalyFilter, limit: u32) -> Result<Vec<Anomaly>, AppError> {
        auth_context::require_admin()?;
        self.repo
            .list(&filter, limit.clamp(1, 200))
            .await
            .map_err(AppError::Internal)
    }

    async fn submit(&self, report: AnomalyReport) -> Result<(), AppError> {
        auth_context::require_admin()?;

        let details = report
            .details
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .map_err(|e| AppError::Internal(e.into()))?;
        let observed_at = format_ts(report.observed_at);
        let id = Uuid::new_v4();

        let opened = self
            .repo
            .open(NewAnomaly {
                id,
                anomaly_type: report.anomaly_type.as_str(),
                subject_id: report.subject_id.as_deref(),
                message: &report.message,
                details: details.as_deref(),
                observed_at: &observed_at,
            })
            .await
            .map_err(AppError::Internal)?;

        let Some(id) = opened else {
            // Already open: refresh what it says and stay quiet. This is the
            // path a long-running failure takes on every single tick.
            self.repo
                .touch(
                    report.anomaly_type.as_str(),
                    report.subject_id.as_deref(),
                    &observed_at,
                    &report.message,
                    details.as_deref(),
                )
                .await
                .map_err(AppError::Internal)?;
            return Ok(());
        };

        tracing::warn!(
            anomaly_id = %id,
            anomaly_type = %report.anomaly_type.as_str(),
            subject_id = report.subject_id.as_deref().unwrap_or("-"),
            "anomaly opened: type={type}, subject={subject}, message={message}",
            r#type = report.anomaly_type.as_str(),
            subject = report.subject_id.as_deref().unwrap_or("-"),
            message = report.message,
        );

        if let Some(anomaly) = self.repo.find_by_id(id).await.map_err(AppError::Internal)? {
            self.notify_opened(&anomaly).await;
        }
        Ok(())
    }

    async fn resolve(&self, id: Uuid) -> Result<(), AppError> {
        auth_context::require_admin()?;

        // Read before closing: the notification wants the anomaly's message,
        // and `was_notified` has to be read while the row is still intact.
        let Some(anomaly) = self.repo.find_by_id(id).await.map_err(AppError::Internal)? else {
            return Err(AppError::NotFound(format!("anomaly '{id}' not found")));
        };
        let was_notified = self
            .repo
            .was_notified(id)
            .await
            .map_err(AppError::Internal)?;

        let changed = self
            .repo
            .resolve(id, &format_ts(Utc::now()))
            .await
            .map_err(AppError::Internal)?;
        if !changed {
            // Already resolved — someone beat us to it. Staying silent here is
            // what stops a duplicate recovery notice.
            return Ok(());
        }

        tracing::info!(
            anomaly_id = %id,
            anomaly_type = %anomaly.anomaly_type.as_str(),
            "anomaly resolved: type={type}",
            r#type = anomaly.anomaly_type.as_str(),
        );

        // "It is working again" must never arrive without its "it is broken".
        if was_notified && let Err(error) = self.push.notify_anomaly_resolved(&anomaly).await {
            tracing::warn!(%error, anomaly_id = %id, "anomaly: failed to notify admins of a resolved anomaly");
        }
        Ok(())
    }

    async fn resolve_subject(
        &self,
        anomaly_type: AnomalyType,
        subject_id: &str,
    ) -> Result<(), AppError> {
        auth_context::require_admin()?;

        // The partial unique index allows at most one *open* anomaly per
        // (type, subject), so this is a one-element loop in practice — but
        // filtering by type here rather than assuming that keeps it honest if
        // the index ever changes.
        let open = self
            .repo
            .list(
                &AnomalyFilter {
                    status: AnomalyQueryStatus::Open,
                    subject_id: Some(subject_id.to_owned()),
                },
                200,
            )
            .await
            .map_err(AppError::Internal)?;

        for anomaly in open.into_iter().filter(|a| a.anomaly_type == anomaly_type) {
            // Reuse `resolve` so the recovery notification stays gated on the
            // open having been notified.
            self.resolve(anomaly.id).await?;
        }
        Ok(())
    }

    async fn run_detector(&self, anomaly_type: AnomalyType) -> Result<(), AppError> {
        auth_context::require_admin()?;

        let Some(detector) = self.registry.get(anomaly_type) else {
            return Err(AppError::NotFound(format!(
                "no detector registered for anomaly type '{}'",
                anomaly_type.as_str()
            )));
        };

        let reports = match tokio::time::timeout(self.detector_timeout, detector.detect()).await {
            Ok(Ok(reports)) => reports,
            Ok(Err(error)) => {
                tracing::warn!(
                    %error,
                    anomaly_type = %anomaly_type.as_str(),
                    "anomaly: detector sweep failed: type={type}",
                    r#type = anomaly_type.as_str(),
                );
                return Ok(());
            }
            Err(_) => {
                tracing::warn!(
                    anomaly_type = %anomaly_type.as_str(),
                    "anomaly: detector sweep timed out: type={type}",
                    r#type = anomaly_type.as_str(),
                );
                return Ok(());
            }
        };

        for report in reports {
            if let Err(error) = self.submit(report).await {
                tracing::warn!(%error, anomaly_type = %anomaly_type.as_str(), "anomaly: failed to submit a swept report");
            }
        }
        Ok(())
    }

    async fn reevaluate_all(&self) -> Result<ReevaluateSummary, AppError> {
        auth_context::require_admin()?;

        let open = self.repo.list_open().await.map_err(AppError::Internal)?;
        let mut summary = ReevaluateSummary {
            evaluated: u32::try_from(open.len()).unwrap_or(u32::MAX),
            resolved: 0,
        };

        for anomaly in open {
            if self.verdict(&anomaly).await == AnomalyStatus::Resolved {
                match self.resolve(anomaly.id).await {
                    Ok(()) => summary.resolved += 1,
                    Err(error) => {
                        tracing::warn!(%error, anomaly_id = %anomaly.id, "anomaly: failed to resolve");
                    }
                }
            }
        }
        Ok(summary)
    }

    fn schedule(&self) -> Vec<(AnomalyType, Duration)> {
        self.registry.schedule()
    }
}
