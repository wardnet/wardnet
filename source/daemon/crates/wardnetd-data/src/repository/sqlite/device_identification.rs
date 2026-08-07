use async_trait::async_trait;
use sqlx::SqlitePool;
use wardnet_common::device::{DeviceSignal, DeviceSignalKind, ManufacturerSource};

use super::super::DeviceIdentificationRepository;
use super::super::device_identification::DeviceSignalRow;
use crate::db::DbPools;

/// SQLite-backed implementation of [`DeviceIdentificationRepository`].
pub struct SqliteDeviceIdentificationRepository {
    pools: DbPools,
}

impl SqliteDeviceIdentificationRepository {
    /// Create a new repository backed by the given connection pool.
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self::new_pools(DbPools::single(pool))
    }

    /// Create a new repository with split reader/writer pools.
    #[must_use]
    pub fn new_pools(pools: DbPools) -> Self {
        Self { pools }
    }
}

/// Raw row from the `device_signals` table used for internal mapping.
#[derive(sqlx::FromRow)]
struct SignalRow {
    kind: String,
    value: String,
    confidence: String,
    observed_at: String,
}

/// The `confidence` value [`DeviceIdentificationRepository::record`] writes for
/// a signal that resolved to a vendor through the curated catalog.
const CONFIDENCE_INFERRED: &str = "inferred";

/// The `confidence` value written for a signal recorded exactly as observed,
/// with no catalog match behind it.
const CONFIDENCE_OBSERVED: &str = "observed";

/// Serialize a [`DeviceSignalKind`] to its stored `snake_case` string form
/// (mirrors how `device_type` and `connection_mode` are persisted).
fn kind_to_db(kind: DeviceSignalKind) -> String {
    serde_json::to_string(&kind)
        .map_or_else(|_| "unknown".to_owned(), |s| s.trim_matches('"').to_owned())
}

/// Parse a stored `kind` back into the enum, or `None` for a value this build
/// does not recognise — a forward-compatibility case if a newer daemon wrote a
/// kind we have since rolled back past.
fn kind_from_db(raw: &str) -> Option<DeviceSignalKind> {
    serde_json::from_str(&format!("\"{raw}\"")).ok()
}

#[async_trait]
impl DeviceIdentificationRepository for SqliteDeviceIdentificationRepository {
    async fn record(&self, row: &DeviceSignalRow) -> anyhow::Result<()> {
        // Upsert on the natural key so a device re-announcing an unchanged
        // fact refreshes `observed_at` instead of piling up duplicate rows.
        sqlx::query(
            "INSERT INTO device_signals (device_id, kind, value, confidence, observed_at) \
             VALUES (?, ?, ?, ?, strftime('%Y-%m-%dT%H:%M:%SZ', 'now')) \
             ON CONFLICT(device_id, kind, value) DO UPDATE SET \
               confidence = excluded.confidence, observed_at = excluded.observed_at",
        )
        .bind(&row.device_id)
        .bind(kind_to_db(row.kind))
        .bind(&row.value)
        .bind(if row.inferred {
            CONFIDENCE_INFERRED
        } else {
            CONFIDENCE_OBSERVED
        })
        .execute(&self.pools.write)
        .await?;
        Ok(())
    }

    async fn set_manufacturer_if_absent(
        &self,
        device_id: &str,
        manufacturer: &str,
        source: ManufacturerSource,
    ) -> anyhow::Result<bool> {
        // `manufacturer IS NULL` is the whole guard: it makes first-writer-wins
        // atomic, and it makes a missing device a no-op rather than a write
        // against an id that does not exist.
        let result = sqlx::query(
            "UPDATE devices SET manufacturer = ?, manufacturer_source = ? \
             WHERE id = ? AND manufacturer IS NULL",
        )
        .bind(manufacturer)
        .bind(
            serde_json::to_string(&source)
                .map(|s| s.trim_matches('"').to_owned())
                .unwrap_or_default(),
        )
        .bind(device_id)
        .execute(&self.pools.write)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn prune_signals(
        &self,
        device_id: &str,
        kind: DeviceSignalKind,
        keep: usize,
    ) -> anyhow::Result<()> {
        // Delete everything outside the newest `keep` for this (device, kind).
        // `rowid` breaks ties so two rows sharing a second-resolution
        // `observed_at` still order deterministically — otherwise the LIMIT
        // could keep and delete the same row across calls.
        sqlx::query(
            "DELETE FROM device_signals \
             WHERE device_id = ? AND kind = ? AND value NOT IN ( \
               SELECT value FROM device_signals \
               WHERE device_id = ? AND kind = ? \
               ORDER BY observed_at DESC, rowid DESC LIMIT ? \
             )",
        )
        .bind(device_id)
        .bind(kind_to_db(kind))
        .bind(device_id)
        .bind(kind_to_db(kind))
        .bind(i64::try_from(keep).unwrap_or(i64::MAX))
        .execute(&self.pools.write)
        .await?;
        Ok(())
    }

    async fn find_by_device(&self, device_id: &str) -> anyhow::Result<Vec<DeviceSignal>> {
        let rows = sqlx::query_as::<_, SignalRow>(
            // `rowid DESC` breaks ties for the same reason `prune_signals`
            // does: `observed_at` has one-second resolution, so a device
            // announcing several services at once produces identical
            // timestamps, and "most recent first" would otherwise be a
            // different order on every read.
            "SELECT kind, value, confidence, observed_at FROM device_signals \
             WHERE device_id = ? ORDER BY observed_at DESC, rowid DESC",
        )
        .bind(device_id)
        .fetch_all(&self.pools.read)
        .await?;

        // Skip unrecognised kinds rather than failing the whole read: one
        // unknown signal must not make a device's detail page unloadable.
        Ok(rows
            .into_iter()
            .filter_map(|r| {
                Some(DeviceSignal {
                    kind: kind_from_db(&r.kind)?,
                    value: r.value,
                    inferred: r.confidence == CONFIDENCE_INFERRED,
                    observed_at: r.observed_at.parse().ok()?,
                })
            })
            .collect())
    }
}
