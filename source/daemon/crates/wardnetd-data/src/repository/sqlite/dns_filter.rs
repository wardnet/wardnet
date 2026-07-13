//! SQLite-backed [`DnsFilterRepository`] implementation.

use async_trait::async_trait;
use chrono::{NaiveDateTime, TimeZone, Utc};
use sqlx::SqlitePool;
use uuid::Uuid;

use wardnet_common::dns::{AllowlistEntry, Blocklist, CustomFilterRule};
use wardnet_common::dns_filter::{DeviceDnsFilterSettings, DnsFilterConfig, DnsFilterProfile};

use crate::db::DbPools;
use crate::repository::dns_filter::{
    AllowlistRow, BlocklistRow, BlocklistUpdate, CustomRuleRow, CustomRuleUpdate,
    DeviceSettingsRow, DeviceSettingsWithIp, DnsFilterRepository, ProfileFilterInputs,
};

const TS_FMT: &str = "%Y-%m-%dT%H:%M:%SZ";
const KEY_DNS_FILTERING_ENABLED: &str = "dns_filtering_enabled";

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

/// SQLite-backed DNS filter repository.
pub struct SqliteDnsFilterRepository {
    pools: DbPools,
}

/// Rows written per `INSERT` statement. Bounded by `SQLite`'s bind-variable
/// limit (3 binds per row here).
const INSERT_CHUNK_ROWS: usize = 500;

/// Rows per import transaction. The sole write connection is held for one
/// batch and then released, so this is the granularity at which every other
/// writer in the daemon gets a turn — it bounds how long DHCP, stats or a
/// health probe can be stuck behind an import. A Pi sustains ~145k rows/sec
/// on SD, so 2k rows is a hold of tens of milliseconds; the ~950 extra
/// commits a 1.9M-domain feed costs are cheap under WAL + synchronous=NORMAL
/// (no fsync per commit).
const IMPORT_BATCH_ROWS: usize = 2_000;

/// Rows per cleanup transaction when reclaiming a superseded generation.
const DELETE_BATCH_ROWS: i64 = 2_000;

/// How many times `delete_profile` re-reads and drains its blocklists before
/// giving up and letting the cascade handle the remainder.
const DRAIN_MAX_PASSES: u32 = 5;

/// Which of a blocklist's domain rows [`delete_domains_in_batches`] should
/// remove.
#[derive(Clone, Copy)]
enum DomainScope {
    /// Every row, whatever its generation — used before dropping the blocklist.
    All,
    /// One specific generation — clears debris left at the generation we are
    /// about to write.
    Generation(i64),
    /// Everything the given generation supersedes. Note this is "all except",
    /// not "the previous one": a daemon killed between the flip and the cleanup
    /// leaves an orphaned generation that nothing else would ever reclaim.
    AllExcept(i64),
}

impl DomainScope {
    /// SQL predicate appended to the `WHERE blocklist_id = ?` filter, plus the
    /// generation it binds (if any). Kept together so the clause and its bind
    /// can never drift apart.
    fn predicate(self) -> (&'static str, Option<i64>) {
        match self {
            Self::All => ("", None),
            Self::Generation(g) => ("AND generation = ?", Some(g)),
            Self::AllExcept(g) => ("AND generation <> ?", Some(g)),
        }
    }
}

/// Delete a blocklist's domains a bounded chunk per transaction, so the sole
/// write connection is never held for long.
///
/// Draining is what makes deletion safe: `blocklist_id` is `ON DELETE CASCADE`,
/// so dropping a blocklist row with millions of domains still attached would
/// make `SQLite` delete them all inside one implicit transaction — the same
/// writer monopolisation the batched import exists to avoid.
///
/// Returns the number of rows removed.
async fn delete_domains_in_batches(
    pool: &SqlitePool,
    blocklist_id: &str,
    scope: DomainScope,
) -> anyhow::Result<u64> {
    let (predicate, generation) = scope.predicate();
    let sql = format!(
        "DELETE FROM dns_filter_blocked_domains \
         WHERE rowid IN ( \
             SELECT rowid FROM dns_filter_blocked_domains \
             WHERE blocklist_id = ? {predicate} LIMIT ? \
         )"
    );

    let mut removed = 0_u64;
    loop {
        let mut q = sqlx::query(sqlx::AssertSqlSafe(sql.clone())).bind(blocklist_id);
        if let Some(g) = generation {
            q = q.bind(g);
        }
        let affected = q
            .bind(DELETE_BATCH_ROWS)
            .execute(pool)
            .await?
            .rows_affected();

        removed += affected;
        if affected == 0 {
            return Ok(removed);
        }
        tokio::task::yield_now().await;
    }
}

impl SqliteDnsFilterRepository {
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self::new_pools(DbPools::single(pool))
    }

    #[must_use]
    pub fn new_pools(pools: DbPools) -> Self {
        Self { pools }
    }
}

// ── Internal row types ────────────────────────────────────────────────────

#[derive(sqlx::FromRow)]
struct DbProfileRow {
    id: String,
    name: String,
    description: Option<String>,
    builtin: i64,
    created_at: String,
    updated_at: String,
}

impl DbProfileRow {
    fn into_profile(self) -> anyhow::Result<DnsFilterProfile> {
        Ok(DnsFilterProfile {
            id: self.id.parse()?,
            name: self.name,
            description: self.description,
            builtin: self.builtin != 0,
            created_at: parse_ts(&self.created_at)?,
            updated_at: parse_ts(&self.updated_at)?,
        })
    }
}

#[derive(sqlx::FromRow)]
struct DbBlocklistRow {
    id: String,
    profile_id: String,
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
            profile_id: self.profile_id.parse()?,
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
    profile_id: String,
    domain: String,
    reason: Option<String>,
    created_at: String,
}

impl DbAllowlistRow {
    fn into_entry(self) -> anyhow::Result<AllowlistEntry> {
        Ok(AllowlistEntry {
            id: self.id.parse()?,
            profile_id: self.profile_id.parse()?,
            domain: self.domain,
            reason: self.reason,
            created_at: parse_ts(&self.created_at)?,
        })
    }
}

#[derive(sqlx::FromRow)]
struct DbCustomRuleRow {
    id: String,
    profile_id: String,
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
            profile_id: self.profile_id.parse()?,
            rule_text: self.rule_text,
            enabled: self.enabled != 0,
            comment: self.comment,
            created_at: parse_ts(&self.created_at)?,
            updated_at: parse_ts(&self.updated_at)?,
        })
    }
}

#[async_trait]
impl DnsFilterRepository for SqliteDnsFilterRepository {
    // ── Profiles ────────────────────────────────────────────────────────

    async fn list_profiles(&self) -> anyhow::Result<Vec<DnsFilterProfile>> {
        let rows = sqlx::query_as::<_, DbProfileRow>(
            "SELECT id, name, description, builtin, created_at, updated_at \
             FROM dns_filter_profiles ORDER BY builtin DESC, name ASC",
        )
        .fetch_all(&self.pools.read)
        .await?;
        rows.into_iter().map(DbProfileRow::into_profile).collect()
    }

    async fn get_profile(&self, id: Uuid) -> anyhow::Result<Option<DnsFilterProfile>> {
        let id_str = id.to_string();
        let row = sqlx::query_as::<_, DbProfileRow>(
            "SELECT id, name, description, builtin, created_at, updated_at \
             FROM dns_filter_profiles WHERE id = ?",
        )
        .bind(&id_str)
        .fetch_optional(&self.pools.read)
        .await?;
        row.map(DbProfileRow::into_profile).transpose()
    }

    async fn create_profile(
        &self,
        id: Uuid,
        name: &str,
        description: Option<&str>,
    ) -> anyhow::Result<DnsFilterProfile> {
        let now = now_iso();
        let id_str = id.to_string();
        sqlx::query(
            "INSERT INTO dns_filter_profiles (id, name, description, builtin, created_at, updated_at) \
             VALUES (?, ?, ?, 0, ?, ?)",
        )
        .bind(&id_str)
        .bind(name)
        .bind(description)
        .bind(&now)
        .bind(&now)
        .execute(&self.pools.write)
        .await?;
        Ok(DnsFilterProfile {
            id,
            name: name.to_owned(),
            description: description.map(str::to_owned),
            builtin: false,
            created_at: parse_ts(&now)?,
            updated_at: parse_ts(&now)?,
        })
    }

    async fn update_profile_fields(
        &self,
        id: Uuid,
        name: Option<&str>,
        description: Option<Option<&str>>,
    ) -> anyhow::Result<bool> {
        if name.is_none() && description.is_none() {
            // Nothing to update — not an error, return true (row exists).
            return Ok(true);
        }
        let now = now_iso();
        let id_str = id.to_string();
        // Build the SET clause dynamically so we only touch the fields the
        // caller asked to change, preserving the others unchanged.
        let mut set_parts: Vec<&str> = vec!["updated_at = ?"];
        if name.is_some() {
            set_parts.push("name = ?");
        }
        if description.is_some() {
            set_parts.push("description = ?");
        }
        let sql = format!(
            "UPDATE dns_filter_profiles SET {} WHERE id = ?",
            set_parts.join(", ")
        );
        let mut q = sqlx::query(sqlx::AssertSqlSafe(sql)).bind(&now);
        if let Some(n) = name {
            q = q.bind(n);
        }
        if let Some(d) = description {
            q = q.bind(d); // binds Option<&str>: None → SQL NULL
        }
        q = q.bind(&id_str);
        let result = q.execute(&self.pools.write).await?;
        Ok(result.rows_affected() > 0)
    }

    async fn delete_profile(&self, id: Uuid) -> anyhow::Result<bool> {
        let id_str = id.to_string();

        // Deleting a profile cascades into its blocklists, which cascade into
        // their domains. Drain those in batches first, for the same reason
        // `delete_blocklist` does — but only for a profile we can actually
        // delete, so a rejected builtin isn't stripped of its domains.
        let deletable: Option<String> =
            sqlx::query_scalar("SELECT id FROM dns_filter_profiles WHERE id = ? AND builtin = 0")
                .bind(&id_str)
                .fetch_optional(&self.pools.read)
                .await?;
        if deletable.is_none() {
            return Ok(false);
        }

        // Re-read the blocklist set each pass: one taken up-front would miss a
        // blocklist created (or freshly imported into) while we drain, and its
        // domains would then cascade away in one implicit transaction — the
        // starvation we are here to avoid. Converges once a pass removes
        // nothing; bounded so a caller hammering this profile cannot spin us
        // forever.
        for pass in 0..DRAIN_MAX_PASSES {
            let blocklist_ids: Vec<String> =
                sqlx::query_scalar("SELECT id FROM dns_filter_blocklists WHERE profile_id = ?")
                    .bind(&id_str)
                    .fetch_all(&self.pools.read)
                    .await?;

            let mut removed = 0_u64;
            for bl in &blocklist_ids {
                removed +=
                    delete_domains_in_batches(&self.pools.write, bl, DomainScope::All).await?;
            }
            if removed == 0 {
                break;
            }
            if pass + 1 == DRAIN_MAX_PASSES {
                tracing::warn!(
                    profile_id = %id,
                    passes = DRAIN_MAX_PASSES,
                    "profile still gaining blocklist domains while draining; deleting anyway — \
                     the cascade may briefly hold the write connection"
                );
            }
        }

        let result = sqlx::query("DELETE FROM dns_filter_profiles WHERE id = ? AND builtin = 0")
            .bind(&id_str)
            .execute(&self.pools.write)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    // ── Blocklists ──────────────────────────────────────────────────────

    async fn list_blocklists(&self, profile_id: Uuid) -> anyhow::Result<Vec<Blocklist>> {
        let pid = profile_id.to_string();
        let rows = sqlx::query_as::<_, DbBlocklistRow>(
            "SELECT id, profile_id, name, url, enabled, entry_count, last_updated, \
                    cron_schedule, last_error, last_error_at, created_at, updated_at \
             FROM dns_filter_blocklists WHERE profile_id = ? \
             ORDER BY created_at ASC",
        )
        .bind(&pid)
        .fetch_all(&self.pools.read)
        .await?;
        rows.into_iter()
            .map(DbBlocklistRow::into_blocklist)
            .collect()
    }

    async fn get_blocklist(&self, id: Uuid) -> anyhow::Result<Option<Blocklist>> {
        let id_str = id.to_string();
        let row = sqlx::query_as::<_, DbBlocklistRow>(
            "SELECT id, profile_id, name, url, enabled, entry_count, last_updated, \
                    cron_schedule, last_error, last_error_at, created_at, updated_at \
             FROM dns_filter_blocklists WHERE id = ?",
        )
        .bind(&id_str)
        .fetch_optional(&self.pools.read)
        .await?;
        row.map(DbBlocklistRow::into_blocklist).transpose()
    }

    async fn create_blocklist(&self, row: &BlocklistRow) -> anyhow::Result<()> {
        let now = now_iso();
        sqlx::query(
            "INSERT INTO dns_filter_blocklists \
                 (id, profile_id, name, url, enabled, cron_schedule, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&row.id)
        .bind(&row.profile_id)
        .bind(&row.name)
        .bind(&row.url)
        .bind(row.enabled)
        .bind(&row.cron_schedule)
        .bind(&now)
        .bind(&now)
        .execute(&self.pools.write)
        .await?;
        Ok(())
    }

    async fn update_blocklist(&self, id: Uuid, row: &BlocklistUpdate) -> anyhow::Result<()> {
        let mut sets: Vec<&str> = Vec::new();
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
        if sets.is_empty() {
            return Ok(());
        }

        let now = now_iso();
        sets.push("updated_at = ?");
        binds.push(now);
        binds.push(id.to_string());

        let sql = format!(
            "UPDATE dns_filter_blocklists SET {} WHERE id = ?",
            sets.join(", ")
        );
        let mut q = sqlx::query(sqlx::AssertSqlSafe(sql));
        for b in &binds {
            q = q.bind(b);
        }
        q.execute(&self.pools.write).await?;
        Ok(())
    }

    async fn delete_blocklist(&self, id: Uuid) -> anyhow::Result<bool> {
        let id_str = id.to_string();

        // Drain the domains first. Left attached, the ON DELETE CASCADE would
        // drop millions of rows in one implicit transaction and monopolise the
        // sole writer — see `delete_domains_in_batches`.
        delete_domains_in_batches(&self.pools.write, &id_str, DomainScope::All).await?;

        let result = sqlx::query("DELETE FROM dns_filter_blocklists WHERE id = ?")
            .bind(&id_str)
            .execute(&self.pools.write)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn replace_blocklist_domains(&self, id: Uuid, domains: &[String]) -> anyhow::Result<u64> {
        let id_str = id.to_string();
        let now = now_iso();

        // Dedupe defensively. `(domain, blocklist_id, generation)` is the
        // primary key, so a repeated domain in the caller's slice would abort
        // the whole import — and `entry_count` must reflect rows actually
        // stored, not the caller's slice length.
        let domains: Vec<&String> = {
            let mut seen = std::collections::HashSet::with_capacity(domains.len());
            domains.iter().filter(|d| seen.insert(*d)).collect()
        };
        let count = domains.len() as u64;

        // Read the generation readers are currently pinned to; we build the
        // next one alongside it.
        let active: i64 =
            sqlx::query_scalar("SELECT active_generation FROM dns_filter_blocklists WHERE id = ?")
                .bind(&id_str)
                .fetch_optional(&self.pools.read)
                .await?
                .ok_or_else(|| anyhow::anyhow!("blocklist {id} not found"))?;
        let next = active.wrapping_add(1);

        // Clear any debris a previously-interrupted import left behind at this
        // generation (the daemon can be killed mid-import).
        delete_domains_in_batches(&self.pools.write, &id_str, DomainScope::Generation(next))
            .await?;

        // Write the new generation in independently-committed batches. Each
        // batch takes the sole write connection, holds it for a few hundred
        // rows, then hands it back — so DHCP, stats, the query log and the
        // health probes all keep making progress while a 1.9M-domain feed
        // imports. Readers still see `active`, so a partial generation is
        // invisible to them.
        for batch in domains.chunks(IMPORT_BATCH_ROWS) {
            let mut tx = self.pools.write.begin().await?;
            for chunk in batch.chunks(INSERT_CHUNK_ROWS) {
                let placeholders: Vec<&str> = chunk.iter().map(|_| "(?, ?, ?)").collect();
                let sql = format!(
                    "INSERT INTO dns_filter_blocked_domains (domain, blocklist_id, generation) \
                     VALUES {}",
                    placeholders.join(", ")
                );
                let mut q = sqlx::query(sqlx::AssertSqlSafe(sql));
                for d in chunk {
                    q = q.bind(d).bind(&id_str).bind(next);
                }
                q.execute(&mut *tx).await?;
            }
            tx.commit().await?;

            // Give any writer queued behind us a turn before we re-acquire.
            tokio::task::yield_now().await;
        }

        // The swap itself: one tiny transaction. Readers move from the old
        // generation to the new one atomically.
        sqlx::query(
            "UPDATE dns_filter_blocklists SET active_generation = ?, entry_count = ?, \
             last_updated = ?, last_error = NULL, last_error_at = NULL, updated_at = ? \
             WHERE id = ?",
        )
        .bind(next)
        .bind(count.cast_signed())
        .bind(&now)
        .bind(&now)
        .bind(&id_str)
        .execute(&self.pools.write)
        .await?;

        // Reclaim every superseded row. Batched for the same reason as the
        // insert: a 1.9M-row DELETE would hold the writer just as long as the
        // import used to. `AllExcept` rather than the single previous
        // generation, so a generation orphaned by a crash between an earlier
        // flip and its cleanup is reclaimed here instead of leaking forever.
        // Best-effort — the rows are already invisible to readers, and the next
        // import sweeps whatever is left.
        if let Err(e) =
            delete_domains_in_batches(&self.pools.write, &id_str, DomainScope::AllExcept(next))
                .await
        {
            tracing::warn!(
                blocklist_id = %id,
                active_generation = next,
                error = %e,
                "failed to reclaim superseded blocklist generations; rows are invisible to \
                 readers and will be cleaned up by the next import"
            );
        }

        Ok(count)
    }

    async fn count_unimported_blocklists(&self) -> anyhow::Result<u64> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM dns_filter_blocklists \
             WHERE enabled = 1 AND last_updated IS NULL",
        )
        .fetch_one(&self.pools.read)
        .await?;
        Ok(count.cast_unsigned())
    }

    async fn set_blocklist_error(&self, id: Uuid, error: Option<&str>) -> anyhow::Result<()> {
        let id_str = id.to_string();
        let now = now_iso();
        match error {
            Some(msg) => {
                sqlx::query(
                    "UPDATE dns_filter_blocklists SET last_error = ?, last_error_at = ?, \
                     updated_at = ? WHERE id = ?",
                )
                .bind(msg)
                .bind(&now)
                .bind(&now)
                .bind(&id_str)
                .execute(&self.pools.write)
                .await?;
            }
            None => {
                sqlx::query(
                    "UPDATE dns_filter_blocklists SET last_error = NULL, last_error_at = NULL, \
                     updated_at = ? WHERE id = ?",
                )
                .bind(&now)
                .bind(&id_str)
                .execute(&self.pools.write)
                .await?;
            }
        }
        Ok(())
    }

    // ── Allowlist ───────────────────────────────────────────────────────

    async fn list_allowlist(&self, profile_id: Uuid) -> anyhow::Result<Vec<AllowlistEntry>> {
        let pid = profile_id.to_string();
        let rows = sqlx::query_as::<_, DbAllowlistRow>(
            "SELECT id, profile_id, domain, reason, created_at \
             FROM dns_filter_allowlist WHERE profile_id = ? ORDER BY created_at ASC",
        )
        .bind(&pid)
        .fetch_all(&self.pools.read)
        .await?;
        rows.into_iter().map(DbAllowlistRow::into_entry).collect()
    }

    async fn create_allowlist_entry(&self, row: &AllowlistRow) -> anyhow::Result<()> {
        let now = now_iso();
        sqlx::query(
            "INSERT INTO dns_filter_allowlist (id, profile_id, domain, reason, created_at) \
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&row.id)
        .bind(&row.profile_id)
        .bind(&row.domain)
        .bind(&row.reason)
        .bind(&now)
        .execute(&self.pools.write)
        .await?;
        Ok(())
    }

    async fn delete_allowlist_entry(&self, id: Uuid) -> anyhow::Result<bool> {
        let id_str = id.to_string();
        let result = sqlx::query("DELETE FROM dns_filter_allowlist WHERE id = ?")
            .bind(&id_str)
            .execute(&self.pools.write)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    // ── Custom rules ────────────────────────────────────────────────────

    async fn list_custom_rules(&self, profile_id: Uuid) -> anyhow::Result<Vec<CustomFilterRule>> {
        let pid = profile_id.to_string();
        let rows = sqlx::query_as::<_, DbCustomRuleRow>(
            "SELECT id, profile_id, rule_text, enabled, comment, created_at, updated_at \
             FROM dns_filter_custom_rules WHERE profile_id = ? ORDER BY created_at ASC",
        )
        .bind(&pid)
        .fetch_all(&self.pools.read)
        .await?;
        rows.into_iter().map(DbCustomRuleRow::into_rule).collect()
    }

    async fn get_custom_rule(&self, id: Uuid) -> anyhow::Result<Option<CustomFilterRule>> {
        let id_str = id.to_string();
        let row = sqlx::query_as::<_, DbCustomRuleRow>(
            "SELECT id, profile_id, rule_text, enabled, comment, created_at, updated_at \
             FROM dns_filter_custom_rules WHERE id = ?",
        )
        .bind(&id_str)
        .fetch_optional(&self.pools.read)
        .await?;
        row.map(DbCustomRuleRow::into_rule).transpose()
    }

    async fn create_custom_rule(&self, row: &CustomRuleRow) -> anyhow::Result<()> {
        let now = now_iso();
        sqlx::query(
            "INSERT INTO dns_filter_custom_rules \
                 (id, profile_id, rule_text, enabled, comment, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&row.id)
        .bind(&row.profile_id)
        .bind(&row.rule_text)
        .bind(row.enabled)
        .bind(&row.comment)
        .bind(&now)
        .bind(&now)
        .execute(&self.pools.write)
        .await?;
        Ok(())
    }

    async fn update_custom_rule(&self, id: Uuid, row: &CustomRuleUpdate) -> anyhow::Result<()> {
        let mut sets: Vec<&str> = Vec::new();
        let mut binds: Vec<String> = Vec::new();

        if let Some(ref rt) = row.rule_text {
            sets.push("rule_text = ?");
            binds.push(rt.clone());
        }
        if let Some(enabled) = row.enabled {
            sets.push("enabled = ?");
            binds.push(if enabled {
                "1".to_owned()
            } else {
                "0".to_owned()
            });
        }
        if let Some(ref c) = row.comment {
            sets.push("comment = ?");
            binds.push(c.clone());
        }
        if sets.is_empty() {
            return Ok(());
        }

        let now = now_iso();
        sets.push("updated_at = ?");
        binds.push(now);
        binds.push(id.to_string());

        let sql = format!(
            "UPDATE dns_filter_custom_rules SET {} WHERE id = ?",
            sets.join(", ")
        );
        let mut q = sqlx::query(sqlx::AssertSqlSafe(sql));
        for b in &binds {
            q = q.bind(b);
        }
        q.execute(&self.pools.write).await?;
        Ok(())
    }

    async fn delete_custom_rule(&self, id: Uuid) -> anyhow::Result<bool> {
        let id_str = id.to_string();
        let result = sqlx::query("DELETE FROM dns_filter_custom_rules WHERE id = ?")
            .bind(&id_str)
            .execute(&self.pools.write)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn load_filter_inputs_for_profile(
        &self,
        profile_id: Uuid,
    ) -> anyhow::Result<ProfileFilterInputs> {
        let pid = profile_id.to_string();

        // No blocked-domain query here on purpose: the runtime profile
        // references each blocklist's compiled filter directly. See
        // `ProfileFilterInputs`.
        let allow: Vec<String> =
            sqlx::query_scalar("SELECT domain FROM dns_filter_allowlist WHERE profile_id = ?")
                .bind(&pid)
                .fetch_all(&self.pools.read)
                .await?;

        let rules: Vec<String> = sqlx::query_scalar(
            "SELECT rule_text FROM dns_filter_custom_rules \
             WHERE profile_id = ? AND enabled = 1",
        )
        .bind(&pid)
        .fetch_all(&self.pools.read)
        .await?;

        Ok(ProfileFilterInputs {
            allowlist: allow,
            custom_rules: rules,
        })
    }

    async fn load_blocklist_domains(&self, blocklist_id: Uuid) -> anyhow::Result<Vec<String>> {
        let id = blocklist_id.to_string();
        let domains: Vec<String> = sqlx::query_scalar(
            "SELECT bd.domain \
             FROM dns_filter_blocked_domains bd \
             JOIN dns_filter_blocklists bl \
               ON bd.blocklist_id = bl.id AND bd.generation = bl.active_generation \
             WHERE bl.id = ?",
        )
        .bind(&id)
        .fetch_all(&self.pools.read)
        .await?;
        Ok(domains)
    }

    // ── Per-device settings ─────────────────────────────────────────────

    async fn find_device_settings(
        &self,
        device_id: Uuid,
    ) -> anyhow::Result<Option<DeviceDnsFilterSettings>> {
        let did = device_id.to_string();

        let settings_row: Option<(i64, String)> = sqlx::query_as(
            "SELECT enabled, updated_at FROM dns_filter_device_settings WHERE device_id = ?",
        )
        .bind(&did)
        .fetch_optional(&self.pools.read)
        .await?;

        let profile_ids: Vec<String> = sqlx::query_scalar(
            "SELECT profile_id FROM dns_filter_device_profile WHERE device_id = ?",
        )
        .bind(&did)
        .fetch_all(&self.pools.read)
        .await?;

        if settings_row.is_none() && profile_ids.is_empty() {
            return Ok(None);
        }

        let (enabled, updated_at) = match settings_row {
            Some((e, u)) => (e != 0, parse_ts(&u)?),
            None => (true, Utc::now()),
        };

        let profile_ids: Vec<Uuid> = profile_ids
            .into_iter()
            .filter_map(|s| s.parse().ok())
            .collect();

        Ok(Some(DeviceDnsFilterSettings {
            device_id,
            enabled,
            profile_ids,
            updated_at,
        }))
    }

    async fn upsert_device_settings(&self, row: &DeviceSettingsRow) -> anyhow::Result<()> {
        let now = now_iso();
        sqlx::query(
            "INSERT INTO dns_filter_device_settings (device_id, enabled, updated_at) \
             VALUES (?, ?, ?) \
             ON CONFLICT(device_id) DO UPDATE SET enabled = excluded.enabled, \
                                                  updated_at = excluded.updated_at",
        )
        .bind(&row.device_id)
        .bind(row.enabled)
        .bind(&now)
        .execute(&self.pools.write)
        .await?;
        Ok(())
    }

    async fn set_device_profiles(
        &self,
        device_id: Uuid,
        profile_ids: &[Uuid],
    ) -> anyhow::Result<()> {
        let did = device_id.to_string();
        let mut tx = self.pools.write.begin().await?;

        sqlx::query("DELETE FROM dns_filter_device_profile WHERE device_id = ?")
            .bind(&did)
            .execute(&mut *tx)
            .await?;

        for pid in profile_ids {
            sqlx::query(
                "INSERT INTO dns_filter_device_profile (device_id, profile_id) VALUES (?, ?)",
            )
            .bind(&did)
            .bind(pid.to_string())
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(())
    }

    async fn list_device_settings_with_ips(
        &self,
        only_filtering_active: bool,
    ) -> anyhow::Result<Vec<DeviceSettingsWithIp>> {
        // Union of devices with explicit settings and devices with profile
        // assignments. Left-join to bring in last-known IP.
        // The base query already has a `WHERE d.id IN (...)`, so the
        // optional filter has to be `AND ...`, not a second `WHERE`.
        let extra_clause = if only_filtering_active {
            "AND COALESCE(s.enabled, 1) = 1"
        } else {
            ""
        };
        let sql = format!(
            "SELECT d.id, d.last_ip, s.enabled, s.updated_at \
             FROM devices d \
             LEFT JOIN dns_filter_device_settings s ON s.device_id = d.id \
             WHERE d.id IN ( \
                 SELECT device_id FROM dns_filter_device_settings \
                 UNION \
                 SELECT device_id FROM dns_filter_device_profile \
             ) {extra_clause}"
        );

        let rows: Vec<(String, Option<String>, Option<i64>, Option<String>)> =
            sqlx::query_as(sqlx::AssertSqlSafe(sql))
                .fetch_all(&self.pools.read)
                .await?;

        let mut out = Vec::with_capacity(rows.len());
        for (device_id_s, ip, enabled, updated) in rows {
            let device_id: Uuid = device_id_s.parse()?;

            let profile_ids: Vec<String> = sqlx::query_scalar(
                "SELECT profile_id FROM dns_filter_device_profile WHERE device_id = ?",
            )
            .bind(&device_id_s)
            .fetch_all(&self.pools.read)
            .await?;
            let profile_ids: Vec<Uuid> = profile_ids
                .into_iter()
                .filter_map(|s| s.parse().ok())
                .collect();

            let updated_at = match updated {
                Some(s) => parse_ts(&s)?,
                None => Utc::now(),
            };

            out.push(DeviceSettingsWithIp {
                settings: DeviceDnsFilterSettings {
                    device_id,
                    enabled: enabled.unwrap_or(1) != 0,
                    profile_ids,
                    updated_at,
                },
                ip,
            });
        }
        Ok(out)
    }

    // ── Global config ───────────────────────────────────────────────────

    async fn get_dns_filter_config(&self) -> anyhow::Result<DnsFilterConfig> {
        let enabled: Option<String> =
            sqlx::query_scalar("SELECT value FROM system_config WHERE key = ?")
                .bind(KEY_DNS_FILTERING_ENABLED)
                .fetch_optional(&self.pools.read)
                .await?;

        let default_profile_ids: Vec<String> = sqlx::query_scalar(
            "SELECT profile_id FROM dns_filter_default_profile \
             ORDER BY created_at, profile_id",
        )
        .fetch_all(&self.pools.read)
        .await?;

        let enabled = enabled.as_deref() != Some("false");
        let default_profile_ids: Vec<Uuid> = default_profile_ids
            .into_iter()
            .filter_map(|s| s.parse().ok())
            .collect();

        Ok(DnsFilterConfig {
            enabled,
            default_profile_ids,
        })
    }

    async fn set_dns_filter_config(&self, config: &DnsFilterConfig) -> anyhow::Result<()> {
        let mut tx = self.pools.write.begin().await?;
        sqlx::query(
            "INSERT INTO system_config (key, value) VALUES (?, ?) \
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        )
        .bind(KEY_DNS_FILTERING_ENABLED)
        .bind(if config.enabled { "true" } else { "false" })
        .execute(&mut *tx)
        .await?;

        sqlx::query("DELETE FROM dns_filter_default_profile")
            .execute(&mut *tx)
            .await?;
        for pid in &config.default_profile_ids {
            sqlx::query("INSERT INTO dns_filter_default_profile (profile_id) VALUES (?)")
                .bind(pid.to_string())
                .execute(&mut *tx)
                .await?;
        }

        tx.commit().await?;
        Ok(())
    }
}
