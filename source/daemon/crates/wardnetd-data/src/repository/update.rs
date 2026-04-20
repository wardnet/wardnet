use async_trait::async_trait;
use wardnet_common::update::{UpdateHistoryEntry, UpdateHistoryStatus};

/// Row struct for inserting a new update history entry.
#[derive(Debug, Clone)]
pub struct UpdateHistoryRow {
    pub from_version: String,
    pub to_version: String,
    pub phase: String,
    pub status: UpdateHistoryStatus,
    pub error: Option<String>,
}

/// Repository trait for persistent update history.
///
/// The auto-update subsystem writes one row per install attempt. Runtime
/// state (pending version, channel, last check) is stored in `system_config`
/// via [`SystemConfigRepository`](super::SystemConfigRepository).
#[async_trait]
pub trait UpdateRepository: Send + Sync {
    /// Insert a new history row. Returns the auto-increment primary key so
    /// callers can later update the row's terminal status/phase.
    async fn insert(&self, row: &UpdateHistoryRow) -> anyhow::Result<i64>;

    /// Mark an existing row as finished with the given status/phase.
    async fn finalize(
        &self,
        id: i64,
        status: UpdateHistoryStatus,
        phase: &str,
        error: Option<&str>,
    ) -> anyhow::Result<()>;

    /// Return the most recent `limit` history entries, newest first.
    async fn list(&self, limit: u32) -> anyhow::Result<Vec<UpdateHistoryEntry>>;

    /// Return the most recent entry with status = 'succeeded', if any.
    async fn last_succeeded(&self) -> anyhow::Result<Option<UpdateHistoryEntry>>;
}
