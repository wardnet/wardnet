use async_trait::async_trait;
use sqlx::SqlitePool;
use wardnet_common::access_request::{AccessRequestKind, AccessRequestStatus, DeviceAccessRequest};

use crate::db::DbPools;
use crate::repository::access_request::{AccessRequestRepository, DuplicateOpenAccessRequestError};

const SELECT_COLS: &str =
    "id, device_id, kind, domain, reason, status, created_at, decided_at, decided_by";

/// True when a unique violation came from the open-request partial index
/// rather than the primary key.
///
/// `SQLite` names the offending *column* rather than the index, and
/// `device_id` is unique only under that partial index — the primary key is
/// `id` — so the column name identifies it unambiguously. The index name is
/// still checked first, for drivers that do report it.
fn is_open_request_constraint(db_err: &dyn sqlx::error::DatabaseError) -> bool {
    if db_err.constraint() == Some("idx_access_requests_open_private_dns") {
        return true;
    }
    let message = db_err.message();
    message.contains("idx_access_requests_open_private_dns")
        || message.contains("device_access_requests.device_id")
}

#[derive(sqlx::FromRow)]
struct DbAccessRequestRow {
    id: String,
    device_id: String,
    kind: String,
    domain: Option<String>,
    reason: Option<String>,
    status: String,
    created_at: String,
    decided_at: Option<String>,
    decided_by: Option<String>,
}

impl DbAccessRequestRow {
    fn into_domain(self) -> DeviceAccessRequest {
        DeviceAccessRequest {
            id: self.id,
            device_id: self.device_id,
            kind: AccessRequestKind::parse(&self.kind),
            domain: self.domain,
            reason: self.reason,
            status: AccessRequestStatus::parse(&self.status),
            created_at: self.created_at,
            decided_at: self.decided_at,
            decided_by: self.decided_by,
        }
    }
}

/// `SQLite`-backed [`AccessRequestRepository`].
pub struct SqliteAccessRequestRepository {
    pools: DbPools,
}

impl SqliteAccessRequestRepository {
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self::new_pools(DbPools::single(pool))
    }

    #[must_use]
    pub fn new_pools(pools: DbPools) -> Self {
        Self { pools }
    }

    async fn fetch_by_id(&self, id: &str) -> anyhow::Result<Option<DeviceAccessRequest>> {
        let query = format!("SELECT {SELECT_COLS} FROM device_access_requests WHERE id = ?");
        let row = sqlx::query_as::<_, DbAccessRequestRow>(sqlx::AssertSqlSafe(query))
            .bind(id)
            .fetch_optional(&self.pools.read)
            .await?;
        Ok(row.map(DbAccessRequestRow::into_domain))
    }
}

#[async_trait]
impl AccessRequestRepository for SqliteAccessRequestRepository {
    async fn insert(
        &self,
        id: &str,
        device_id: &str,
        kind: AccessRequestKind,
        domain: Option<&str>,
        reason: Option<&str>,
        created_at: &str,
    ) -> anyhow::Result<DeviceAccessRequest> {
        let result = sqlx::query(
            "INSERT INTO device_access_requests \
             (id, device_id, kind, domain, reason, status, created_at) \
             VALUES (?, ?, ?, ?, ?, 'pending', ?)",
        )
        .bind(id)
        .bind(device_id)
        .bind(kind.as_str())
        .bind(domain)
        .bind(reason)
        .bind(created_at)
        .execute(&self.pools.write)
        .await;

        if let Err(sqlx::Error::Database(db_err)) = &result
            && db_err.is_unique_violation()
            && is_open_request_constraint(db_err.as_ref())
        {
            // The device already has an open request of this kind — a
            // business-rule conflict, not a generic fault. Surface a marker the
            // service layer maps to a 409.
            return Err(anyhow::Error::new(DuplicateOpenAccessRequestError));
        }
        result?;

        Ok(DeviceAccessRequest {
            id: id.to_owned(),
            device_id: device_id.to_owned(),
            kind,
            domain: domain.map(str::to_owned),
            reason: reason.map(str::to_owned),
            status: AccessRequestStatus::Pending,
            created_at: created_at.to_owned(),
            decided_at: None,
            decided_by: None,
        })
    }

    async fn list_by_device(&self, device_id: &str) -> anyhow::Result<Vec<DeviceAccessRequest>> {
        let query = format!(
            "SELECT {SELECT_COLS} FROM device_access_requests \
             WHERE device_id = ? ORDER BY created_at DESC, id DESC"
        );
        let rows = sqlx::query_as::<_, DbAccessRequestRow>(sqlx::AssertSqlSafe(query))
            .bind(device_id)
            .fetch_all(&self.pools.read)
            .await?;
        Ok(rows
            .into_iter()
            .map(DbAccessRequestRow::into_domain)
            .collect())
    }

    async fn list_all(
        &self,
        status: Option<AccessRequestStatus>,
    ) -> anyhow::Result<Vec<DeviceAccessRequest>> {
        let rows = if let Some(s) = status {
            let query = format!(
                "SELECT {SELECT_COLS} FROM device_access_requests \
                 WHERE status = ? ORDER BY created_at DESC, id DESC"
            );
            sqlx::query_as::<_, DbAccessRequestRow>(sqlx::AssertSqlSafe(query))
                .bind(s.as_str())
                .fetch_all(&self.pools.read)
                .await?
        } else {
            let query = format!(
                "SELECT {SELECT_COLS} FROM device_access_requests \
                 ORDER BY created_at DESC, id DESC"
            );
            sqlx::query_as::<_, DbAccessRequestRow>(sqlx::AssertSqlSafe(query))
                .fetch_all(&self.pools.read)
                .await?
        };
        Ok(rows
            .into_iter()
            .map(DbAccessRequestRow::into_domain)
            .collect())
    }

    async fn find_by_id(&self, id: &str) -> anyhow::Result<Option<DeviceAccessRequest>> {
        self.fetch_by_id(id).await
    }

    async fn update_status(
        &self,
        id: &str,
        status: AccessRequestStatus,
        decided_by: &str,
        decided_at: &str,
    ) -> anyhow::Result<Option<DeviceAccessRequest>> {
        // `AND status = 'pending'` makes the idempotence the trait documents
        // real rather than merely intended: `resolve_pending` can decide the
        // same row from the event bus, and without this guard whichever write
        // landed second would silently overwrite the first one's `decided_by`.
        // A caller that finds nothing to update re-reads to see who won.
        let affected = sqlx::query(
            "UPDATE device_access_requests \
             SET status = ?, decided_by = ?, decided_at = ? \
             WHERE id = ? AND status = 'pending'",
        )
        .bind(status.as_str())
        .bind(decided_by)
        .bind(decided_at)
        .bind(id)
        .execute(&self.pools.write)
        .await?
        .rows_affected();

        if affected == 0 {
            return Ok(None);
        }
        self.fetch_by_id(id).await
    }

    async fn resolve_pending(
        &self,
        device_id: &str,
        kind: AccessRequestKind,
        status: AccessRequestStatus,
        decided_by: Option<&str>,
        decided_at: &str,
    ) -> anyhow::Result<Option<DeviceAccessRequest>> {
        // `AND status = 'pending'` is what makes this idempotent — see the
        // trait doc. Without it the listener would overwrite the `decided_by`
        // the approving admin just wrote.
        let query = format!(
            "UPDATE device_access_requests \
             SET status = ?, decided_by = ?, decided_at = ? \
             WHERE device_id = ? AND kind = ? AND status = 'pending' \
             RETURNING {SELECT_COLS}"
        );
        let row = sqlx::query_as::<_, DbAccessRequestRow>(sqlx::AssertSqlSafe(query))
            .bind(status.as_str())
            .bind(decided_by)
            .bind(decided_at)
            .bind(device_id)
            .bind(kind.as_str())
            .fetch_optional(&self.pools.write)
            .await?;
        Ok(row.map(DbAccessRequestRow::into_domain))
    }
}
