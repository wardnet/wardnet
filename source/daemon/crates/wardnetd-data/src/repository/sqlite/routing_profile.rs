//! SQLite-backed [`RoutingProfileRepository`] implementation.

use async_trait::async_trait;
use chrono::{NaiveDateTime, TimeZone, Utc};
use sqlx::SqlitePool;
use uuid::Uuid;

use wardnet_common::routing_profile::{DomainRoutingRule, DomainRoutingTarget, RoutingProfile};

use crate::db::DbPools;
use crate::repository::routing_profile::{
    DeviceAssignment, RoutingProfileRepository, RoutingProfileRow, RoutingProfileUpdate,
    RoutingRuleRow, RoutingRuleUpdate,
};

const TS_FMT: &str = "%Y-%m-%dT%H:%M:%SZ";

fn now_iso() -> String {
    Utc::now().format(TS_FMT).to_string()
}

fn parse_ts(s: &str) -> anyhow::Result<chrono::DateTime<Utc>> {
    let naive = NaiveDateTime::parse_from_str(s, TS_FMT)?;
    Ok(Utc.from_utc_datetime(&naive))
}

/// SQLite-backed routing-profile repository.
pub struct SqliteRoutingProfileRepository {
    pools: DbPools,
}

impl SqliteRoutingProfileRepository {
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
struct DbProfileRow {
    id: String,
    name: String,
    rule_count: i64,
    created_at: String,
    updated_at: String,
}

impl DbProfileRow {
    fn into_profile(self) -> anyhow::Result<RoutingProfile> {
        Ok(RoutingProfile {
            id: self.id.parse()?,
            name: self.name,
            rule_count: self.rule_count,
            created_at: parse_ts(&self.created_at)?,
            updated_at: parse_ts(&self.updated_at)?,
        })
    }
}

#[derive(sqlx::FromRow)]
struct DbRuleRow {
    id: String,
    profile_id: String,
    pattern: String,
    target_kind: String,
    tunnel_id: Option<String>,
    enabled: i64,
    created_at: String,
    updated_at: String,
}

impl DbRuleRow {
    fn into_rule(self) -> anyhow::Result<DomainRoutingRule> {
        let tunnel_id = match self.tunnel_id {
            Some(s) => Some(s.parse()?),
            None => None,
        };
        let target = DomainRoutingTarget::from_storage(&self.target_kind, tunnel_id)
            .ok_or_else(|| anyhow::anyhow!("invalid routing rule target in database"))?;
        Ok(DomainRoutingRule {
            id: self.id.parse()?,
            profile_id: self.profile_id.parse()?,
            pattern: self.pattern,
            target,
            enabled: self.enabled != 0,
            created_at: parse_ts(&self.created_at)?,
            updated_at: parse_ts(&self.updated_at)?,
        })
    }
}

// `rule_count` is a correlated subquery so the profile list carries its rule
// count in one round-trip (no per-row follow-up query from the UI).
const LIST_PROFILES_SQL: &str = "SELECT id, name, \
     (SELECT COUNT(*) FROM routing_profile_rules r WHERE r.profile_id = routing_profiles.id) AS rule_count, \
     created_at, updated_at FROM routing_profiles ORDER BY name ASC";
const GET_PROFILE_SQL: &str = "SELECT id, name, \
     (SELECT COUNT(*) FROM routing_profile_rules r WHERE r.profile_id = routing_profiles.id) AS rule_count, \
     created_at, updated_at FROM routing_profiles WHERE id = ?";
const LIST_RULES_SQL: &str = "SELECT id, profile_id, pattern, target_kind, tunnel_id, enabled, created_at, updated_at \
     FROM routing_profile_rules WHERE profile_id = ? ORDER BY created_at ASC";
const LIST_ALL_RULES_SQL: &str = "SELECT id, profile_id, pattern, target_kind, tunnel_id, enabled, created_at, updated_at \
     FROM routing_profile_rules ORDER BY created_at ASC";
const GET_RULE_SQL: &str = "SELECT id, profile_id, pattern, target_kind, tunnel_id, enabled, created_at, updated_at \
     FROM routing_profile_rules WHERE id = ?";

#[async_trait]
impl RoutingProfileRepository for SqliteRoutingProfileRepository {
    // ── Profiles ────────────────────────────────────────────────────────

    async fn list_profiles(&self) -> anyhow::Result<Vec<RoutingProfile>> {
        let rows = sqlx::query_as::<_, DbProfileRow>(LIST_PROFILES_SQL)
            .fetch_all(&self.pools.read)
            .await?;
        rows.into_iter().map(DbProfileRow::into_profile).collect()
    }

    async fn get_profile(&self, id: Uuid) -> anyhow::Result<Option<RoutingProfile>> {
        let row = sqlx::query_as::<_, DbProfileRow>(GET_PROFILE_SQL)
            .bind(id.to_string())
            .fetch_optional(&self.pools.read)
            .await?;
        row.map(DbProfileRow::into_profile).transpose()
    }

    async fn create_profile(&self, row: &RoutingProfileRow) -> anyhow::Result<RoutingProfile> {
        let now = now_iso();
        sqlx::query(
            "INSERT INTO routing_profiles (id, name, created_at, updated_at) \
             VALUES (?, ?, ?, ?)",
        )
        .bind(&row.id)
        .bind(&row.name)
        .bind(&now)
        .bind(&now)
        .execute(&self.pools.write)
        .await?;
        Ok(RoutingProfile {
            id: row.id.parse()?,
            name: row.name.clone(),
            rule_count: 0,
            created_at: parse_ts(&now)?,
            updated_at: parse_ts(&now)?,
        })
    }

    async fn update_profile(
        &self,
        id: Uuid,
        update: &RoutingProfileUpdate,
    ) -> anyhow::Result<Option<RoutingProfile>> {
        let Some(name) = update.name.as_ref() else {
            // Nothing to change — return the current row unchanged.
            return self.get_profile(id).await;
        };
        let now = now_iso();
        let affected =
            sqlx::query("UPDATE routing_profiles SET name = ?, updated_at = ? WHERE id = ?")
                .bind(name)
                .bind(&now)
                .bind(id.to_string())
                .execute(&self.pools.write)
                .await?
                .rows_affected();
        if affected == 0 {
            return Ok(None);
        }
        self.get_profile(id).await
    }

    async fn delete_profile(&self, id: Uuid) -> anyhow::Result<bool> {
        let affected = sqlx::query("DELETE FROM routing_profiles WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pools.write)
            .await?
            .rows_affected();
        Ok(affected > 0)
    }

    // ── Rules ───────────────────────────────────────────────────────────

    async fn list_rules(&self, profile_id: Uuid) -> anyhow::Result<Vec<DomainRoutingRule>> {
        let rows = sqlx::query_as::<_, DbRuleRow>(LIST_RULES_SQL)
            .bind(profile_id.to_string())
            .fetch_all(&self.pools.read)
            .await?;
        rows.into_iter().map(DbRuleRow::into_rule).collect()
    }

    async fn list_all_rules(&self) -> anyhow::Result<Vec<DomainRoutingRule>> {
        let rows = sqlx::query_as::<_, DbRuleRow>(LIST_ALL_RULES_SQL)
            .fetch_all(&self.pools.read)
            .await?;
        rows.into_iter().map(DbRuleRow::into_rule).collect()
    }

    async fn get_rule(&self, id: Uuid) -> anyhow::Result<Option<DomainRoutingRule>> {
        let row = sqlx::query_as::<_, DbRuleRow>(GET_RULE_SQL)
            .bind(id.to_string())
            .fetch_optional(&self.pools.read)
            .await?;
        row.map(DbRuleRow::into_rule).transpose()
    }

    async fn create_rule(&self, row: &RoutingRuleRow) -> anyhow::Result<DomainRoutingRule> {
        let now = now_iso();
        let (kind, tunnel_id) = row.target.to_storage();
        sqlx::query(
            "INSERT INTO routing_profile_rules \
             (id, profile_id, pattern, target_kind, tunnel_id, enabled, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&row.id)
        .bind(row.profile_id.to_string())
        .bind(&row.pattern)
        .bind(kind)
        .bind(tunnel_id.map(|t| t.to_string()))
        .bind(row.enabled)
        .bind(&now)
        .bind(&now)
        .execute(&self.pools.write)
        .await?;
        Ok(DomainRoutingRule {
            id: row.id.parse()?,
            profile_id: row.profile_id,
            pattern: row.pattern.clone(),
            target: row.target.clone(),
            enabled: row.enabled,
            created_at: parse_ts(&now)?,
            updated_at: parse_ts(&now)?,
        })
    }

    async fn update_rule(
        &self,
        id: Uuid,
        update: &RoutingRuleUpdate,
    ) -> anyhow::Result<Option<DomainRoutingRule>> {
        // Read-modify-write keeps the SQL a single fixed statement (no dynamic
        // column list) while honouring the partial-update semantics.
        let Some(existing) = self.get_rule(id).await? else {
            return Ok(None);
        };
        let pattern = update.pattern.clone().unwrap_or(existing.pattern);
        let target = update.target.clone().unwrap_or(existing.target);
        let enabled = update.enabled.unwrap_or(existing.enabled);
        let (kind, tunnel_id) = target.to_storage();
        let now = now_iso();
        sqlx::query(
            "UPDATE routing_profile_rules \
             SET pattern = ?, target_kind = ?, tunnel_id = ?, enabled = ?, updated_at = ? \
             WHERE id = ?",
        )
        .bind(&pattern)
        .bind(kind)
        .bind(tunnel_id.map(|t| t.to_string()))
        .bind(enabled)
        .bind(&now)
        .bind(id.to_string())
        .execute(&self.pools.write)
        .await?;
        self.get_rule(id).await
    }

    async fn delete_rule(&self, id: Uuid) -> anyhow::Result<bool> {
        let affected = sqlx::query("DELETE FROM routing_profile_rules WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pools.write)
            .await?
            .rows_affected();
        Ok(affected > 0)
    }

    // ── Device assignment (ordered) ─────────────────────────────────────

    async fn get_device_profiles(&self, device_id: Uuid) -> anyhow::Result<Vec<Uuid>> {
        let ids: Vec<String> = sqlx::query_scalar(
            "SELECT profile_id FROM routing_device_profile \
             WHERE device_id = ? ORDER BY position ASC",
        )
        .bind(device_id.to_string())
        .fetch_all(&self.pools.read)
        .await?;
        Ok(ids.into_iter().filter_map(|s| s.parse().ok()).collect())
    }

    async fn set_device_profiles(
        &self,
        device_id: Uuid,
        profile_ids: &[Uuid],
    ) -> anyhow::Result<()> {
        let did = device_id.to_string();
        let mut tx = self.pools.write.begin().await?;

        sqlx::query("DELETE FROM routing_device_profile WHERE device_id = ?")
            .bind(&did)
            .execute(&mut *tx)
            .await?;

        for (position, pid) in profile_ids.iter().enumerate() {
            sqlx::query(
                "INSERT INTO routing_device_profile (device_id, profile_id, position) \
                 VALUES (?, ?, ?)",
            )
            .bind(&did)
            .bind(pid.to_string())
            .bind(i64::try_from(position).unwrap_or(i64::MAX))
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(())
    }

    async fn list_device_assignments(&self) -> anyhow::Result<Vec<DeviceAssignment>> {
        let device_ids: Vec<String> =
            sqlx::query_scalar("SELECT DISTINCT device_id FROM routing_device_profile")
                .fetch_all(&self.pools.read)
                .await?;

        let mut out = Vec::with_capacity(device_ids.len());
        for did in device_ids {
            let device_id: Uuid = did.parse()?;
            let profile_ids = self.get_device_profiles(device_id).await?;
            out.push(DeviceAssignment {
                device_id,
                profile_ids,
            });
        }
        Ok(out)
    }

    async fn list_profile_devices(&self, profile_id: Uuid) -> anyhow::Result<Vec<Uuid>> {
        let ids: Vec<String> = sqlx::query_scalar(
            "SELECT device_id FROM routing_device_profile \
             WHERE profile_id = ? ORDER BY device_id ASC",
        )
        .bind(profile_id.to_string())
        .fetch_all(&self.pools.read)
        .await?;
        Ok(ids.into_iter().filter_map(|s| s.parse().ok()).collect())
    }
}
