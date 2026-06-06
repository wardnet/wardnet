use async_trait::async_trait;
use chrono::{DateTime, NaiveDateTime, TimeZone, Utc};
use sqlx::SqlitePool;
use sqlx::sqlite::SqliteConnection;
use wardnet_common::api::LastShutdownState;

use super::super::{LastShutdownInfo, SystemConfigRepository};
use crate::db::DbPools;

const TS_FMT: &str = "%Y-%m-%dT%H:%M:%SZ";

// Markers for the *current* run, updated on boot (`running`) and on
// graceful exit (`graceful`).
const KEY_LAST_SHUTDOWN_STATE: &str = "last_shutdown_state";
const KEY_LAST_SHUTDOWN_AT: &str = "last_shutdown_at";
// Heartbeat — the most recent moment the previous run was alive.
const KEY_LAST_SEEN_AT: &str = "last_seen_at";
// Ack for the most recent unclean event; banner predicate compares
// this against `KEY_PREV_SHUTDOWN_AT`.
const KEY_LAST_SHUTDOWN_ACK: &str = "last_shutdown_acknowledged_at";

// Persisted classification of the *previous* run, written once by
// `detect_previous_shutdown` so the read path can surface it without
// re-deriving it from the live "running" marker.
const KEY_PREV_SHUTDOWN_STATE: &str = "prev_shutdown_state";
const KEY_PREV_SHUTDOWN_AT: &str = "prev_shutdown_at";

const STATE_RUNNING: &str = "running";
const STATE_GRACEFUL: &str = "graceful";
const STATE_UNCLEAN: &str = "unclean";
const STATE_UNKNOWN: &str = "unknown";

fn now_iso() -> String {
    Utc::now().format(TS_FMT).to_string()
}

fn parse_ts(value: &str) -> anyhow::Result<DateTime<Utc>> {
    let naive = NaiveDateTime::parse_from_str(value, TS_FMT)?;
    Ok(Utc.from_utc_datetime(&naive))
}

fn parse_ts_opt(value: Option<&str>) -> anyhow::Result<Option<DateTime<Utc>>> {
    value.map(parse_ts).transpose()
}

async fn get_value(conn: &mut SqliteConnection, key: &str) -> anyhow::Result<Option<String>> {
    let v = sqlx::query_scalar::<_, String>("SELECT value FROM system_config WHERE key = ?")
        .bind(key)
        .fetch_optional(conn)
        .await?;
    Ok(v)
}

async fn upsert_value(conn: &mut SqliteConnection, key: &str, value: &str) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO system_config (key, value) VALUES (?, ?) \
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
    )
    .bind(key)
    .bind(value)
    .execute(conn)
    .await?;
    Ok(())
}

fn state_to_str(state: LastShutdownState) -> &'static str {
    match state {
        LastShutdownState::Graceful => STATE_GRACEFUL,
        LastShutdownState::Unclean => STATE_UNCLEAN,
        LastShutdownState::Unknown => STATE_UNKNOWN,
    }
}

fn parse_prev_state(value: Option<&str>) -> LastShutdownState {
    match value {
        Some(STATE_GRACEFUL) => LastShutdownState::Graceful,
        Some(STATE_UNCLEAN) => LastShutdownState::Unclean,
        _ => LastShutdownState::Unknown,
    }
}

/// SQLite-backed implementation of [`SystemConfigRepository`].
pub struct SqliteSystemConfigRepository {
    pools: DbPools,
}

impl SqliteSystemConfigRepository {
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

#[async_trait]
impl SystemConfigRepository for SqliteSystemConfigRepository {
    async fn get(&self, key: &str) -> anyhow::Result<Option<String>> {
        let row = sqlx::query_scalar::<_, String>("SELECT value FROM system_config WHERE key = ?")
            .bind(key)
            .fetch_optional(&self.pools.read)
            .await?;
        Ok(row)
    }

    async fn set(&self, key: &str, value: &str) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO system_config (key, value) VALUES (?, ?) \
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        )
        .bind(key)
        .bind(value)
        .execute(&self.pools.write)
        .await?;
        Ok(())
    }

    async fn delete(&self, key: &str) -> anyhow::Result<()> {
        sqlx::query("DELETE FROM system_config WHERE key = ?")
            .bind(key)
            .execute(&self.pools.write)
            .await?;
        Ok(())
    }

    async fn device_count(&self) -> anyhow::Result<i64> {
        let count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM devices")
            .fetch_one(&self.pools.read)
            .await?;
        Ok(count)
    }

    async fn tunnel_count(&self) -> anyhow::Result<i64> {
        let count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM tunnels")
            .fetch_one(&self.pools.read)
            .await?;
        Ok(count)
    }

    async fn db_size_bytes(&self) -> anyhow::Result<u64> {
        let size = sqlx::query_scalar::<_, i64>(
            "SELECT page_count * page_size AS size FROM pragma_page_count(), pragma_page_size()",
        )
        .fetch_one(&self.pools.read)
        .await?;
        Ok(u64::try_from(size).unwrap_or(0))
    }

    async fn detect_previous_shutdown(&self) -> anyhow::Result<LastShutdownInfo> {
        let mut tx = self.pools.write.begin().await?;

        let prev_state = get_value(&mut tx, KEY_LAST_SHUTDOWN_STATE).await?;
        let prev_at = parse_ts_opt(get_value(&mut tx, KEY_LAST_SHUTDOWN_AT).await?.as_deref())?;
        let prev_seen = parse_ts_opt(get_value(&mut tx, KEY_LAST_SEEN_AT).await?.as_deref())?;
        let acknowledged_at =
            parse_ts_opt(get_value(&mut tx, KEY_LAST_SHUTDOWN_ACK).await?.as_deref())?;

        let info = match prev_state.as_deref() {
            Some(STATE_GRACEFUL) => LastShutdownInfo {
                state: LastShutdownState::Graceful,
                at: prev_at,
                acknowledged_at,
            },
            Some(STATE_RUNNING) => LastShutdownInfo {
                state: LastShutdownState::Unclean,
                // Pick the later of the two timestamps. `last_seen_at`
                // can be a stale heartbeat from an earlier run that
                // wasn't reset before the prior run armed its marker
                // and crashed; in that case `last_shutdown_at` (the
                // prior run's boot time) is the more accurate floor.
                at: [prev_seen, prev_at].into_iter().flatten().max(),
                acknowledged_at,
            },
            // None → first-ever boot. Some(legacy/corrupt) → fail safe.
            // Both surface as `Unknown` with no timestamp.
            _ => LastShutdownInfo {
                state: LastShutdownState::Unknown,
                at: None,
                acknowledged_at,
            },
        };

        // Persist the classification so the API can surface it later.
        upsert_value(&mut tx, KEY_PREV_SHUTDOWN_STATE, state_to_str(info.state)).await?;
        match info.at {
            Some(at) => {
                upsert_value(
                    &mut tx,
                    KEY_PREV_SHUTDOWN_AT,
                    &at.format(TS_FMT).to_string(),
                )
                .await?;
            }
            None => {
                sqlx::query("DELETE FROM system_config WHERE key = ?")
                    .bind(KEY_PREV_SHUTDOWN_AT)
                    .execute(&mut *tx)
                    .await?;
            }
        }

        // Arm the marker for *this* run.
        let now = now_iso();
        upsert_value(&mut tx, KEY_LAST_SHUTDOWN_STATE, STATE_RUNNING).await?;
        upsert_value(&mut tx, KEY_LAST_SHUTDOWN_AT, &now).await?;

        tx.commit().await?;
        Ok(info)
    }

    async fn record_graceful_shutdown(&self) -> anyhow::Result<()> {
        let now = now_iso();
        let mut tx = self.pools.write.begin().await?;
        upsert_value(&mut tx, KEY_LAST_SHUTDOWN_STATE, STATE_GRACEFUL).await?;
        upsert_value(&mut tx, KEY_LAST_SHUTDOWN_AT, &now).await?;
        tx.commit().await?;
        Ok(())
    }

    async fn record_heartbeat(&self) -> anyhow::Result<()> {
        self.set(KEY_LAST_SEEN_AT, &now_iso()).await
    }

    async fn read_last_shutdown(&self) -> anyhow::Result<LastShutdownInfo> {
        let prev_state = self.get(KEY_PREV_SHUTDOWN_STATE).await?;
        let prev_at = parse_ts_opt(self.get(KEY_PREV_SHUTDOWN_AT).await?.as_deref())?;
        let acknowledged_at = parse_ts_opt(self.get(KEY_LAST_SHUTDOWN_ACK).await?.as_deref())?;

        Ok(LastShutdownInfo {
            state: parse_prev_state(prev_state.as_deref()),
            at: prev_at,
            acknowledged_at,
        })
    }

    async fn acknowledge_last_shutdown(&self) -> anyhow::Result<()> {
        self.set(KEY_LAST_SHUTDOWN_ACK, &now_iso()).await
    }
}
