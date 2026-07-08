use async_trait::async_trait;
use sqlx::SqlitePool;

use crate::db::DbPools;
use crate::repository::inbound_wg::{InboundWgPeerRepository, InboundWgPeerRow};

#[derive(sqlx::FromRow)]
struct DbInboundWgPeerRow {
    id: String,
    public_key: String,
    allowed_ip: String,
    name: String,
    enabled: bool,
    created_at: String,
    device_id: Option<String>,
}

impl DbInboundWgPeerRow {
    fn into_domain(self) -> InboundWgPeerRow {
        InboundWgPeerRow {
            id: self.id,
            public_key: self.public_key,
            allowed_ip: self.allowed_ip,
            name: self.name,
            enabled: self.enabled,
            created_at: self.created_at,
            device_id: self.device_id,
        }
    }
}

/// Column list shared by every `SELECT` so the mapping stays in one place.
const SELECT_COLS: &str = "id, public_key, allowed_ip, name, enabled, created_at, device_id";

/// `SQLite`-backed [`InboundWgPeerRepository`].
pub struct SqliteInboundWgPeerRepository {
    pools: DbPools,
}

impl SqliteInboundWgPeerRepository {
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
impl InboundWgPeerRepository for SqliteInboundWgPeerRepository {
    async fn insert(&self, row: &InboundWgPeerRow) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO inbound_wg_peers \
             (id, public_key, allowed_ip, name, enabled, created_at, device_id) \
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&row.id)
        .bind(&row.public_key)
        .bind(&row.allowed_ip)
        .bind(&row.name)
        .bind(row.enabled)
        .bind(&row.created_at)
        .bind(&row.device_id)
        .execute(&self.pools.write)
        .await?;
        Ok(())
    }

    async fn find_by_id(&self, id: &str) -> anyhow::Result<Option<InboundWgPeerRow>> {
        let query = format!("SELECT {SELECT_COLS} FROM inbound_wg_peers WHERE id = ?");
        let row = sqlx::query_as::<_, DbInboundWgPeerRow>(sqlx::AssertSqlSafe(query))
            .bind(id)
            .fetch_optional(&self.pools.read)
            .await?;
        Ok(row.map(DbInboundWgPeerRow::into_domain))
    }

    async fn find_by_device_id(&self, device_id: &str) -> anyhow::Result<Option<InboundWgPeerRow>> {
        let query = format!("SELECT {SELECT_COLS} FROM inbound_wg_peers WHERE device_id = ?");
        let row = sqlx::query_as::<_, DbInboundWgPeerRow>(sqlx::AssertSqlSafe(query))
            .bind(device_id)
            .fetch_optional(&self.pools.read)
            .await?;
        Ok(row.map(DbInboundWgPeerRow::into_domain))
    }

    async fn find_all(&self) -> anyhow::Result<Vec<InboundWgPeerRow>> {
        let query =
            format!("SELECT {SELECT_COLS} FROM inbound_wg_peers ORDER BY created_at ASC, id ASC");
        let rows = sqlx::query_as::<_, DbInboundWgPeerRow>(sqlx::AssertSqlSafe(query))
            .fetch_all(&self.pools.read)
            .await?;
        Ok(rows
            .into_iter()
            .map(DbInboundWgPeerRow::into_domain)
            .collect())
    }

    async fn find_enabled(&self) -> anyhow::Result<Vec<InboundWgPeerRow>> {
        let query = format!(
            "SELECT {SELECT_COLS} FROM inbound_wg_peers \
             WHERE enabled = 1 ORDER BY created_at ASC, id ASC"
        );
        let rows = sqlx::query_as::<_, DbInboundWgPeerRow>(sqlx::AssertSqlSafe(query))
            .fetch_all(&self.pools.read)
            .await?;
        Ok(rows
            .into_iter()
            .map(DbInboundWgPeerRow::into_domain)
            .collect())
    }

    async fn delete(&self, id: &str) -> anyhow::Result<()> {
        sqlx::query("DELETE FROM inbound_wg_peers WHERE id = ?")
            .bind(id)
            .execute(&self.pools.write)
            .await?;
        Ok(())
    }
}
