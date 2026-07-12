use async_trait::async_trait;
use sqlx::SqlitePool;

use crate::db::DbPools;
use crate::repository::inbound_wg::{
    DeviceAlreadyGrantedError, InboundWgPeerRepository, InboundWgPeerRow,
};

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

/// Return `true` if a `SQLite` unique-constraint error is the one guarding the
/// `device_id` link specifically (as opposed to `public_key` / `allowed_ip`).
///
/// `SQLite` does not surface the index name (`idx_inbound_wg_peers_device_id`)
/// in its error the way some engines do — `constraint()` is typically `None` —
/// so the reliable signal is the rendered message, which names the offending
/// column as `inbound_wg_peers.device_id`. We check `constraint()` first (in
/// case a future driver populates it with the index name) and fall back to the
/// column reference in the message.
fn is_device_id_constraint(db_err: &dyn sqlx::error::DatabaseError) -> bool {
    if db_err.constraint() == Some("idx_inbound_wg_peers_device_id") {
        return true;
    }
    db_err.message().contains("inbound_wg_peers.device_id")
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
        let result = sqlx::query(
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
        .await;

        if let Err(sqlx::Error::Database(db_err)) = &result
            && db_err.is_unique_violation()
            && is_device_id_constraint(db_err.as_ref())
        {
            // The device already has a credential — a business-rule conflict,
            // not a generic fault. Surface a marker the service layer maps to a
            // 409. Other unique columns (public_key / allowed_ip) fall through
            // to the ordinary error below.
            return Err(anyhow::Error::new(DeviceAlreadyGrantedError));
        }
        result?;
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

    async fn set_enabled(&self, id: &str, enabled: bool) -> anyhow::Result<()> {
        sqlx::query("UPDATE inbound_wg_peers SET enabled = ? WHERE id = ?")
            .bind(enabled)
            .bind(id)
            .execute(&self.pools.write)
            .await?;
        Ok(())
    }
}
