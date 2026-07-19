use async_trait::async_trait;
use sqlx::SqlitePool;

use crate::db::DbPools;
use crate::repository::private_dns::{
    DeviceAlreadyGrantedPrivateDnsError, PrivateDnsGrantRepository, PrivateDnsGrantRow,
};

#[derive(sqlx::FromRow)]
struct DbPrivateDnsGrantRow {
    id: String,
    device_id: String,
    token: String,
    created_at: String,
}

impl DbPrivateDnsGrantRow {
    fn into_domain(self) -> PrivateDnsGrantRow {
        PrivateDnsGrantRow {
            id: self.id,
            device_id: self.device_id,
            token: self.token,
            created_at: self.created_at,
        }
    }
}

/// Column list shared by every `SELECT` so the mapping stays in one place.
const SELECT_COLS: &str = "id, device_id, token, created_at";

/// Return `true` if a `SQLite` unique-constraint error is the one guarding the
/// `device_id` link specifically (as opposed to `token`).
///
/// `SQLite` does not surface the index name (`idx_private_dns_grants_device_id`)
/// in its error the way some engines do — `constraint()` is typically `None` —
/// so the reliable signal is the rendered message, which names the offending
/// column as `private_dns_grants.device_id`. We check `constraint()` first (in
/// case a future driver populates it with the index name) and fall back to the
/// column reference in the message.
fn is_device_id_constraint(db_err: &dyn sqlx::error::DatabaseError) -> bool {
    if db_err.constraint() == Some("idx_private_dns_grants_device_id") {
        return true;
    }
    db_err.message().contains("private_dns_grants.device_id")
}

/// `SQLite`-backed [`PrivateDnsGrantRepository`].
pub struct SqlitePrivateDnsGrantRepository {
    pools: DbPools,
}

impl SqlitePrivateDnsGrantRepository {
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
impl PrivateDnsGrantRepository for SqlitePrivateDnsGrantRepository {
    async fn insert(&self, row: &PrivateDnsGrantRow) -> anyhow::Result<()> {
        let result = sqlx::query(
            "INSERT INTO private_dns_grants (id, device_id, token, created_at) \
             VALUES (?, ?, ?, ?)",
        )
        .bind(&row.id)
        .bind(&row.device_id)
        .bind(&row.token)
        .bind(&row.created_at)
        .execute(&self.pools.write)
        .await;

        if let Err(sqlx::Error::Database(db_err)) = &result
            && db_err.is_unique_violation()
            && is_device_id_constraint(db_err.as_ref())
        {
            // The device already has a grant — a business-rule conflict, not a
            // generic fault. Surface a marker the service layer maps to a 409.
            // A token collision falls through to the ordinary error below.
            return Err(anyhow::Error::new(DeviceAlreadyGrantedPrivateDnsError));
        }
        result?;
        Ok(())
    }

    async fn find_by_id(&self, id: &str) -> anyhow::Result<Option<PrivateDnsGrantRow>> {
        let query = format!("SELECT {SELECT_COLS} FROM private_dns_grants WHERE id = ?");
        let row = sqlx::query_as::<_, DbPrivateDnsGrantRow>(sqlx::AssertSqlSafe(query))
            .bind(id)
            .fetch_optional(&self.pools.read)
            .await?;
        Ok(row.map(DbPrivateDnsGrantRow::into_domain))
    }

    async fn find_by_device_id(
        &self,
        device_id: &str,
    ) -> anyhow::Result<Option<PrivateDnsGrantRow>> {
        let query = format!("SELECT {SELECT_COLS} FROM private_dns_grants WHERE device_id = ?");
        let row = sqlx::query_as::<_, DbPrivateDnsGrantRow>(sqlx::AssertSqlSafe(query))
            .bind(device_id)
            .fetch_optional(&self.pools.read)
            .await?;
        Ok(row.map(DbPrivateDnsGrantRow::into_domain))
    }

    async fn find_by_token(&self, token: &str) -> anyhow::Result<Option<PrivateDnsGrantRow>> {
        let query = format!("SELECT {SELECT_COLS} FROM private_dns_grants WHERE token = ?");
        let row = sqlx::query_as::<_, DbPrivateDnsGrantRow>(sqlx::AssertSqlSafe(query))
            .bind(token)
            .fetch_optional(&self.pools.read)
            .await?;
        Ok(row.map(DbPrivateDnsGrantRow::into_domain))
    }

    async fn find_all(&self) -> anyhow::Result<Vec<PrivateDnsGrantRow>> {
        let query =
            format!("SELECT {SELECT_COLS} FROM private_dns_grants ORDER BY created_at ASC, id ASC");
        let rows = sqlx::query_as::<_, DbPrivateDnsGrantRow>(sqlx::AssertSqlSafe(query))
            .fetch_all(&self.pools.read)
            .await?;
        Ok(rows
            .into_iter()
            .map(DbPrivateDnsGrantRow::into_domain)
            .collect())
    }

    async fn delete(&self, id: &str) -> anyhow::Result<()> {
        sqlx::query("DELETE FROM private_dns_grants WHERE id = ?")
            .bind(id)
            .execute(&self.pools.write)
            .await?;
        Ok(())
    }
}
