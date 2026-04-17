use async_trait::async_trait;
use chrono::{NaiveDateTime, TimeZone, Utc};
use sqlx::SqlitePool;
use uuid::Uuid;
use wardnet_common::dns::{AllowlistEntry, Blocklist, CustomFilterRule};

use crate::repository::dns::{
    AllowlistRow, BlocklistRow, BlocklistUpdate, CustomRuleRow, CustomRuleUpdate, DnsRepository,
    QueryLogFilter, QueryLogRow,
};

/// SQLite-backed DNS repository.
pub struct SqliteDnsRepository {
    pool: SqlitePool,
}

impl SqliteDnsRepository {
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

// ── Timestamp helpers ─────────────────────────────────────────────────────

const TS_FMT: &str = "%Y-%m-%dT%H:%M:%SZ";

fn now_iso() -> String {
    Utc::now().format(TS_FMT).to_string()
}

fn parse_ts(s: &str) -> anyhow::Result<chrono::DateTime<Utc>> {
    let naive = NaiveDateTime::parse_from_str(s, TS_FMT)?;
    Ok(Utc.from_utc_datetime(&naive))
}

fn parse_ts_opt(s: Option<&String>) -> anyhow::Result<Option<chrono::DateTime<Utc>>> {
    match s {
        Some(v) => Ok(Some(parse_ts(v)?)),
        None => Ok(None),
    }
}

// ── Internal DB row types ─────────────────────────────────────────────────

#[derive(sqlx::FromRow)]
struct DbBlocklistRow {
    id: String,
    name: String,
    url: String,
    enabled: i64,
    entry_count: i64,
    last_updated: Option<String>,
    cron_schedule: String,
    last_error: Option<String>,
    last_error_at: Option<String>,
    created_at: String,
    updated_at: String,
}

impl DbBlocklistRow {
    fn into_blocklist(self) -> anyhow::Result<Blocklist> {
        Ok(Blocklist {
            id: self.id.parse()?,
            name: self.name,
            url: self.url,
            enabled: self.enabled != 0,
            entry_count: self.entry_count.cast_unsigned(),
            last_updated: parse_ts_opt(self.last_updated.as_ref())?,
            cron_schedule: self.cron_schedule,
            last_error: self.last_error,
            last_error_at: parse_ts_opt(self.last_error_at.as_ref())?,
            created_at: parse_ts(&self.created_at)?,
            updated_at: parse_ts(&self.updated_at)?,
        })
    }
}

#[derive(sqlx::FromRow)]
struct DbAllowlistRow {
    id: String,
    domain: String,
    reason: Option<String>,
    created_at: String,
}

impl DbAllowlistRow {
    fn into_entry(self) -> anyhow::Result<AllowlistEntry> {
        Ok(AllowlistEntry {
            id: self.id.parse()?,
            domain: self.domain,
            reason: self.reason,
            created_at: parse_ts(&self.created_at)?,
        })
    }
}

#[derive(sqlx::FromRow)]
struct DbCustomRuleRow {
    id: String,
    rule_text: String,
    enabled: i64,
    comment: Option<String>,
    created_at: String,
    updated_at: String,
}

impl DbCustomRuleRow {
    fn into_rule(self) -> anyhow::Result<CustomFilterRule> {
        Ok(CustomFilterRule {
            id: self.id.parse()?,
            rule_text: self.rule_text,
            enabled: self.enabled != 0,
            comment: self.comment,
            created_at: parse_ts(&self.created_at)?,
            updated_at: parse_ts(&self.updated_at)?,
        })
    }
}

// ── Query-log row (existing) ──────────────────────────────────────────────

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

// ── Trait implementation ──────────────────────────────────────────────────

#[async_trait]
impl DnsRepository for SqliteDnsRepository {
    // ── Query log ─────────────────────────────────────────────────────────

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
        let mut conditions = Vec::new();
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

        let sql = format!(
            "SELECT timestamp, client_ip, domain, query_type, result, upstream, latency_ms, device_id \
             FROM dns_query_log {where_clause} \
             ORDER BY id DESC LIMIT ? OFFSET ?"
        );

        let mut query = sqlx::query_as::<_, DbQueryLogRow>(&sql);
        for bind in &binds {
            query = query.bind(bind);
        }
        query = query.bind(limit).bind(offset);

        let rows = query.fetch_all(&self.pool).await?;
        Ok(rows.into_iter().map(DbQueryLogRow::into_row).collect())
    }

    async fn query_log_count(&self, filter: &QueryLogFilter) -> anyhow::Result<u64> {
        let mut conditions = Vec::new();
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

        let sql = format!("SELECT COUNT(*) FROM dns_query_log {where_clause}");
        let mut query = sqlx::query_scalar::<_, i64>(&sql);
        for bind in &binds {
            query = query.bind(bind);
        }

        let count = query.fetch_one(&self.pool).await?;
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

    // ── Blocklists ────────────────────────────────────────────────────────

    async fn list_blocklists(&self) -> anyhow::Result<Vec<Blocklist>> {
        let rows = sqlx::query_as::<_, DbBlocklistRow>(
            "SELECT id, name, url, enabled, entry_count, last_updated, cron_schedule, \
             last_error, last_error_at, created_at, updated_at \
             FROM dns_blocklists ORDER BY created_at ASC",
        )
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter()
            .map(DbBlocklistRow::into_blocklist)
            .collect()
    }

    async fn get_blocklist(&self, id: Uuid) -> anyhow::Result<Option<Blocklist>> {
        let id_str = id.to_string();
        let row = sqlx::query_as::<_, DbBlocklistRow>(
            "SELECT id, name, url, enabled, entry_count, last_updated, cron_schedule, \
             last_error, last_error_at, created_at, updated_at \
             FROM dns_blocklists WHERE id = ?",
        )
        .bind(&id_str)
        .fetch_optional(&self.pool)
        .await?;

        row.map(DbBlocklistRow::into_blocklist).transpose()
    }

    async fn create_blocklist(&self, row: &BlocklistRow) -> anyhow::Result<()> {
        let now = now_iso();
        sqlx::query(
            "INSERT INTO dns_blocklists (id, name, url, enabled, cron_schedule, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&row.id)
        .bind(&row.name)
        .bind(&row.url)
        .bind(row.enabled)
        .bind(&row.cron_schedule)
        .bind(&now)
        .bind(&now)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn update_blocklist(&self, id: Uuid, row: &BlocklistUpdate) -> anyhow::Result<()> {
        let mut sets = Vec::new();
        let mut binds: Vec<String> = Vec::new();

        if let Some(ref name) = row.name {
            sets.push("name = ?");
            binds.push(name.clone());
        }
        if let Some(ref url) = row.url {
            sets.push("url = ?");
            binds.push(url.clone());
        }
        if let Some(enabled) = row.enabled {
            sets.push("enabled = ?");
            binds.push(if enabled {
                "1".to_owned()
            } else {
                "0".to_owned()
            });
        }
        if let Some(ref cron) = row.cron_schedule {
            sets.push("cron_schedule = ?");
            binds.push(cron.clone());
        }

        let now = now_iso();
        sets.push("updated_at = ?");
        binds.push(now);

        let id_str = id.to_string();
        binds.push(id_str);

        let sql = format!("UPDATE dns_blocklists SET {} WHERE id = ?", sets.join(", "));

        let mut query = sqlx::query(&sql);
        for bind in &binds {
            query = query.bind(bind);
        }

        query.execute(&self.pool).await?;
        Ok(())
    }

    async fn delete_blocklist(&self, id: Uuid) -> anyhow::Result<bool> {
        let id_str = id.to_string();
        let result = sqlx::query("DELETE FROM dns_blocklists WHERE id = ?")
            .bind(&id_str)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn replace_blocklist_domains(&self, id: Uuid, domains: &[String]) -> anyhow::Result<u64> {
        let id_str = id.to_string();
        let now = now_iso();
        let count = domains.len() as u64;

        let mut tx = self.pool.begin().await?;

        // Delete existing domains for this blocklist.
        sqlx::query("DELETE FROM dns_blocked_domains WHERE blocklist_id = ?")
            .bind(&id_str)
            .execute(&mut *tx)
            .await?;

        // Bulk insert in chunks of 500.
        for chunk in domains.chunks(500) {
            let placeholders: Vec<&str> = chunk.iter().map(|_| "(?, ?)").collect();
            let sql = format!(
                "INSERT INTO dns_blocked_domains (domain, blocklist_id) VALUES {}",
                placeholders.join(", ")
            );
            let mut query = sqlx::query(&sql);
            for domain in chunk {
                query = query.bind(domain).bind(&id_str);
            }
            query.execute(&mut *tx).await?;
        }

        // Update the blocklist metadata.
        sqlx::query(
            "UPDATE dns_blocklists SET entry_count = ?, last_updated = ?, \
             last_error = NULL, last_error_at = NULL, updated_at = ? WHERE id = ?",
        )
        .bind(count.cast_signed())
        .bind(&now)
        .bind(&now)
        .bind(&id_str)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(count)
    }

    async fn list_all_blocked_domains_for_enabled(&self) -> anyhow::Result<Vec<String>> {
        let rows = sqlx::query_scalar::<_, String>(
            "SELECT DISTINCT bd.domain \
             FROM dns_blocked_domains bd \
             JOIN dns_blocklists bl ON bd.blocklist_id = bl.id \
             WHERE bl.enabled = 1",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    async fn set_blocklist_error(&self, id: Uuid, error: Option<&str>) -> anyhow::Result<()> {
        let id_str = id.to_string();
        let now = now_iso();
        match error {
            Some(msg) => {
                sqlx::query(
                    "UPDATE dns_blocklists SET last_error = ?, last_error_at = ?, updated_at = ? \
                     WHERE id = ?",
                )
                .bind(msg)
                .bind(&now)
                .bind(&now)
                .bind(&id_str)
                .execute(&self.pool)
                .await?;
            }
            None => {
                sqlx::query(
                    "UPDATE dns_blocklists SET last_error = NULL, last_error_at = NULL, \
                     updated_at = ? WHERE id = ?",
                )
                .bind(&now)
                .bind(&id_str)
                .execute(&self.pool)
                .await?;
            }
        }
        Ok(())
    }

    // ── Allowlist ─────────────────────────────────────────────────────────

    async fn list_allowlist(&self) -> anyhow::Result<Vec<AllowlistEntry>> {
        let rows = sqlx::query_as::<_, DbAllowlistRow>(
            "SELECT id, domain, reason, created_at FROM dns_allowlist ORDER BY created_at ASC",
        )
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(DbAllowlistRow::into_entry).collect()
    }

    async fn create_allowlist_entry(&self, row: &AllowlistRow) -> anyhow::Result<()> {
        let now = now_iso();
        sqlx::query(
            "INSERT INTO dns_allowlist (id, domain, reason, created_at) VALUES (?, ?, ?, ?)",
        )
        .bind(&row.id)
        .bind(&row.domain)
        .bind(&row.reason)
        .bind(&now)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn delete_allowlist_entry(&self, id: Uuid) -> anyhow::Result<bool> {
        let id_str = id.to_string();
        let result = sqlx::query("DELETE FROM dns_allowlist WHERE id = ?")
            .bind(&id_str)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    // ── Custom rules ──────────────────────────────────────────────────────

    async fn list_custom_rules(&self) -> anyhow::Result<Vec<CustomFilterRule>> {
        let rows = sqlx::query_as::<_, DbCustomRuleRow>(
            "SELECT id, rule_text, enabled, comment, created_at, updated_at \
             FROM dns_custom_rules ORDER BY created_at ASC",
        )
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(DbCustomRuleRow::into_rule).collect()
    }

    async fn get_custom_rule(&self, id: Uuid) -> anyhow::Result<Option<CustomFilterRule>> {
        let id_str = id.to_string();
        let row = sqlx::query_as::<_, DbCustomRuleRow>(
            "SELECT id, rule_text, enabled, comment, created_at, updated_at \
             FROM dns_custom_rules WHERE id = ?",
        )
        .bind(&id_str)
        .fetch_optional(&self.pool)
        .await?;

        row.map(DbCustomRuleRow::into_rule).transpose()
    }

    async fn create_custom_rule(&self, row: &CustomRuleRow) -> anyhow::Result<()> {
        let now = now_iso();
        sqlx::query(
            "INSERT INTO dns_custom_rules (id, rule_text, enabled, comment, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(&row.id)
        .bind(&row.rule_text)
        .bind(row.enabled)
        .bind(&row.comment)
        .bind(&now)
        .bind(&now)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn update_custom_rule(&self, id: Uuid, row: &CustomRuleUpdate) -> anyhow::Result<()> {
        let mut sets = Vec::new();
        let mut binds: Vec<String> = Vec::new();

        if let Some(ref rule_text) = row.rule_text {
            sets.push("rule_text = ?");
            binds.push(rule_text.clone());
        }
        if let Some(enabled) = row.enabled {
            sets.push("enabled = ?");
            binds.push(if enabled {
                "1".to_owned()
            } else {
                "0".to_owned()
            });
        }
        if let Some(ref comment) = row.comment {
            sets.push("comment = ?");
            binds.push(comment.clone());
        }

        let now = now_iso();
        sets.push("updated_at = ?");
        binds.push(now);

        let id_str = id.to_string();
        binds.push(id_str);

        let sql = format!(
            "UPDATE dns_custom_rules SET {} WHERE id = ?",
            sets.join(", ")
        );

        let mut query = sqlx::query(&sql);
        for bind in &binds {
            query = query.bind(bind);
        }

        query.execute(&self.pool).await?;
        Ok(())
    }

    async fn delete_custom_rule(&self, id: Uuid) -> anyhow::Result<bool> {
        let id_str = id.to_string();
        let result = sqlx::query("DELETE FROM dns_custom_rules WHERE id = ?")
            .bind(&id_str)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }
}
