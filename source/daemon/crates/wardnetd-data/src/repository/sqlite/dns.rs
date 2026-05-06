//! SQLite-backed [`DnsRepository`] implementation — query log only.
//!
//! After the Stage 7 split, filter-source CRUD lives in
//! [`SqliteDnsFilterRepository`](super::dns_filter::SqliteDnsFilterRepository).

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::SqlitePool;

use crate::repository::dns::{
    BucketSize, DnsRepository, QueryLogFilter, QueryLogRow, QueryStatsRow, SeriesBucketRow,
    TopClientRow, TopDomainRow,
};

const TS_FMT: &str = "%Y-%m-%dT%H:%M:%SZ";

pub struct SqliteDnsRepository {
    pool: SqlitePool,
}

impl SqliteDnsRepository {
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
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
        let mut tx = self.pool.begin().await?;
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

        let mut q = sqlx::query_as::<_, DbQueryLogRow>(&sql);
        for b in &binds {
            q = q.bind(b);
        }
        q = q.bind(limit).bind(offset);

        let rows = q.fetch_all(&self.pool).await?;
        Ok(rows.into_iter().map(DbQueryLogRow::into_row).collect())
    }

    async fn query_log_count(&self, filter: &QueryLogFilter) -> anyhow::Result<u64> {
        let (where_clause, binds) = build_where(filter);
        let sql = format!("SELECT COUNT(*) FROM dns_query_log {where_clause}");
        let mut q = sqlx::query_scalar::<_, i64>(&sql);
        for b in &binds {
            q = q.bind(b);
        }
        let count = q.fetch_one(&self.pool).await?;
        Ok(u64::try_from(count).unwrap_or(0))
    }

    async fn cleanup_query_log(&self, retention_days: u32) -> anyhow::Result<u64> {
        let cutoff = Utc::now()
            .checked_sub_signed(chrono::Duration::days(i64::from(retention_days)))
            .unwrap_or_else(Utc::now)
            .format(TS_FMT)
            .to_string();

        let result = sqlx::query("DELETE FROM dns_query_log WHERE timestamp < ?")
            .bind(&cutoff)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected())
    }

    async fn query_stats(&self, since: DateTime<Utc>) -> anyhow::Result<QueryStatsRow> {
        let since_str = since.format(TS_FMT).to_string();
        let row: (i64, i64, Option<f64>, i64, i64) = sqlx::query_as(
            "SELECT \
                 COUNT(*) AS total, \
                 COALESCE(SUM(CASE WHEN result = 'blocked' THEN 1 ELSE 0 END), 0) AS blocked, \
                 AVG(latency_ms) AS avg_latency, \
                 COUNT(DISTINCT client_ip) AS unique_clients, \
                 COUNT(DISTINCT domain) AS unique_domains \
             FROM dns_query_log WHERE timestamp >= ?",
        )
        .bind(&since_str)
        .fetch_one(&self.pool)
        .await?;

        Ok(QueryStatsRow {
            total_queries: u64::try_from(row.0).unwrap_or(0),
            blocked_queries: u64::try_from(row.1).unwrap_or(0),
            avg_latency_ms: row.2.unwrap_or(0.0),
            unique_clients: u64::try_from(row.3).unwrap_or(0),
            unique_domains: u64::try_from(row.4).unwrap_or(0),
        })
    }

    async fn top_domains(
        &self,
        since: DateTime<Utc>,
        limit: u32,
        blocked_only: bool,
    ) -> anyhow::Result<Vec<TopDomainRow>> {
        let since_str = since.format(TS_FMT).to_string();
        let sql = if blocked_only {
            "SELECT domain, COUNT(*) AS c FROM dns_query_log \
             WHERE timestamp >= ? AND result = 'blocked' \
             GROUP BY domain ORDER BY c DESC, domain ASC LIMIT ?"
        } else {
            "SELECT domain, COUNT(*) AS c FROM dns_query_log \
             WHERE timestamp >= ? \
             GROUP BY domain ORDER BY c DESC, domain ASC LIMIT ?"
        };

        let rows: Vec<(String, i64)> = sqlx::query_as(sql)
            .bind(&since_str)
            .bind(limit)
            .fetch_all(&self.pool)
            .await?;

        Ok(rows
            .into_iter()
            .map(|(domain, count)| TopDomainRow {
                domain,
                count: u64::try_from(count).unwrap_or(0),
            })
            .collect())
    }

    async fn top_clients(
        &self,
        since: DateTime<Utc>,
        limit: u32,
    ) -> anyhow::Result<Vec<TopClientRow>> {
        let since_str = since.format(TS_FMT).to_string();
        let rows: Vec<(String, i64)> = sqlx::query_as(
            "SELECT client_ip, COUNT(*) AS c FROM dns_query_log \
             WHERE timestamp >= ? \
             GROUP BY client_ip ORDER BY c DESC, client_ip ASC LIMIT ?",
        )
        .bind(&since_str)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|(client_ip, count)| TopClientRow {
                client_ip,
                count: u64::try_from(count).unwrap_or(0),
            })
            .collect())
    }

    async fn series_buckets(
        &self,
        since: DateTime<Utc>,
        bucket: BucketSize,
    ) -> anyhow::Result<Vec<SeriesBucketRow>> {
        let since_str = since.format(TS_FMT).to_string();
        let fmt = bucket.strftime_fmt();
        let sql = format!(
            "SELECT strftime('{fmt}', timestamp) AS bucket, \
                    COUNT(*) AS total, \
                    COALESCE(SUM(CASE WHEN result = 'blocked' THEN 1 ELSE 0 END), 0) AS blocked \
             FROM dns_query_log WHERE timestamp >= ? \
             GROUP BY bucket ORDER BY bucket ASC"
        );

        let rows: Vec<(Option<String>, i64, i64)> = sqlx::query_as(&sql)
            .bind(&since_str)
            .fetch_all(&self.pool)
            .await?;

        Ok(rows
            .into_iter()
            .filter_map(|(bucket, total, blocked)| {
                bucket.map(|bucket| SeriesBucketRow {
                    bucket,
                    total: u64::try_from(total).unwrap_or(0),
                    blocked: u64::try_from(blocked).unwrap_or(0),
                })
            })
            .collect())
    }
}

fn build_where(filter: &QueryLogFilter) -> (String, Vec<String>) {
    let mut conditions: Vec<&str> = Vec::new();
    let mut binds: Vec<String> = Vec::new();
    if let Some(ref ip) = filter.client_ip {
        conditions.push("client_ip = ?");
        binds.push(ip.clone());
    }
    if let Some(ref domain) = filter.domain {
        conditions.push("domain LIKE ?");
        binds.push(format!("%{domain}%"));
    }
    if let Some(ref result) = filter.result {
        conditions.push("result = ?");
        binds.push(result.clone());
    }
    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };
    (where_clause, binds)
}
