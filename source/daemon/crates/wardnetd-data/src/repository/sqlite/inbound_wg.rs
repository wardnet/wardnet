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
        }
    }
}

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
             (id, public_key, allowed_ip, name, enabled, created_at) \
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(&row.id)
        .bind(&row.public_key)
        .bind(&row.allowed_ip)
        .bind(&row.name)
        .bind(row.enabled)
        .bind(&row.created_at)
        .execute(&self.pools.write)
        .await?;
        Ok(())
    }

    async fn find_by_id(&self, id: &str) -> anyhow::Result<Option<InboundWgPeerRow>> {
        const FIND_BY_ID: &str = "SELECT id, public_key, allowed_ip, name, enabled, created_at \
             FROM inbound_wg_peers WHERE id = ?";
        let row = sqlx::query_as::<_, DbInboundWgPeerRow>(FIND_BY_ID)
            .bind(id)
            .fetch_optional(&self.pools.read)
            .await?;
        Ok(row.map(DbInboundWgPeerRow::into_domain))
    }

    async fn find_all(&self) -> anyhow::Result<Vec<InboundWgPeerRow>> {
        const FIND_ALL: &str = "SELECT id, public_key, allowed_ip, name, enabled, created_at \
             FROM inbound_wg_peers ORDER BY created_at ASC, id ASC";
        let rows = sqlx::query_as::<_, DbInboundWgPeerRow>(FIND_ALL)
            .fetch_all(&self.pools.read)
            .await?;
        Ok(rows
            .into_iter()
            .map(DbInboundWgPeerRow::into_domain)
            .collect())
    }

    async fn find_enabled(&self) -> anyhow::Result<Vec<InboundWgPeerRow>> {
        const FIND_ENABLED: &str = "SELECT id, public_key, allowed_ip, name, enabled, created_at \
             FROM inbound_wg_peers WHERE enabled = 1 ORDER BY created_at ASC, id ASC";
        let rows = sqlx::query_as::<_, DbInboundWgPeerRow>(FIND_ENABLED)
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
