//! SQLite-backed [`MaintenanceRepository`] implementation.

use async_trait::async_trait;
use sqlx::SqlitePool;

use crate::db::DbPools;
use crate::repository::maintenance::MaintenanceRepository;

/// How many pages a single `PRAGMA incremental_vacuum(N)` call is
/// allowed to reclaim. Tuned for a Raspberry Pi: ~8 MiB at the SQLite
/// default 4 KiB page size — small enough to release the writer lock
/// promptly so concurrent flushes don't queue against
/// `busy_timeout`. The cleanup tick fires daily; unreclaimed pages
/// stay on the freelist and get picked up by the next call.
const INCREMENTAL_VACUUM_PAGES: u32 = 2_000;

pub struct SqliteMaintenanceRepository {
    pools: DbPools,
}

impl SqliteMaintenanceRepository {
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
impl MaintenanceRepository for SqliteMaintenanceRepository {
    async fn incremental_vacuum(&self) -> anyhow::Result<u64> {
        // `PRAGMA incremental_vacuum(N)` is a no-op on databases
        // created with `auto_vacuum=NONE`, so it's safe to call
        // unconditionally. The `before`/`after` freelist samples are
        // taken outside any transaction; concurrent writers can grow
        // the freelist between samples, so the returned page count is
        // best-effort telemetry rather than a precise total.
        let before: i64 = sqlx::query_scalar("PRAGMA freelist_count")
            .fetch_one(&self.pools.write)
            .await
            .unwrap_or(0);
        let stmt = format!("PRAGMA incremental_vacuum({INCREMENTAL_VACUUM_PAGES})");
        sqlx::query(&stmt).execute(&self.pools.write).await?;
        let after: i64 = sqlx::query_scalar("PRAGMA freelist_count")
            .fetch_one(&self.pools.write)
            .await
            .unwrap_or(before);
        Ok(u64::try_from((before - after).max(0)).unwrap_or(0))
    }
}
