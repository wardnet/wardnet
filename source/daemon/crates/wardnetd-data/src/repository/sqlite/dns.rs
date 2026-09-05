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
use crate::repository::dns::{DnsRepository, QueryLogFilter, QueryLogPageRow, QueryLogRow};

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

    /// Resolve a client-IP substring against `lk_dns_client_ip`.
    ///
    /// `None` means the substring matches no client the log has recorded — a
    /// verdict the lookup can give from a few dozen rows, where the log itself
    /// can only give it by reading everything.
    async fn resolve_client_ip(&self, pattern: &str) -> anyhow::Result<Option<ClientIp>> {
        // One past the threshold, so a full read distinguishes "this many" from
        // "at least this many" without counting the rest.
        let ids: Vec<i64> = sqlx::query_scalar(RESOLVE_CLIENT_IP_SQL)
            .bind(format!("%{pattern}%"))
            .bind(
                i64::try_from(CLIENT_IP_ID_BINDS)
                    .unwrap_or(i64::MAX)
                    .saturating_add(1),
            )
            .fetch_all(&self.pools.read)
            .await?;

        Ok(match ids.len() {
            0 => None,
            1 => Some(ClientIp::One(ids[0])),
            n if n <= CLIENT_IP_ID_BINDS => Some(ClientIp::Several(ids)),
            _ => Some(ClientIp::Broad(format!("%{pattern}%"))),
        })
    }
}

/// Resolves a client-IP substring to the lookup ids it matches.
///
/// Fixed text, so it is a `const` rather than a `format!()` over the table
/// name: nothing about this statement varies at run time, unlike the builders
/// below whose placeholder counts follow the filter.
const RESOLVE_CLIENT_IP_SQL: &str = "SELECT id FROM lk_dns_client_ip WHERE v LIKE ? LIMIT ?";

/// How many matched client ids are still worth naming individually.
///
/// The bound is a planning choice before it is a bind-count one. Each named id
/// is an index seek whose results SQLite must then sort into `ORDER BY q.id
/// DESC`; past a few dozen clients that costs more than the backwards
/// primary-key walk it replaces, because a substring matching that many clients
/// also matches most of the log and the walk early-exits at `LIMIT`.
///
/// A household holds far fewer distinct clients than this — 24 on the box ADR
/// 0034 measured — so the bound is a guard against a pathological lookup, not a
/// threshold the admin log reaches.
const CLIENT_IP_ID_BINDS: usize = 64;

#[derive(sqlx::FromRow)]
struct DbQueryLogRow {
    id: i64,
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
    /// An epoch outside `DateTime`'s range falls back to the Unix epoch rather
    /// than dropping the row. Every value this repository writes is in range,
    /// so reaching the fallback means the column was written by something else
    /// — and a visibly wrong 1970 timestamp in a seven-day log is diagnosable,
    /// whereas a silently shorter page makes the caller's `has_more` report
    /// that no further page exists.
    fn into_page_row(self) -> QueryLogPageRow {
        let timestamp = DateTime::from_timestamp(self.timestamp, 0).unwrap_or_else(|| {
            tracing::warn!(
                epoch = self.timestamp,
                "query log row has an out-of-range timestamp: epoch={epoch}",
                epoch = self.timestamp,
            );
            DateTime::UNIX_EPOCH
        });
        QueryLogPageRow {
            id: self.id,
            entry: QueryLogRow {
                timestamp,
                client_ip: self.client_ip,
                domain: self.domain,
                query_type: self.query_type,
                result: self.result,
                upstream: self.upstream,
                latency_ms: self.latency_ms,
                device_id: self.device_id,
                protocol: self.protocol,
            },
        }
    }
}

/// Largest number of distinct values bound in one statement.
///
/// `insert_query_log_batch` is a public trait method, so the batch size is the
/// caller's choice; chunking here means a caller that hands over more distinct
/// values than SQLite will bind gets a few more statements rather than
/// `too many SQL variables` at runtime.
const RESOLVE_CHUNK: usize = 256;

/// Resolve every `values` entry in `table` to its lookup id, inserting the ones
/// that are new.
///
/// Two statements per chunk, never one per row — that is what keeps roughly
/// 1.79M lookups a week off the writer without needing a cache in front of it.
async fn resolve_ids(
    tx: &mut Transaction<'_, Sqlite>,
    table: &str,
    values: &HashSet<&str>,
) -> anyhow::Result<HashMap<String, i64>> {
    let mut ids = HashMap::with_capacity(values.len());
    if values.is_empty() {
        return Ok(ids);
    }
    let all: Vec<&str> = values.iter().copied().collect();
    for chunk in all.chunks(RESOLVE_CHUNK) {
        resolve_chunk(tx, table, chunk, &mut ids).await?;
    }
    Ok(ids)
}

async fn resolve_chunk(
    tx: &mut Transaction<'_, Sqlite>,
    table: &str,
    values: &[&str],
    ids: &mut HashMap<String, i64>,
) -> anyhow::Result<()> {
    let placeholders = vec!["?"; values.len()].join(", ");

    // `ON CONFLICT DO NOTHING` rather than `INSERT OR IGNORE`: the latter also
    // swallows unrelated constraint failures on this table.
    let mut q = sqlx::query(sqlx::AssertSqlSafe(format!(
        "INSERT INTO {table} (v) VALUES {} ON CONFLICT(v) DO NOTHING",
        vec!["(?)"; values.len()].join(", ")
    )));
    for v in values {
        q = q.bind(*v);
    }
    q.execute(&mut **tx).await?;

    let mut q = sqlx::query_as::<_, (i64, String)>(sqlx::AssertSqlSafe(format!(
        "SELECT id, v FROM {table} WHERE v IN ({placeholders})"
    )));
    for v in values {
        q = q.bind(*v);
    }
    for (id, v) in q.fetch_all(&mut **tx).await? {
        ids.insert(v, id);
    }
    Ok(())
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
        before: Option<i64>,
        filter: &QueryLogFilter,
    ) -> anyhow::Result<Vec<QueryLogPageRow>> {
        let client_ip = match filter.client_ip.as_deref() {
            None => None,
            Some(pattern) => match self.resolve_client_ip(pattern).await? {
                Some(term) => Some(term),
                // A substring matching no recorded client cannot match any log
                // row. Answering it here costs a scan of a few dozen lookup
                // rows; leaving it to SQLite costs a pass over every log row
                // the other filters admit, because there is no matching row for
                // `LIMIT` to stop at.
                None => return Ok(Vec::new()),
            },
        };
        let (where_clause, binds) = build_where(filter, client_ip.as_ref(), before);
        let sql = page_sql(&where_clause);

        let mut q = sqlx::query_as::<_, DbQueryLogRow>(sqlx::AssertSqlSafe(sql));
        for b in &binds {
            q = match b {
                Bind::Text(v) => q.bind(v),
                Bind::Int(v) => q.bind(v),
            };
        }
        q = q.bind(limit);

        let rows = q.fetch_all(&self.pools.read).await?;
        Ok(rows.into_iter().map(DbQueryLogRow::into_page_row).collect())
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
        // Use `NOT IN (SELECT DISTINCT ...)`: the equivalent correlated
        // `NOT EXISTS` rescans the log once per lookup row and measured 135 s,
        // which would stall the single-connection write pool for two minutes.
        //
        // `lk_dns_domain` is the only lookup pruned because it is the only one
        // that grows fast enough to matter — ~543 orphans/day against tens or
        // hundreds a year. It is *not* the only one that grows, nor the only
        // one whose size is load-bearing: `lk_dns_client_ip` also accumulates
        // and is also scanned by a `LIKE` in `build_where`. Pruning a second
        // lookup is a live question, not a settled one; see ADR 0034.
        // Nothing but the retention DELETE can orphan a domain, so a tick that
        // removed no rows has nothing to prune and skips the scan — which is
        // every tick on a box younger than its retention window. A prune that
        // failed on an earlier tick is picked up by the next one that deletes,
        // so orphans are deferred rather than stranded.
        if deleted == 0 {
            return Ok(0);
        }

        // The retention DELETE has already committed — it and the prune are
        // separate autocommit statements. A prune failure is therefore reported
        // rather than propagated: returning `Err` here would tell the runner the
        // whole cleanup failed and hide the rows retention did delete, when the
        // only real consequence is that some orphaned domains outlive one tick.
        match sqlx::query(sqlx::AssertSqlSafe(format!(
            "DELETE FROM {LK_DOMAIN} WHERE id NOT IN (SELECT DISTINCT domain_id FROM dns_query_log)"
        )))
        .execute(&self.pools.write)
        .await
        {
            Ok(result) => {
                let pruned = result.rows_affected();
                if pruned > 0 {
                    tracing::debug!(pruned, "pruned orphaned query-log domains: pruned={pruned}");
                }
            }
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "failed to prune orphaned query-log domains; retention delete stands: {e}"
                );
            }
        }
        Ok(deleted)
    }
}

/// The page SELECT, with `where_clause` spliced in as `build_where` produced it.
///
/// Separate from `query_log_paginated` so a test can assert on the plan of the
/// statement the repository actually runs. An `EXPLAIN QUERY PLAN` over a
/// hand-copied approximation of this SQL proves nothing about it: the joins and
/// the ordering are what decide whether a filter's index is used at all, and a
/// copy stays green while the original stops seeking.
pub(crate) fn page_sql(where_clause: &str) -> String {
    format!(
        "SELECT q.id, q.timestamp, ip.v AS client_ip, d.v AS domain, qt.v AS query_type, \
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
         ORDER BY q.id DESC LIMIT ?"
    )
}

/// One bound value in the order its placeholder appears in the WHERE clause.
///
/// The filters mix resolved lookup ids with the patterns that resolve them, and
/// `sqlx` binds positionally, so the builder returns the values as one ordered
/// list rather than one list per type.
pub(crate) enum Bind {
    Text(String),
    Int(i64),
}

/// How the client filter reaches the log query once its substring has been
/// resolved against `lk_dns_client_ip`.
pub(crate) enum ClientIp {
    /// Exactly one client matched. `idx_dns_query_log_client_ip_id` serves both
    /// the filter and `ORDER BY q.id DESC`: under an equality constraint on a
    /// single-column index SQLite walks that key's entries in rowid order, so
    /// the page is a backwards read that stops at `LIMIT`.
    One(i64),
    /// A handful matched. Each id is still an index seek, but SQLite cannot
    /// carry one rowid order across several of them, so it sorts. The sort is
    /// bounded by `LIMIT` and by the matching rows rather than by the table.
    Several(Vec<i64>),
    /// So many matched that naming them buys nothing: a substring this broad
    /// selects most of the log, where the backwards primary-key walk already
    /// early-exits at `LIMIT` and a per-id seek only adds work. The pattern
    /// goes back to the database as a subquery, which also keeps the bind count
    /// off SQLite's variable limit.
    Broad(String),
}

pub(crate) fn build_where(
    filter: &QueryLogFilter,
    client_ip: Option<&ClientIp>,
    before: Option<i64>,
) -> (String, Vec<Bind>) {
    let mut conditions: Vec<String> = Vec::new();
    let mut binds: Vec<Bind> = Vec::new();

    // Keyset, not offset: the page is the newest `limit` rows below the cursor,
    // so every page is the same seek into the primary key. An offset makes
    // SQLite walk and discard every row already read, which is why deep pages
    // degrade — measured 0.21 s at offset 200000 against 0.00 s at the head.
    if let Some(cursor) = before {
        conditions.push("q.id < ?".to_owned());
        binds.push(Bind::Int(cursor));
    }
    // Substring match, mirroring the domain filter: the admin UI feeds this
    // from a free-text input, so a partial IP ("192.168.1") must narrow
    // rather than silently matching nothing.
    //
    // Every filter resolves against its lookup table and feeds an indexed
    // integer comparison. That is what makes substring search cheap: the scan
    // reads one row per distinct value — dozens of clients, thousands of
    // domains — not the millions of log rows.
    match client_ip {
        None => {}
        Some(ClientIp::One(id)) => {
            conditions.push("q.client_ip_id = ?".to_owned());
            binds.push(Bind::Int(*id));
        }
        Some(ClientIp::Several(ids)) => {
            let placeholders = vec!["?"; ids.len()].join(", ");
            conditions.push(format!("q.client_ip_id IN ({placeholders})"));
            binds.extend(ids.iter().map(|id| Bind::Int(*id)));
        }
        Some(ClientIp::Broad(pattern)) => {
            conditions.push(format!(
                "q.client_ip_id IN (SELECT id FROM {LK_CLIENT_IP} WHERE v LIKE ?)"
            ));
            binds.push(Bind::Text(pattern.clone()));
        }
    }
    if let Some(ref device_id) = filter.device_id {
        // Scalar `=`, not `IN`: an IN-list SQLite cannot prove is a single
        // value forces `USE TEMP B-TREE FOR ORDER BY`, which sorts every row
        // for that device before `LIMIT` applies. The scalar form keeps
        // `idx_dns_query_log_device_id` serving the ordering with an early exit.
        conditions.push(format!(
            "q.device_id = (SELECT id FROM {LK_DEVICE} WHERE v = ?)"
        ));
        binds.push(Bind::Text(device_id.clone()));
    }
    if let Some(ref domain) = filter.domain {
        conditions.push(format!(
            "q.domain_id IN (SELECT id FROM {LK_DOMAIN} WHERE v LIKE ?)"
        ));
        binds.push(Bind::Text(format!("%{domain}%")));
    }
    if let Some(result) = filter.result {
        // Scalar, for the same reason as the device filter above.
        conditions.push(format!(
            "q.result_id = (SELECT id FROM {LK_RESULT} WHERE v = ?)"
        ));
        binds.push(Bind::Text(result.as_str().to_owned()));
    }
    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };
    (where_clause, binds)
}
