//! SQLite-backed [`DnsRepository`] implementation — query log only.
//!
//! Filter-source CRUD lives in
//! [`SqliteDnsFilterRepository`](super::dns_filter::SqliteDnsFilterRepository).
//! DNS observability stats are served by the generic `StatsRepository`.
//!
//! The query log is normalised: every repeated column is an integer id into a
//! `lk_dns_*` lookup table and `timestamp` is a whole-second Unix epoch. That
//! layout is confined to this module — the repository resolves ids on write and
//! joins them back on read, so nothing above `wardnetd-data` knows the lookup
//! tables exist. See `docs/adr/0034-query-log-normalisation.md`.

use std::collections::{HashMap, HashSet};

use async_trait::async_trait;
use chrono::DateTime;
use sqlx::{Sqlite, SqlitePool, Transaction};

use crate::db::DbPools;
use crate::repository::dns::{DnsRepository, QueryLogFilter, QueryLogRow};

/// Lookup table for each repeated column, paired with the `dns_query_log`
/// column holding its id. Table names reach the SQL builders from this array
/// only — never from a caller — which is what makes the dynamic SQL below safe.
const LK_DOMAIN: &str = "lk_dns_domain";
const LK_CLIENT_IP: &str = "lk_dns_client_ip";
const LK_DEVICE: &str = "lk_dns_device";
const LK_QUERY_TYPE: &str = "lk_dns_query_type";
const LK_RESULT: &str = "lk_dns_result";
const LK_UPSTREAM: &str = "lk_dns_upstream";
const LK_PROTOCOL: &str = "lk_dns_protocol";

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
    timestamp: i64,
    client_ip: String,
    domain: String,
    query_type: String,
    result: String,
    upstream: Option<String>,
    latency_ms: f64,
    device_id: Option<String>,
    protocol: String,
}

impl DbQueryLogRow {
    /// A row whose epoch seconds are outside `DateTime`'s range is dropped
    /// rather than clamped — a clamped timestamp reads as a real observation.
    fn into_row(self) -> Option<QueryLogRow> {
        Some(QueryLogRow {
            timestamp: DateTime::from_timestamp(self.timestamp, 0)?,
            client_ip: self.client_ip,
            domain: self.domain,
            query_type: self.query_type,
            result: self.result,
            upstream: self.upstream,
            latency_ms: self.latency_ms,
            device_id: self.device_id,
            protocol: self.protocol,
        })
    }
}

/// Resolve every `values` entry in `table` to its lookup id, inserting the ones
/// that are new.
///
/// Two statements per table per batch, never one per row — that is what keeps
/// roughly 1.79M lookups a week off the writer without needing a cache in front
/// of it. A batch carries at most as many distinct values as it has rows, and
/// both callers flush in chunks of `BATCH_MAX`, so the bind count stays well
/// inside SQLite's parameter limit.
async fn resolve_ids(
    tx: &mut Transaction<'_, Sqlite>,
    table: &str,
    values: &HashSet<&str>,
) -> anyhow::Result<HashMap<String, i64>> {
    let mut ids = HashMap::with_capacity(values.len());
    if values.is_empty() {
        return Ok(ids);
    }
    let values: Vec<&str> = values.iter().copied().collect();
    let placeholders = vec!["?"; values.len()].join(", ");

    // `ON CONFLICT DO NOTHING` rather than `INSERT OR IGNORE`: the latter also
    // swallows unrelated constraint failures on this table.
    let mut q = sqlx::query(sqlx::AssertSqlSafe(format!(
        "INSERT INTO {table} (v) VALUES {} ON CONFLICT(v) DO NOTHING",
        vec!["(?)"; values.len()].join(", ")
    )));
    for v in &values {
        q = q.bind(*v);
    }
    q.execute(&mut **tx).await?;

    let mut q = sqlx::query_as::<_, (i64, String)>(sqlx::AssertSqlSafe(format!(
        "SELECT id, v FROM {table} WHERE v IN ({placeholders})"
    )));
    for v in &values {
        q = q.bind(*v);
    }
    for (id, v) in q.fetch_all(&mut **tx).await? {
        ids.insert(v, id);
    }
    Ok(ids)
}

/// Look up an id the batch is known to have resolved. A miss means the resolve
/// step and the bind step disagree about which values the batch contains, which
/// would silently write a wrong id; fail the batch instead.
fn id_for(ids: &HashMap<String, i64>, table: &str, value: &str) -> anyhow::Result<i64> {
    ids.get(value)
        .copied()
        .ok_or_else(|| anyhow::anyhow!("unresolved {table} lookup value: {value}"))
}

#[async_trait]
impl DnsRepository for SqliteDnsRepository {
    async fn insert_query_log_batch(&self, entries: &[QueryLogRow]) -> anyhow::Result<()> {
        if entries.is_empty() {
            return Ok(());
        }
        let mut tx = self.pools.write.begin().await?;

        let domains = resolve_ids(
            &mut tx,
            LK_DOMAIN,
            &entries.iter().map(|e| e.domain.as_str()).collect(),
        )
        .await?;
        let client_ips = resolve_ids(
            &mut tx,
            LK_CLIENT_IP,
            &entries.iter().map(|e| e.client_ip.as_str()).collect(),
        )
        .await?;
        let query_types = resolve_ids(
            &mut tx,
            LK_QUERY_TYPE,
            &entries.iter().map(|e| e.query_type.as_str()).collect(),
        )
        .await?;
        let results = resolve_ids(
            &mut tx,
            LK_RESULT,
            &entries.iter().map(|e| e.result.as_str()).collect(),
        )
        .await?;
        let protocols = resolve_ids(
            &mut tx,
            LK_PROTOCOL,
            &entries.iter().map(|e| e.protocol.as_str()).collect(),
        )
        .await?;
        let upstreams = resolve_ids(
            &mut tx,
            LK_UPSTREAM,
            &entries
                .iter()
                .filter_map(|e| e.upstream.as_deref())
                .collect(),
        )
        .await?;
        let devices = resolve_ids(
            &mut tx,
            LK_DEVICE,
            &entries
                .iter()
                .filter_map(|e| e.device_id.as_deref())
                .collect(),
        )
        .await?;

        for entry in entries {
            let upstream_id = entry
                .upstream
                .as_deref()
                .map(|v| id_for(&upstreams, LK_UPSTREAM, v))
                .transpose()?;
            let device_id = entry
                .device_id
                .as_deref()
                .map(|v| id_for(&devices, LK_DEVICE, v))
                .transpose()?;

            sqlx::query(
                "INSERT INTO dns_query_log \
                 (timestamp, client_ip_id, domain_id, query_type_id, result_id, upstream_id, \
                 latency_ms, device_id, protocol_id) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(entry.timestamp.timestamp())
            .bind(id_for(&client_ips, LK_CLIENT_IP, &entry.client_ip)?)
            .bind(id_for(&domains, LK_DOMAIN, &entry.domain)?)
            .bind(id_for(&query_types, LK_QUERY_TYPE, &entry.query_type)?)
            .bind(id_for(&results, LK_RESULT, &entry.result)?)
            .bind(upstream_id)
            .bind(entry.latency_ms)
            .bind(device_id)
            .bind(id_for(&protocols, LK_PROTOCOL, &entry.protocol)?)
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
            "SELECT q.timestamp, ip.v AS client_ip, d.v AS domain, qt.v AS query_type, \
             r.v AS result, up.v AS upstream, q.latency_ms, dev.v AS device_id, p.v AS protocol \
             FROM dns_query_log q \
             JOIN {LK_CLIENT_IP} ip ON ip.id = q.client_ip_id \
             JOIN {LK_DOMAIN} d ON d.id = q.domain_id \
             JOIN {LK_QUERY_TYPE} qt ON qt.id = q.query_type_id \
             JOIN {LK_RESULT} r ON r.id = q.result_id \
             JOIN {LK_PROTOCOL} p ON p.id = q.protocol_id \
             LEFT JOIN {LK_UPSTREAM} up ON up.id = q.upstream_id \
             LEFT JOIN {LK_DEVICE} dev ON dev.id = q.device_id \
             {where_clause} \
             ORDER BY q.id DESC LIMIT ? OFFSET ?"
        );

        let mut q = sqlx::query_as::<_, DbQueryLogRow>(sqlx::AssertSqlSafe(sql));
        for b in &binds {
            q = q.bind(b);
        }
        q = q.bind(limit).bind(offset);

        let rows = q.fetch_all(&self.pools.read).await?;
        Ok(rows
            .into_iter()
            .filter_map(DbQueryLogRow::into_row)
            .collect())
    }

    async fn cleanup_query_log(&self, retention_days: u32) -> anyhow::Result<u64> {
        let cutoff = chrono::Utc::now()
            .checked_sub_signed(chrono::Duration::days(i64::from(retention_days)))
            .unwrap_or_else(chrono::Utc::now)
            .timestamp();

        let result = sqlx::query("DELETE FROM dns_query_log WHERE timestamp < ?")
            .bind(cutoff)
            .execute(&self.pools.write)
            .await?;
        let deleted = result.rows_affected();

        // Prune orphaned domains in the same call, immediately after the
        // retention delete — before it, almost nothing is orphaned yet.
        //
        // `NOT IN (SELECT DISTINCT ...)` measured 367 ms; the equivalent
        // correlated `NOT EXISTS` rescans the log once per lookup row and
        // measured 135 s, which would stall the single-connection write pool
        // for two minutes. `lk_dns_domain` is the only lookup pruned: it is the
        // only one that grows without bound, and the only one whose size is
        // load-bearing (substring search scans it).
        let pruned = sqlx::query(sqlx::AssertSqlSafe(format!(
            "DELETE FROM {LK_DOMAIN} WHERE id NOT IN (SELECT DISTINCT domain_id FROM dns_query_log)"
        )))
        .execute(&self.pools.write)
        .await?
        .rows_affected();

        if pruned > 0 {
            tracing::debug!(pruned, "pruned orphaned query-log domains: pruned={pruned}");
        }
        Ok(deleted)
    }
}

fn build_where(filter: &QueryLogFilter) -> (String, Vec<String>) {
    let mut conditions: Vec<String> = Vec::new();
    let mut binds: Vec<String> = Vec::new();
    // Substring match, mirroring the domain filter: the admin UI feeds this
    // from a free-text input, so a partial IP ("192.168.1") must narrow
    // rather than silently matching nothing.
    //
    // Every filter resolves against its lookup table and feeds an indexed
    // integer `IN`. That is what makes substring search cheap: the scan reads
    // the few thousand distinct values, not the millions of log rows.
    if let Some(ref ip) = filter.client_ip {
        conditions.push(format!(
            "q.client_ip_id IN (SELECT id FROM {LK_CLIENT_IP} WHERE v LIKE ?)"
        ));
        binds.push(format!("%{ip}%"));
    }
    if let Some(ref device_id) = filter.device_id {
        conditions.push(format!(
            "q.device_id IN (SELECT id FROM {LK_DEVICE} WHERE v = ?)"
        ));
        binds.push(device_id.clone());
    }
    if let Some(ref domain) = filter.domain {
        conditions.push(format!(
            "q.domain_id IN (SELECT id FROM {LK_DOMAIN} WHERE v LIKE ?)"
        ));
        binds.push(format!("%{domain}%"));
    }
    if let Some(result) = filter.result {
        conditions.push(format!(
            "q.result_id IN (SELECT id FROM {LK_RESULT} WHERE v = ?)"
        ));
        binds.push(result.as_str().to_owned());
    }
    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };
    (where_clause, binds)
}
