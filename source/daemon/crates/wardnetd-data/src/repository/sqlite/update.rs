use async_trait::async_trait;
use chrono::Utc;
use sqlx::SqlitePool;
use wardnet_common::update::{UpdateHistoryEntry, UpdateHistoryStatus};

use super::super::update::{UpdateHistoryRow, UpdateRepository};

/// SQLite-backed implementation of [`UpdateRepository`].
pub struct SqliteUpdateRepository {
    pool: SqlitePool,
}

impl SqliteUpdateRepository {
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[derive(sqlx::FromRow)]
struct DbRow {
    id: i64,
    from_version: String,
    to_version: String,
    phase: String,
    status: String,
    error: Option<String>,
    started_at: String,
    finished_at: Option<String>,
}

impl DbRow {
    fn into_entry(self) -> anyhow::Result<UpdateHistoryEntry> {
        let status = UpdateHistoryStatus::from_str(&self.status)
            .ok_or_else(|| anyhow::anyhow!("unknown update history status: {}", self.status))?;
        let finished_at = self
            .finished_at
            .filter(|s| !s.is_empty())
            .map(|s| s.parse())
            .transpose()?;
        Ok(UpdateHistoryEntry {
            id: self.id,
            from_version: self.from_version,
            to_version: self.to_version,
            phase: self.phase,
            status,
            error: self.error,
            started_at: self.started_at.parse()?,
            finished_at,
        })
    }
}

#[async_trait]
impl UpdateRepository for SqliteUpdateRepository {
    async fn insert(&self, row: &UpdateHistoryRow) -> anyhow::Result<i64> {
        let result = sqlx::query(
            "INSERT INTO update_history (from_version, to_version, phase, status, error) \
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&row.from_version)
        .bind(&row.to_version)
        .bind(&row.phase)
        .bind(row.status.as_str())
        .bind(&row.error)
        .execute(&self.pool)
        .await?;
        Ok(result.last_insert_rowid())
    }

    async fn finalize(
        &self,
        id: i64,
        status: UpdateHistoryStatus,
        phase: &str,
        error: Option<&str>,
    ) -> anyhow::Result<()> {
        let now = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        sqlx::query(
            "UPDATE update_history SET status = ?, phase = ?, error = ?, finished_at = ? \
             WHERE id = ?",
        )
        .bind(status.as_str())
        .bind(phase)
        .bind(error)
        .bind(&now)
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn list(&self, limit: u32) -> anyhow::Result<Vec<UpdateHistoryEntry>> {
        let rows = sqlx::query_as::<_, DbRow>(
            "SELECT id, from_version, to_version, phase, status, error, started_at, finished_at \
             FROM update_history ORDER BY started_at DESC LIMIT ?",
        )
        .bind(i64::from(limit))
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(DbRow::into_entry).collect()
    }

    async fn last_succeeded(&self) -> anyhow::Result<Option<UpdateHistoryEntry>> {
        let row = sqlx::query_as::<_, DbRow>(
            "SELECT id, from_version, to_version, phase, status, error, started_at, finished_at \
             FROM update_history WHERE status = 'succeeded' ORDER BY started_at DESC LIMIT 1",
        )
        .fetch_optional(&self.pool)
        .await?;
        row.map(DbRow::into_entry).transpose()
    }
}
