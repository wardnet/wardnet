//! SQLite-backed [`DnsLocalRepository`] implementation.

use async_trait::async_trait;
use chrono::{NaiveDateTime, TimeZone, Utc};
use sqlx::SqlitePool;
use uuid::Uuid;

use wardnet_common::dns::{
    ConditionalForwardingRule, CustomDnsRecord, DnsRecordSource, DnsRecordType, DnsZone,
    DnsZoneSource,
};

use crate::db::DbPools;
use crate::repository::dns_local::{
    DnsLocalRepository, RecordRow, RecordUpdate, RuleRow, RuleUpdate, UpsertRecordRow, ZoneRow,
    ZoneUpdate,
};

const TS_FMT: &str = "%Y-%m-%dT%H:%M:%SZ";

fn now_iso() -> String {
    Utc::now().format(TS_FMT).to_string()
}

fn parse_ts(s: &str) -> anyhow::Result<chrono::DateTime<Utc>> {
    let naive = NaiveDateTime::parse_from_str(s, TS_FMT)?;
    Ok(Utc.from_utc_datetime(&naive))
}

/// Map a [`DnsRecordType`] to the `SCREAMING_SNAKE` string stored in the
/// `dns_custom_records.record_type` column (matches the enum's serde repr).
fn record_type_to_db(rt: DnsRecordType) -> &'static str {
    match rt {
        DnsRecordType::A => "A",
        DnsRecordType::Aaaa => "AAAA",
        DnsRecordType::Cname => "CNAME",
        DnsRecordType::Txt => "TXT",
        DnsRecordType::Mx => "MX",
        DnsRecordType::Srv => "SRV",
    }
}

/// Inverse of [`record_type_to_db`].
fn record_type_from_db(s: &str) -> anyhow::Result<DnsRecordType> {
    Ok(match s {
        "A" => DnsRecordType::A,
        "AAAA" => DnsRecordType::Aaaa,
        "CNAME" => DnsRecordType::Cname,
        "TXT" => DnsRecordType::Txt,
        "MX" => DnsRecordType::Mx,
        "SRV" => DnsRecordType::Srv,
        other => anyhow::bail!("unknown DNS record type in database: {other}"),
    })
}

/// Map a [`DnsRecordSource`] to the `snake_case` string stored in the
/// `dns_custom_records.source` column (matches the enum's serde repr).
fn source_to_db(source: DnsRecordSource) -> &'static str {
    match source {
        DnsRecordSource::Manual => "manual",
        DnsRecordSource::Dhcp => "dhcp",
        DnsRecordSource::System => "system",
    }
}

/// Inverse of [`source_to_db`]. Unknown values fall back to `Manual` so a
/// stray row never fails the whole list query.
fn source_from_db(s: &str) -> DnsRecordSource {
    match s {
        "dhcp" => DnsRecordSource::Dhcp,
        "system" => DnsRecordSource::System,
        _ => DnsRecordSource::Manual,
    }
}

/// Map a [`DnsZoneSource`] to the `snake_case` string stored in the
/// `dns_zones.source` column (matches the enum's serde repr).
fn zone_source_to_db(source: DnsZoneSource) -> &'static str {
    match source {
        DnsZoneSource::Manual => "manual",
        DnsZoneSource::System => "system",
    }
}

/// Inverse of [`zone_source_to_db`]. Unknown values fall back to `Manual` so a
/// stray row never fails the whole list query.
fn zone_source_from_db(s: &str) -> DnsZoneSource {
    match s {
        "system" => DnsZoneSource::System,
        _ => DnsZoneSource::Manual,
    }
}

/// SQLite-backed local-DNS repository.
pub struct SqliteDnsLocalRepository {
    pools: DbPools,
}

impl SqliteDnsLocalRepository {
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self::new_pools(DbPools::single(pool))
    }

    #[must_use]
    pub fn new_pools(pools: DbPools) -> Self {
        Self { pools }
    }
}

// ── Internal row types ──────────────────────────────────────────────────────

#[derive(sqlx::FromRow)]
struct DbZoneRow {
    id: String,
    name: String,
    enabled: i64,
    source: String,
    created_at: String,
    updated_at: String,
}

impl DbZoneRow {
    fn into_zone(self) -> anyhow::Result<DnsZone> {
        Ok(DnsZone {
            id: self.id.parse()?,
            name: self.name,
            enabled: self.enabled != 0,
            source: zone_source_from_db(&self.source),
            created_at: parse_ts(&self.created_at)?,
            updated_at: parse_ts(&self.updated_at)?,
        })
    }
}

#[derive(sqlx::FromRow)]
struct DbRecordRow {
    id: String,
    zone_id: Option<String>,
    domain: String,
    record_type: String,
    value: String,
    ttl: i64,
    enabled: i64,
    source: String,
    created_at: String,
    updated_at: String,
}

impl DbRecordRow {
    fn into_record(self) -> anyhow::Result<CustomDnsRecord> {
        Ok(CustomDnsRecord {
            id: self.id.parse()?,
            zone_id: self.zone_id.map(|z| z.parse()).transpose()?,
            domain: self.domain,
            record_type: record_type_from_db(&self.record_type)?,
            value: self.value,
            ttl: u32::try_from(self.ttl)?,
            enabled: self.enabled != 0,
            source: source_from_db(&self.source),
            created_at: parse_ts(&self.created_at)?,
            updated_at: parse_ts(&self.updated_at)?,
        })
    }
}

#[derive(sqlx::FromRow)]
struct DbRuleRow {
    id: String,
    domain: String,
    upstream: String,
    enabled: i64,
    created_at: String,
}

impl DbRuleRow {
    fn into_rule(self) -> anyhow::Result<ConditionalForwardingRule> {
        Ok(ConditionalForwardingRule {
            id: self.id.parse()?,
            domain: self.domain,
            upstream: self.upstream,
            enabled: self.enabled != 0,
            created_at: parse_ts(&self.created_at)?,
        })
    }
}

// Static SQL strings — all column lists are inlined so no runtime format!() is needed
// for the fixed SELECT/GET queries. UPDATE queries use AssertSqlSafe because their SET
// clauses are dynamically assembled from static &str fragments (no user input is interpolated).

const LIST_ZONES_SQL: &str =
    "SELECT id, name, enabled, source, created_at, updated_at FROM dns_zones ORDER BY name ASC";
const GET_ZONE_SQL: &str =
    "SELECT id, name, enabled, source, created_at, updated_at FROM dns_zones WHERE id = ?";

const LIST_RECORDS_SQL: &str = "SELECT id, zone_id, domain, record_type, value, ttl, enabled, source, created_at, updated_at \
     FROM dns_custom_records ORDER BY domain ASC";
const LIST_RECORDS_BY_ZONE_SQL: &str = "SELECT id, zone_id, domain, record_type, value, ttl, enabled, source, created_at, updated_at \
     FROM dns_custom_records WHERE zone_id = ? ORDER BY domain ASC";
const GET_RECORD_SQL: &str = "SELECT id, zone_id, domain, record_type, value, ttl, enabled, source, created_at, updated_at \
     FROM dns_custom_records WHERE id = ?";

const LIST_RULES_SQL: &str = "SELECT id, domain, upstream, enabled, created_at \
     FROM dns_conditional_rules ORDER BY domain ASC";
const GET_RULE_SQL: &str = "SELECT id, domain, upstream, enabled, created_at \
     FROM dns_conditional_rules WHERE id = ?";

#[async_trait]
impl DnsLocalRepository for SqliteDnsLocalRepository {
    // ── Zones ───────────────────────────────────────────────────────────

    async fn list_zones(&self) -> anyhow::Result<Vec<DnsZone>> {
        let rows = sqlx::query_as::<_, DbZoneRow>(LIST_ZONES_SQL)
            .fetch_all(&self.pools.read)
            .await?;
        rows.into_iter().map(DbZoneRow::into_zone).collect()
    }

    async fn get_zone(&self, id: Uuid) -> anyhow::Result<Option<DnsZone>> {
        let row = sqlx::query_as::<_, DbZoneRow>(GET_ZONE_SQL)
            .bind(id.to_string())
            .fetch_optional(&self.pools.read)
            .await?;
        row.map(DbZoneRow::into_zone).transpose()
    }

    async fn create_zone(&self, row: &ZoneRow) -> anyhow::Result<DnsZone> {
        let now = now_iso();
        sqlx::query(
            "INSERT INTO dns_zones (id, name, enabled, source, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(&row.id)
        .bind(&row.name)
        .bind(row.enabled)
        .bind(zone_source_to_db(row.source))
        .bind(&now)
        .bind(&now)
        .execute(&self.pools.write)
        .await?;
        Ok(DnsZone {
            id: row.id.parse()?,
            name: row.name.clone(),
            enabled: row.enabled,
            source: row.source,
            created_at: parse_ts(&now)?,
            updated_at: parse_ts(&now)?,
        })
    }

    async fn update_zone(&self, id: Uuid, update: &ZoneUpdate) -> anyhow::Result<Option<DnsZone>> {
        let now = now_iso();
        // Always touch updated_at so the UPDATE reliably tells us whether the row exists.
        let mut set_parts: Vec<&str> = vec!["updated_at = ?"];
        if update.name.is_some() {
            set_parts.push("name = ?");
        }
        if update.enabled.is_some() {
            set_parts.push("enabled = ?");
        }
        // SET clause built from &'static str fragments only — no user input is interpolated.
        let sql = format!("UPDATE dns_zones SET {} WHERE id = ?", set_parts.join(", "));
        let mut q = sqlx::query(sqlx::AssertSqlSafe(sql)).bind(&now);
        if let Some(ref name) = update.name {
            q = q.bind(name);
        }
        if let Some(enabled) = update.enabled {
            q = q.bind(enabled);
        }
        q = q.bind(id.to_string());
        let result = q.execute(&self.pools.write).await?;
        if result.rows_affected() == 0 {
            return Ok(None);
        }
        let row = sqlx::query_as::<_, DbZoneRow>(GET_ZONE_SQL)
            .bind(id.to_string())
            .fetch_optional(&self.pools.write)
            .await?;
        row.map(DbZoneRow::into_zone).transpose()
    }

    async fn delete_zone(&self, id: Uuid) -> anyhow::Result<bool> {
        let result = sqlx::query("DELETE FROM dns_zones WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pools.write)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    // ── Custom records ──────────────────────────────────────────────────

    async fn list_records(&self) -> anyhow::Result<Vec<CustomDnsRecord>> {
        let rows = sqlx::query_as::<_, DbRecordRow>(LIST_RECORDS_SQL)
            .fetch_all(&self.pools.read)
            .await?;
        rows.into_iter().map(DbRecordRow::into_record).collect()
    }

    async fn list_records_by_zone(&self, zone_id: Uuid) -> anyhow::Result<Vec<CustomDnsRecord>> {
        let rows = sqlx::query_as::<_, DbRecordRow>(LIST_RECORDS_BY_ZONE_SQL)
            .bind(zone_id.to_string())
            .fetch_all(&self.pools.read)
            .await?;
        rows.into_iter().map(DbRecordRow::into_record).collect()
    }

    async fn get_record(&self, id: Uuid) -> anyhow::Result<Option<CustomDnsRecord>> {
        let row = sqlx::query_as::<_, DbRecordRow>(GET_RECORD_SQL)
            .bind(id.to_string())
            .fetch_optional(&self.pools.read)
            .await?;
        row.map(DbRecordRow::into_record).transpose()
    }

    async fn create_record(&self, row: &RecordRow) -> anyhow::Result<CustomDnsRecord> {
        let now = now_iso();
        let zone_id = row.zone_id.map(|z| z.to_string());
        sqlx::query(
            "INSERT INTO dns_custom_records \
                 (id, zone_id, domain, record_type, value, ttl, enabled, source, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&row.id)
        .bind(&zone_id)
        .bind(&row.domain)
        .bind(record_type_to_db(row.record_type))
        .bind(&row.value)
        .bind(i64::from(row.ttl))
        .bind(row.enabled)
        .bind(source_to_db(row.source))
        .bind(&now)
        .bind(&now)
        .execute(&self.pools.write)
        .await?;
        Ok(CustomDnsRecord {
            id: row.id.parse()?,
            zone_id: row.zone_id,
            domain: row.domain.clone(),
            record_type: row.record_type,
            value: row.value.clone(),
            ttl: row.ttl,
            enabled: row.enabled,
            source: row.source,
            created_at: parse_ts(&now)?,
            updated_at: parse_ts(&now)?,
        })
    }

    async fn upsert_record(
        &self,
        domain: &str,
        record_type: DnsRecordType,
        row: &UpsertRecordRow,
    ) -> anyhow::Result<Option<CustomDnsRecord>> {
        let now = now_iso();
        let id = Uuid::new_v4().to_string();
        let zone_id = row.zone_id.map(|z| z.to_string());
        // Conflict target is the UNIQUE(domain, record_type) index. On insert a
        // fresh id is used; on conflict the existing row keeps its id while the
        // mutable columns are overwritten — but the `WHERE source = excluded.source`
        // guard means an upsert only overwrites a row of the *same* provenance.
        // A DHCP upsert therefore cannot clobber a `manual`/`system` record; the
        // guarded UPDATE matches zero rows, RETURNING yields nothing, and we
        // return `Ok(None)`. created_at is left untouched on conflict. RETURNING
        // gives back the canonical stored row (mapped by column name via FromRow),
        // so the preserved id surfaces correctly on the renew path.
        let db_row = sqlx::query_as::<_, DbRecordRow>(
            "INSERT INTO dns_custom_records \
                 (id, zone_id, domain, record_type, value, ttl, enabled, source, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
             ON CONFLICT(domain, record_type) DO UPDATE SET \
                 zone_id = excluded.zone_id, \
                 value = excluded.value, \
                 ttl = excluded.ttl, \
                 enabled = excluded.enabled, \
                 source = excluded.source, \
                 updated_at = excluded.updated_at \
             WHERE dns_custom_records.source = excluded.source \
             RETURNING id, zone_id, domain, record_type, value, ttl, enabled, source, created_at, updated_at",
        )
        .bind(&id)
        .bind(&zone_id)
        .bind(domain)
        .bind(record_type_to_db(record_type))
        .bind(&row.value)
        .bind(i64::from(row.ttl))
        .bind(row.enabled)
        .bind(source_to_db(row.source))
        .bind(&now)
        .bind(&now)
        .fetch_optional(&self.pools.write)
        .await?;
        db_row.map(DbRecordRow::into_record).transpose()
    }

    async fn update_record(
        &self,
        id: Uuid,
        update: &RecordUpdate,
    ) -> anyhow::Result<Option<CustomDnsRecord>> {
        let RecordUpdate {
            zone_id,
            domain,
            record_type,
            value,
            ttl,
            enabled,
        } = update;
        let now = now_iso();
        let mut set_parts: Vec<&str> = vec!["updated_at = ?"];
        if zone_id.is_some() {
            set_parts.push("zone_id = ?");
        }
        if domain.is_some() {
            set_parts.push("domain = ?");
        }
        if record_type.is_some() {
            set_parts.push("record_type = ?");
        }
        if value.is_some() {
            set_parts.push("value = ?");
        }
        if ttl.is_some() {
            set_parts.push("ttl = ?");
        }
        if enabled.is_some() {
            set_parts.push("enabled = ?");
        }
        // SET clause built from &'static str fragments only — no user input is interpolated.
        let sql = format!(
            "UPDATE dns_custom_records SET {} WHERE id = ?",
            set_parts.join(", ")
        );
        let mut q = sqlx::query(sqlx::AssertSqlSafe(sql)).bind(&now);
        if let Some(z) = zone_id {
            // Some(None) → binds SQL NULL, clearing the zone link.
            q = q.bind(z.map(|u| u.to_string()));
        }
        if let Some(d) = domain {
            q = q.bind(d);
        }
        if let Some(rt) = record_type {
            q = q.bind(record_type_to_db(*rt));
        }
        if let Some(v) = value {
            q = q.bind(v);
        }
        if let Some(t) = ttl {
            q = q.bind(i64::from(*t));
        }
        if let Some(e) = enabled {
            q = q.bind(*e);
        }
        q = q.bind(id.to_string());
        let result = q.execute(&self.pools.write).await?;
        if result.rows_affected() == 0 {
            return Ok(None);
        }
        let row = sqlx::query_as::<_, DbRecordRow>(GET_RECORD_SQL)
            .bind(id.to_string())
            .fetch_optional(&self.pools.write)
            .await?;
        row.map(DbRecordRow::into_record).transpose()
    }

    async fn delete_record(&self, id: Uuid) -> anyhow::Result<bool> {
        let result = sqlx::query("DELETE FROM dns_custom_records WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pools.write)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    // ── Conditional forwarding rules ────────────────────────────────────

    async fn list_rules(&self) -> anyhow::Result<Vec<ConditionalForwardingRule>> {
        let rows = sqlx::query_as::<_, DbRuleRow>(LIST_RULES_SQL)
            .fetch_all(&self.pools.read)
            .await?;
        rows.into_iter().map(DbRuleRow::into_rule).collect()
    }

    async fn get_rule(&self, id: Uuid) -> anyhow::Result<Option<ConditionalForwardingRule>> {
        let row = sqlx::query_as::<_, DbRuleRow>(GET_RULE_SQL)
            .bind(id.to_string())
            .fetch_optional(&self.pools.read)
            .await?;
        row.map(DbRuleRow::into_rule).transpose()
    }

    async fn create_rule(&self, row: &RuleRow) -> anyhow::Result<ConditionalForwardingRule> {
        let now = now_iso();
        sqlx::query(
            "INSERT INTO dns_conditional_rules (id, domain, upstream, enabled, created_at) \
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&row.id)
        .bind(&row.domain)
        .bind(&row.upstream)
        .bind(row.enabled)
        .bind(&now)
        .execute(&self.pools.write)
        .await?;
        Ok(ConditionalForwardingRule {
            id: row.id.parse()?,
            domain: row.domain.clone(),
            upstream: row.upstream.clone(),
            enabled: row.enabled,
            created_at: parse_ts(&now)?,
        })
    }

    async fn update_rule(
        &self,
        id: Uuid,
        update: &RuleUpdate,
    ) -> anyhow::Result<Option<ConditionalForwardingRule>> {
        // dns_conditional_rules has no updated_at column. When all fields are None,
        // do an existence check rather than emitting invalid SQL with an empty SET clause.
        if update.domain.is_none() && update.upstream.is_none() && update.enabled.is_none() {
            let exists: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM dns_conditional_rules WHERE id = ?)",
            )
            .bind(id.to_string())
            .fetch_one(&self.pools.read)
            .await?;
            if !exists {
                return Ok(None);
            }
            let row = sqlx::query_as::<_, DbRuleRow>(GET_RULE_SQL)
                .bind(id.to_string())
                .fetch_optional(&self.pools.read)
                .await?;
            return row.map(DbRuleRow::into_rule).transpose();
        }
        let mut set_parts: Vec<&str> = Vec::new();
        if update.domain.is_some() {
            set_parts.push("domain = ?");
        }
        if update.upstream.is_some() {
            set_parts.push("upstream = ?");
        }
        if update.enabled.is_some() {
            set_parts.push("enabled = ?");
        }
        // SET clause built from &'static str fragments only — no user input is interpolated.
        let sql = format!(
            "UPDATE dns_conditional_rules SET {} WHERE id = ?",
            set_parts.join(", ")
        );
        let mut q = sqlx::query(sqlx::AssertSqlSafe(sql));
        if let Some(ref domain) = update.domain {
            q = q.bind(domain);
        }
        if let Some(ref upstream) = update.upstream {
            q = q.bind(upstream);
        }
        if let Some(enabled) = update.enabled {
            q = q.bind(enabled);
        }
        q = q.bind(id.to_string());
        let result = q.execute(&self.pools.write).await?;
        if result.rows_affected() == 0 {
            return Ok(None);
        }
        let row = sqlx::query_as::<_, DbRuleRow>(GET_RULE_SQL)
            .bind(id.to_string())
            .fetch_optional(&self.pools.write)
            .await?;
        row.map(DbRuleRow::into_rule).transpose()
    }

    async fn delete_rule(&self, id: Uuid) -> anyhow::Result<bool> {
        let result = sqlx::query("DELETE FROM dns_conditional_rules WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pools.write)
            .await?;
        Ok(result.rows_affected() > 0)
    }
}
