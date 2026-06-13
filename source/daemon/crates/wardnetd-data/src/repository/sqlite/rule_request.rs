use async_trait::async_trait;
use sqlx::SqlitePool;
use wardnet_common::rule_request::{DeviceRuleRequest, RuleRequestKind, RuleRequestStatus};

use crate::db::DbPools;
use crate::repository::rule_request::RuleRequestRepository;

#[derive(sqlx::FromRow)]
struct DbRuleRequestRow {
    id: String,
    device_id: String,
    kind: String,
    domain: String,
    reason: Option<String>,
    status: String,
    created_at: String,
    decided_at: Option<String>,
    decided_by: Option<String>,
}

impl DbRuleRequestRow {
    fn into_domain(self) -> DeviceRuleRequest {
        DeviceRuleRequest {
            id: self.id,
            device_id: self.device_id,
            kind: RuleRequestKind::parse(&self.kind),
            domain: self.domain,
            reason: self.reason,
            status: RuleRequestStatus::parse(&self.status),
            created_at: self.created_at,
            decided_at: self.decided_at,
            decided_by: self.decided_by,
        }
    }
}

pub struct SqliteRuleRequestRepository {
    pools: DbPools,
}

impl SqliteRuleRequestRepository {
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
impl RuleRequestRepository for SqliteRuleRequestRepository {
    async fn insert(
        &self,
        id: &str,
        device_id: &str,
        kind: RuleRequestKind,
        domain: &str,
        reason: Option<&str>,
        created_at: &str,
    ) -> anyhow::Result<DeviceRuleRequest> {
        sqlx::query(
            "INSERT INTO device_rule_requests \
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
        .await?;

        Ok(DeviceRuleRequest {
            id: id.to_owned(),
            device_id: device_id.to_owned(),
            kind,
            domain: domain.to_owned(),
            reason: reason.map(str::to_owned),
            status: RuleRequestStatus::Pending,
            created_at: created_at.to_owned(),
            decided_at: None,
            decided_by: None,
        })
    }

    async fn list_by_device(&self, device_id: &str) -> anyhow::Result<Vec<DeviceRuleRequest>> {
        let rows = sqlx::query_as::<_, DbRuleRequestRow>(
            "SELECT id, device_id, kind, domain, reason, status, created_at, decided_at, decided_by \
             FROM device_rule_requests WHERE device_id = ? ORDER BY created_at DESC, id DESC",
        )
        .bind(device_id)
        .fetch_all(&self.pools.read)
        .await?;
        Ok(rows
            .into_iter()
            .map(DbRuleRequestRow::into_domain)
            .collect())
    }

    async fn list_all(
        &self,
        status: Option<RuleRequestStatus>,
    ) -> anyhow::Result<Vec<DeviceRuleRequest>> {
        let rows = match status {
            Some(s) => {
                sqlx::query_as::<_, DbRuleRequestRow>(
                    "SELECT id, device_id, kind, domain, reason, status, created_at, decided_at, decided_by \
                     FROM device_rule_requests WHERE status = ? ORDER BY created_at DESC, id DESC",
                )
                .bind(s.as_str())
                .fetch_all(&self.pools.read)
                .await?
            }
            None => {
                sqlx::query_as::<_, DbRuleRequestRow>(
                    "SELECT id, device_id, kind, domain, reason, status, created_at, decided_at, decided_by \
                     FROM device_rule_requests ORDER BY created_at DESC, id DESC",
                )
                .fetch_all(&self.pools.read)
                .await?
            }
        };
        Ok(rows
            .into_iter()
            .map(DbRuleRequestRow::into_domain)
            .collect())
    }

    async fn update_status(
        &self,
        id: &str,
        status: RuleRequestStatus,
        decided_by: &str,
        decided_at: &str,
    ) -> anyhow::Result<Option<DeviceRuleRequest>> {
        let affected = sqlx::query(
            "UPDATE device_rule_requests \
             SET status = ?, decided_by = ?, decided_at = ? WHERE id = ?",
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

        let row = sqlx::query_as::<_, DbRuleRequestRow>(
            "SELECT id, device_id, kind, domain, reason, status, created_at, decided_at, decided_by \
             FROM device_rule_requests WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pools.read)
        .await?;
        Ok(row.map(DbRuleRequestRow::into_domain))
    }
}
