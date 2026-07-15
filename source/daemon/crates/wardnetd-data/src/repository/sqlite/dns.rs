//! SQLite-backed [`DnsRepository`] implementation — query log only.
//!
//! After the Stage 7 split, filter-source CRUD lives in
//! [`SqliteDnsFilterRepository`](super::dns_filter::SqliteDnsFilterRepository).
//! DNS observability stats moved to the generic `StatsRepository`.

use async_trait::async_trait;
use sqlx::SqlitePool;

use crate::db::DbPools;
use crate::repository::dns::{DnsRepository, QueryLogFilter, QueryLogRow};

const TS_FMT: &str = "%Y-%m-%dT%H:%M:%SZ";

pub struct SqliteDnsRepository {
    pools: DbPools,
}

impl SqliteDnsRepository {
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self::new_pools(DbPools::single(pool))
    }

    #[must_use]
    pub fn new_pools(pools: DbPools) -> Self {
        Self { pools }
    }
}

#[derive(sqlx::FromRow)]
struct DbQueryLogRow {
    timestamp: String,
    client_ip: String,
    domain: String,
    query_type: String,
    result: String,
    upstream: Option<String>,
    latency_ms: f64,
    device_id: Option<String>,
}

impl DbQueryLogRow {
    fn into_row(self) -> QueryLogRow {
        QueryLogRow {
            timestamp: self.timestamp,
            client_ip: self.client_ip,
            domain: self.domain,
            query_type: self.query_type,
            result: self.result,
            upstream: self.upstream,
            latency_ms: self.latency_ms,
            device_id: self.device_id,
        }
    }
}

#[async_trait]
impl DnsRepository for SqliteDnsRepository {
    async fn insert_query_log_batch(&self, entries: &[QueryLogRow]) -> anyhow::Result<()> {
        let mut tx = self.pools.write.begin().await?;
        for entry in entries {
            sqlx::query(
                "INSERT INTO dns_query_log \
                 (timestamp, client_ip, domain, query_type, result, upstream, latency_ms, device_id) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&entry.timestamp)
            .bind(&entry.client_ip)
            .bind(&entry.domain)
            .bind(&entry.query_type)
            .bind(&entry.result)
            .bind(&entry.upstream)
            .bind(entry.latency_ms)
            .bind(&entry.device_id)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    async fn query_log_paginated(
        &self,
        limit: u32,
        offset: u32,
        filter: &QueryLogFilter,
    ) -> anyhow::Result<Vec<QueryLogRow>> {
        let (where_clause, binds) = build_where(filter);
        let sql = format!(
            "SELECT timestamp, client_ip, domain, query_type, result, upstream, latency_ms, device_id \
             FROM dns_query_log {where_clause} \
             ORDER BY id DESC LIMIT ? OFFSET ?"
        );

        let mut q = sqlx::query_as::<_, DbQueryLogRow>(sqlx::AssertSqlSafe(sql));
        for b in &binds {
            q = q.bind(b);
        }
        q = q.bind(limit).bind(offset);

        let rows = q.fetch_all(&self.pools.read).await?;
        Ok(rows.into_iter().map(DbQueryLogRow::into_row).collect())
    }

    async fn query_log_count(&self, filter: &QueryLogFilter) -> anyhow::Result<u64> {
        let (where_clause, binds) = build_where(filter);
        let sql = format!("SELECT COUNT(*) FROM dns_query_log {where_clause}");
        let mut q = sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(sql));
        for b in &binds {
            q = q.bind(b);
        }
        let count = q.fetch_one(&self.pools.read).await?;
        Ok(u64::try_from(count).unwrap_or(0))
    }

    async fn cleanup_query_log(&self, retention_days: u32) -> anyhow::Result<u64> {
        let cutoff = chrono::Utc::now()
            .checked_sub_signed(chrono::Duration::days(i64::from(retention_days)))
            .unwrap_or_else(chrono::Utc::now)
            .format(TS_FMT)
            .to_string();

        let result = sqlx::query("DELETE FROM dns_query_log WHERE timestamp < ?")
            .bind(&cutoff)
            .execute(&self.pools.write)
            .await?;
        Ok(result.rows_affected())
    }
}

fn build_where(filter: &QueryLogFilter) -> (String, Vec<String>) {
    let mut conditions: Vec<&str> = Vec::new();
    let mut binds: Vec<String> = Vec::new();
    // Substring match, mirroring the domain filter: the admin UI feeds this
    // from a free-text input, so a partial IP ("192.168.1") must narrow
    // rather than silently matching nothing.
    if let Some(ref ip) = filter.client_ip {
        conditions.push("client_ip LIKE ?");
        binds.push(format!("%{ip}%"));
    }
    if let Some(ref device_id) = filter.device_id {
        conditions.push("device_id = ?");
        binds.push(device_id.clone());
    }
    if let Some(ref domain) = filter.domain {
        conditions.push("domain LIKE ?");
        binds.push(format!("%{domain}%"));
    }
    if let Some(result) = filter.result {
        conditions.push("result = ?");
        binds.push(result.as_str().to_owned());
    }
    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };
    (where_clause, binds)
}
