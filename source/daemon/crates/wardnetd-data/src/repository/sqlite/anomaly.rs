//! SQLite-backed [`AnomalyRepository`] implementation.

use async_trait::async_trait;
use chrono::{NaiveDateTime, TimeZone, Utc};
use sqlx::SqlitePool;
use uuid::Uuid;
use wardnet_common::anomaly::{Anomaly, AnomalyFilter, AnomalyQueryStatus, AnomalyType};

use crate::db::DbPools;
use crate::repository::anomaly::{ANOMALY_RETENTION_CAP, AnomalyRepository, NewAnomaly};

const TS_FMT: &str = "%Y-%m-%dT%H:%M:%SZ";

/// The conflict target names the partial unique index deliberately, rather
/// than using a bare `INSERT OR IGNORE`: a duplicate *open* anomaly is the
/// expected, load-bearing case, but a primary-key collision would be a real
/// bug and should still surface as an error.
const INSERT_IF_NOT_OPEN: &str = "INSERT INTO anomalies \
     (id, anomaly_type, subject_id, message, details, opened_at, last_seen_at, occurrences) \
     VALUES (?, ?, ?, ?, ?, ?, ?, 1) \
     ON CONFLICT (anomaly_type, COALESCE(subject_id, '')) WHERE resolved_at IS NULL \
     DO NOTHING";

/// Retention applies to resolved rows only. Pruning an open anomaly would drop
/// the "already told them" state and silently re-arm its alert.
const PRUNE_RESOLVED: &str = "DELETE FROM anomalies \
     WHERE resolved_at IS NOT NULL AND id NOT IN \
     (SELECT id FROM anomalies WHERE resolved_at IS NOT NULL \
      ORDER BY resolved_at DESC, id DESC LIMIT ?)";

const TOUCH_OPEN: &str = "UPDATE anomalies \
     SET last_seen_at = ?, occurrences = occurrences + 1, message = ?, details = ? \
     WHERE anomaly_type = ? AND COALESCE(subject_id, '') = ? AND resolved_at IS NULL";

const RESOLVE_OPEN: &str =
    "UPDATE anomalies SET resolved_at = ? WHERE id = ? AND resolved_at IS NULL";

const MARK_NOTIFIED: &str = "UPDATE anomalies SET notified_at = ? WHERE id = ?";

const WAS_NOTIFIED: &str = "SELECT notified_at IS NOT NULL FROM anomalies WHERE id = ?";

const LIST_OPEN: &str = "SELECT id, anomaly_type, subject_id, message, details, \
     opened_at, last_seen_at, occurrences, resolved_at FROM anomalies \
     WHERE resolved_at IS NULL ORDER BY opened_at ASC, id ASC";

/// One statement covers all six filter combinations. Splitting it into a const
/// per combination would be six near-identical strings to keep in sync, and
/// the table is capped in the low hundreds of rows so the predicate cost is
/// irrelevant.
const LIST_FILTERED: &str = "SELECT id, anomaly_type, subject_id, message, details, \
     opened_at, last_seen_at, occurrences, resolved_at FROM anomalies \
     WHERE (?1 = 'all' \
            OR (?1 = 'open' AND resolved_at IS NULL) \
            OR (?1 = 'resolved' AND resolved_at IS NOT NULL)) \
       AND (?2 IS NULL OR subject_id = ?2) \
     ORDER BY opened_at DESC, id DESC LIMIT ?3";

const FIND_BY_ID: &str = "SELECT id, anomaly_type, subject_id, message, details, \
     opened_at, last_seen_at, occurrences, resolved_at FROM anomalies WHERE id = ?";

/// Render a timestamp in the storage format.
///
/// Exported because the service layer stamps `observed_at` / `resolved_at`
/// itself; keeping the format string in one place is what stops a write that
/// [`parse_ts`] then cannot read back.
#[must_use]
pub fn format_ts(ts: chrono::DateTime<Utc>) -> String {
    ts.format(TS_FMT).to_string()
}

fn parse_ts(s: &str) -> anyhow::Result<chrono::DateTime<Utc>> {
    let naive = NaiveDateTime::parse_from_str(s, TS_FMT)?;
    Ok(Utc.from_utc_datetime(&naive))
}

fn parse_ts_opt(s: Option<&String>) -> anyhow::Result<Option<chrono::DateTime<Utc>>> {
    match s {
        Some(v) => Ok(Some(parse_ts(v)?)),
        None => Ok(None),
    }
}

fn status_param(status: AnomalyQueryStatus) -> &'static str {
    match status {
        AnomalyQueryStatus::Open => "open",
        AnomalyQueryStatus::Resolved => "resolved",
        AnomalyQueryStatus::All => "all",
    }
}

#[derive(sqlx::FromRow)]
struct DbAnomalyRow {
    id: String,
    anomaly_type: String,
    subject_id: Option<String>,
    message: String,
    details: Option<String>,
    opened_at: String,
    last_seen_at: String,
    occurrences: i64,
    resolved_at: Option<String>,
}

impl DbAnomalyRow {
    /// Convert to the domain type, or `None` when the row is unreadable.
    ///
    /// An unknown `anomaly_type` is the realistic case: a row written by a
    /// newer daemon and read after a downgrade. Malformed `details` is the
    /// other. Neither should take down a whole listing, so both drop the row
    /// (or the field) with a warning rather than propagating an error.
    fn into_domain(self) -> Option<Anomaly> {
        let Some(anomaly_type) = AnomalyType::from_slug(&self.anomaly_type) else {
            tracing::warn!(
                anomaly_type = %self.anomaly_type,
                "anomaly: skipping row with unknown type: anomaly_type={}",
                self.anomaly_type
            );
            return None;
        };
        let id = match Uuid::parse_str(&self.id) {
            Ok(id) => id,
            Err(error) => {
                tracing::warn!(%error, id = %self.id, "anomaly: skipping row with unparseable id={id}", id = self.id);
                return None;
            }
        };

        let details = self.details.as_deref().and_then(|raw| {
            serde_json::from_str(raw)
                .map_err(|error| {
                    tracing::warn!(%error, %id, "anomaly: dropping unparseable details for id={id}");
                })
                .ok()
        });

        let (Ok(opened_at), Ok(last_seen_at), Ok(resolved_at)) = (
            parse_ts(&self.opened_at),
            parse_ts(&self.last_seen_at),
            parse_ts_opt(self.resolved_at.as_ref()),
        ) else {
            tracing::warn!(%id, "anomaly: skipping row with unparseable timestamps for id={id}");
            return None;
        };

        Some(Anomaly {
            id,
            anomaly_type,
            subject_id: self.subject_id,
            message: self.message,
            details,
            opened_at,
            last_seen_at,
            occurrences: u32::try_from(self.occurrences).unwrap_or(u32::MAX),
            resolved_at,
        })
    }
}

pub struct SqliteAnomalyRepository {
    pools: DbPools,
}

impl SqliteAnomalyRepository {
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self::new_pools(DbPools::single(pool))
    }

    #[must_use]
    pub fn new_pools(pools: DbPools) -> Self {
        Self { pools }
    }
}

#[async_trait]
impl AnomalyRepository for SqliteAnomalyRepository {
    async fn open(&self, anomaly: NewAnomaly<'_>) -> anyhow::Result<Option<Uuid>> {
        let inserted = sqlx::query(INSERT_IF_NOT_OPEN)
            .bind(anomaly.id.to_string())
            .bind(anomaly.anomaly_type)
            .bind(anomaly.subject_id)
            .bind(anomaly.message)
            .bind(anomaly.details)
            .bind(anomaly.observed_at)
            .bind(anomaly.observed_at)
            .execute(&self.pools.write)
            .await?
            .rows_affected();

        if inserted == 0 {
            return Ok(None);
        }

        sqlx::query(PRUNE_RESOLVED)
            .bind(ANOMALY_RETENTION_CAP)
            .execute(&self.pools.write)
            .await?;

        Ok(Some(anomaly.id))
    }

    async fn touch(
        &self,
        anomaly_type: &str,
        subject_id: Option<&str>,
        seen_at: &str,
        message: &str,
        details: Option<&str>,
    ) -> anyhow::Result<()> {
        sqlx::query(TOUCH_OPEN)
            .bind(seen_at)
            .bind(message)
            .bind(details)
            .bind(anomaly_type)
            .bind(subject_id.unwrap_or(""))
            .execute(&self.pools.write)
            .await?;
        Ok(())
    }

    async fn resolve(&self, id: Uuid, resolved_at: &str) -> anyhow::Result<bool> {
        let affected = sqlx::query(RESOLVE_OPEN)
            .bind(resolved_at)
            .bind(id.to_string())
            .execute(&self.pools.write)
            .await?
            .rows_affected();
        Ok(affected > 0)
    }

    async fn mark_notified(&self, id: Uuid, at: &str) -> anyhow::Result<()> {
        sqlx::query(MARK_NOTIFIED)
            .bind(at)
            .bind(id.to_string())
            .execute(&self.pools.write)
            .await?;
        Ok(())
    }

    async fn was_notified(&self, id: Uuid) -> anyhow::Result<bool> {
        let notified: Option<i64> = sqlx::query_scalar(WAS_NOTIFIED)
            .bind(id.to_string())
            .fetch_optional(&self.pools.read)
            .await?;
        Ok(notified.unwrap_or(0) != 0)
    }

    async fn list_open(&self) -> anyhow::Result<Vec<Anomaly>> {
        let rows = sqlx::query_as::<_, DbAnomalyRow>(LIST_OPEN)
            .fetch_all(&self.pools.read)
            .await?;
        Ok(rows
            .into_iter()
            .filter_map(DbAnomalyRow::into_domain)
            .collect())
    }

    async fn list(&self, filter: &AnomalyFilter, limit: u32) -> anyhow::Result<Vec<Anomaly>> {
        let rows = sqlx::query_as::<_, DbAnomalyRow>(LIST_FILTERED)
            .bind(status_param(filter.status))
            .bind(filter.subject_id.as_deref())
            .bind(limit)
            .fetch_all(&self.pools.read)
            .await?;
        Ok(rows
            .into_iter()
            .filter_map(DbAnomalyRow::into_domain)
            .collect())
    }

    async fn find_by_id(&self, id: Uuid) -> anyhow::Result<Option<Anomaly>> {
        let row = sqlx::query_as::<_, DbAnomalyRow>(FIND_BY_ID)
            .bind(id.to_string())
            .fetch_optional(&self.pools.read)
            .await?;
        Ok(row.and_then(DbAnomalyRow::into_domain))
    }
}
