use async_trait::async_trait;
use uuid::Uuid;
use wardnet_common::anomaly::{Anomaly, AnomalyFilter};

/// How many *resolved* anomalies the history retains. Enforced inside
/// [`AnomalyRepository::open`] (delete-oldest-beyond-cap), so no background
/// cleanup job is needed — anomaly volume is a handful per day.
///
/// Open anomalies are never pruned regardless of this cap: dropping one would
/// silently re-arm its alert, which is the exact failure this subsystem exists
/// to prevent.
pub const ANOMALY_RETENTION_CAP: u32 = 200;

/// Fields needed to open a new anomaly.
#[derive(Debug, Clone)]
pub struct NewAnomaly<'a> {
    pub id: Uuid,
    /// [`wardnet_common::anomaly::AnomalyType::as_str`].
    pub anomaly_type: &'a str,
    pub subject_id: Option<&'a str>,
    pub message: &'a str,
    /// Serialised JSON, or `None`.
    pub details: Option<&'a str>,
    /// RFC 3339. Seeds both `opened_at` and `last_seen_at`.
    pub observed_at: &'a str,
}

/// Persistence for anomalies — typed admin-facing conditions with an
/// open/resolved lifecycle (issues #1097, #225).
#[async_trait]
pub trait AnomalyRepository: Send + Sync {
    /// Open an anomaly, unless one is already open for the same
    /// `(anomaly_type, subject_id)`.
    ///
    /// Returns `Some(id)` when a row was inserted and `None` when one was
    /// already open. That distinction is the edge-trigger: the caller notifies
    /// only on `Some`. Enforced by a partial unique index rather than a
    /// read-then-write, so it holds across restarts and concurrent detectors.
    ///
    /// Also prunes resolved rows beyond [`ANOMALY_RETENTION_CAP`].
    async fn open(&self, anomaly: NewAnomaly<'_>) -> anyhow::Result<Option<Uuid>>;

    /// Record another observation of an already-open anomaly: advance
    /// `last_seen_at`, bump `occurrences`, and refresh the message/details so
    /// a growing failure count stays current. No-op if none is open.
    async fn touch(
        &self,
        anomaly_type: &str,
        subject_id: Option<&str>,
        seen_at: &str,
        message: &str,
        details: Option<&str>,
    ) -> anyhow::Result<()>;

    /// Close an open anomaly. Returns whether a row changed — `false` means it
    /// was already resolved, which is how a caller avoids sending a duplicate
    /// recovery notice.
    async fn resolve(&self, id: Uuid, resolved_at: &str) -> anyhow::Result<bool>;

    /// Stamp `notified_at`, recording that the admin was told about this
    /// anomaly. Gates the matching recovery notice.
    async fn mark_notified(&self, id: Uuid, at: &str) -> anyhow::Result<()>;

    /// Whether this anomaly's open was ever notified.
    async fn was_notified(&self, id: Uuid) -> anyhow::Result<bool>;

    /// Every open anomaly, oldest first. Drives the reevaluation pass.
    async fn list_open(&self) -> anyhow::Result<Vec<Anomaly>>;

    /// Anomalies matching `filter`, newest first, at most `limit`.
    async fn list(&self, filter: &AnomalyFilter, limit: u32) -> anyhow::Result<Vec<Anomaly>>;

    /// A single anomaly by id, regardless of status.
    async fn find_by_id(&self, id: Uuid) -> anyhow::Result<Option<Anomaly>>;
}
