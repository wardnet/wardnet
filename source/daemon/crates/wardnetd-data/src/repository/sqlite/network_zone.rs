use async_trait::async_trait;
use sqlx::SqlitePool;
use wardnet_common::network_zone::{
    AllowedTargetKind, NetworkZone, ZoneProvenance, ZoneStance, ZoneSubnet,
};

use super::super::NetworkZoneRepository;
use crate::db::DbPools;

/// SQLite-backed implementation of [`NetworkZoneRepository`].
pub struct SqliteNetworkZoneRepository {
    pools: DbPools,
}

impl SqliteNetworkZoneRepository {
    /// Create a new repository backed by the given connection pool.
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self::new_pools(DbPools::single(pool))
    }

    /// Create a new repository with split reader/writer pools.
    #[must_use]
    pub fn new_pools(pools: DbPools) -> Self {
        Self { pools }
    }
}

/// Raw row from the `network_zones` table used for internal mapping.
#[derive(sqlx::FromRow)]
struct ZoneRow {
    id: String,
    name: String,
    provenance: String,
    isolation_stance: String,
    allowed_targets: String,
    member_isolation: i64,
    subnet_cidr: Option<String>,
    admin_ui_reachable: i64,
    is_default: i64,
    is_default_for_new: i64,
    created_at: String,
    updated_at: String,
}

impl ZoneRow {
    fn into_zone(self) -> anyhow::Result<NetworkZone> {
        let provenance: ZoneProvenance = serde_json::from_str(&format!("\"{}\"", self.provenance))
            .unwrap_or(ZoneProvenance::Manual);
        let isolation_stance: ZoneStance =
            serde_json::from_str(&format!("\"{}\"", self.isolation_stance))
                .unwrap_or(ZoneStance::SharedSubnet);
        let allowed_targets: Vec<AllowedTargetKind> = serde_json::from_str(&self.allowed_targets)?;
        Ok(NetworkZone {
            id: self.id.parse()?,
            name: self.name,
            provenance,
            isolation_stance,
            allowed_targets,
            member_isolation: self.member_isolation != 0,
            subnet: self.subnet_cidr.map(|cidr| ZoneSubnet { cidr }),
            admin_ui_reachable: self.admin_ui_reachable != 0,
            is_default: self.is_default != 0,
            is_default_for_new: self.is_default_for_new != 0,
            created_at: self.created_at.parse()?,
            updated_at: self.updated_at.parse()?,
        })
    }
}

const SELECT_COLS: &str = "id, name, provenance, isolation_stance, allowed_targets, \
    member_isolation, subnet_cidr, admin_ui_reachable, is_default, is_default_for_new, \
    created_at, updated_at";

/// Serialize a scalar enum to its `snake_case` wire string (strip the JSON quotes).
fn enum_str<T: serde::Serialize>(value: &T) -> anyhow::Result<String> {
    Ok(serde_json::to_string(value)?.trim_matches('"').to_owned())
}

#[async_trait]
impl NetworkZoneRepository for SqliteNetworkZoneRepository {
    async fn find_all(&self) -> anyhow::Result<Vec<NetworkZone>> {
        let query = format!("SELECT {SELECT_COLS} FROM network_zones ORDER BY name");
        let rows = sqlx::query_as::<_, ZoneRow>(sqlx::AssertSqlSafe(query))
            .fetch_all(&self.pools.read)
            .await?;
        rows.into_iter().map(ZoneRow::into_zone).collect()
    }

    async fn find_by_id(&self, id: &str) -> anyhow::Result<Option<NetworkZone>> {
        let query = format!("SELECT {SELECT_COLS} FROM network_zones WHERE id = ?");
        let row = sqlx::query_as::<_, ZoneRow>(sqlx::AssertSqlSafe(query))
            .bind(id)
            .fetch_optional(&self.pools.read)
            .await?;
        row.map(ZoneRow::into_zone).transpose()
    }

    async fn find_default(&self) -> anyhow::Result<NetworkZone> {
        let query = format!("SELECT {SELECT_COLS} FROM network_zones WHERE is_default = 1");
        let row = sqlx::query_as::<_, ZoneRow>(sqlx::AssertSqlSafe(query))
            .fetch_optional(&self.pools.read)
            .await?
            .ok_or_else(|| anyhow::anyhow!("no default network zone configured"))?;
        row.into_zone()
    }

    async fn find_default_for_new(&self) -> anyhow::Result<NetworkZone> {
        let query = format!("SELECT {SELECT_COLS} FROM network_zones WHERE is_default_for_new = 1");
        let row = sqlx::query_as::<_, ZoneRow>(sqlx::AssertSqlSafe(query))
            .fetch_optional(&self.pools.read)
            .await?
            .ok_or_else(|| anyhow::anyhow!("no default-for-new network zone configured"))?;
        row.into_zone()
    }

    async fn insert(&self, zone: &NetworkZone) -> anyhow::Result<()> {
        let allowed_targets = serde_json::to_string(&zone.allowed_targets)?;
        sqlx::query(
            "INSERT INTO network_zones \
             (id, name, provenance, isolation_stance, allowed_targets, member_isolation, \
              subnet_cidr, admin_ui_reachable, is_default, is_default_for_new, \
              created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(zone.id.to_string())
        .bind(&zone.name)
        .bind(enum_str(&zone.provenance)?)
        .bind(enum_str(&zone.isolation_stance)?)
        .bind(allowed_targets)
        .bind(i64::from(zone.member_isolation))
        .bind(zone.subnet.as_ref().map(|s| s.cidr.clone()))
        .bind(i64::from(zone.admin_ui_reachable))
        .bind(i64::from(zone.is_default))
        .bind(i64::from(zone.is_default_for_new))
        .bind(zone.created_at.to_rfc3339())
        .bind(zone.updated_at.to_rfc3339())
        .execute(&self.pools.write)
        .await?;
        Ok(())
    }

    async fn update(&self, zone: &NetworkZone) -> anyhow::Result<()> {
        let allowed_targets = serde_json::to_string(&zone.allowed_targets)?;
        sqlx::query(
            "UPDATE network_zones SET \
                name = ?, provenance = ?, isolation_stance = ?, allowed_targets = ?, \
                member_isolation = ?, subnet_cidr = ?, admin_ui_reachable = ?, \
                updated_at = ? \
             WHERE id = ?",
        )
        .bind(&zone.name)
        .bind(enum_str(&zone.provenance)?)
        .bind(enum_str(&zone.isolation_stance)?)
        .bind(allowed_targets)
        .bind(i64::from(zone.member_isolation))
        .bind(zone.subnet.as_ref().map(|s| s.cidr.clone()))
        .bind(i64::from(zone.admin_ui_reachable))
        .bind(zone.updated_at.to_rfc3339())
        .bind(zone.id.to_string())
        .execute(&self.pools.write)
        .await?;
        Ok(())
    }

    async fn delete(&self, id: &str) -> anyhow::Result<()> {
        sqlx::query("DELETE FROM network_zones WHERE id = ?")
            .bind(id)
            .execute(&self.pools.write)
            .await?;
        Ok(())
    }

    async fn set_default(&self, id: &str) -> anyhow::Result<()> {
        let mut tx = self.pools.write.begin().await?;
        sqlx::query("UPDATE network_zones SET is_default = 0 WHERE is_default = 1")
            .execute(&mut *tx)
            .await?;
        let set = sqlx::query("UPDATE network_zones SET is_default = 1 WHERE id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        // Guard against clearing the flag from every row but setting it on none
        // (unknown id) — that would orphan the anchor. Roll back by not committing.
        if set.rows_affected() == 0 {
            anyhow::bail!("cannot set default: no network zone with id {id}");
        }
        tx.commit().await?;
        Ok(())
    }

    async fn set_default_for_new(&self, id: &str) -> anyhow::Result<()> {
        let mut tx = self.pools.write.begin().await?;
        sqlx::query("UPDATE network_zones SET is_default_for_new = 0 WHERE is_default_for_new = 1")
            .execute(&mut *tx)
            .await?;
        let set = sqlx::query("UPDATE network_zones SET is_default_for_new = 1 WHERE id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        if set.rows_affected() == 0 {
            anyhow::bail!("cannot set default-for-new: no network zone with id {id}");
        }
        tx.commit().await?;
        Ok(())
    }

    async fn count_members(&self, zone_id: &str) -> anyhow::Result<i64> {
        let count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM devices WHERE zone_id = ?")
            .bind(zone_id)
            .fetch_one(&self.pools.read)
            .await?;
        Ok(count)
    }

    async fn member_counts(&self) -> anyhow::Result<std::collections::HashMap<String, i64>> {
        let rows = sqlx::query_as::<_, (String, i64)>(
            "SELECT zone_id, COUNT(*) FROM devices GROUP BY zone_id",
        )
        .fetch_all(&self.pools.read)
        .await?;
        Ok(rows.into_iter().collect())
    }
}
