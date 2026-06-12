use async_trait::async_trait;
use sqlx::SqlitePool;
use wardnet_common::device::{Device, DeviceType};
use wardnet_common::routing::{RoutingRule, RoutingTarget, RuleCreator};

use super::super::DeviceRepository;
use super::super::device::DeviceRow as InsertDeviceRow;
use crate::db::DbPools;

/// SQLite-backed implementation of [`DeviceRepository`].
pub struct SqliteDeviceRepository {
    pools: DbPools,
}

impl SqliteDeviceRepository {
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

/// Raw row from the `devices` table used for internal mapping.
#[derive(sqlx::FromRow)]
struct DeviceRow {
    id: String,
    mac: String,
    name: Option<String>,
    hostname: Option<String>,
    manufacturer: Option<String>,
    device_type: String,
    first_seen: String,
    last_seen: String,
    last_ip: String,
    admin_locked: i32,
    dns_capture_enabled: i32,
    dns_capture_cap_count: i64,
    dns_capture_cap_days: i64,
}

impl DeviceRow {
    fn into_device(self) -> anyhow::Result<Device> {
        let device_type: DeviceType = serde_json::from_str(&format!("\"{}\"", self.device_type))
            .unwrap_or(DeviceType::Unknown);
        Ok(Device {
            id: self.id.parse()?,
            mac: self.mac,
            name: self.name,
            hostname: self.hostname,
            manufacturer: self.manufacturer,
            device_type,
            first_seen: self.first_seen.parse()?,
            last_seen: self.last_seen.parse()?,
            last_ip: self.last_ip,
            admin_locked: self.admin_locked != 0,
            dns_capture_enabled: self.dns_capture_enabled != 0,
            dns_capture_cap_count: self.dns_capture_cap_count,
            dns_capture_cap_days: self.dns_capture_cap_days,
        })
    }
}

/// Raw row from the `routing_rules` table used for internal mapping.
#[derive(sqlx::FromRow)]
struct RuleRow {
    device_id: String,
    target_json: String,
    created_by: String,
}

const SELECT_COLS: &str = "id, mac, name, hostname, manufacturer, device_type, first_seen, last_seen, last_ip, admin_locked, dns_capture_enabled, dns_capture_cap_count, dns_capture_cap_days";

#[async_trait]
impl DeviceRepository for SqliteDeviceRepository {
    async fn find_by_ip(&self, ip: &str) -> anyhow::Result<Option<Device>> {
        let query = format!("SELECT {SELECT_COLS} FROM devices WHERE last_ip = ?");
        let row = sqlx::query_as::<_, DeviceRow>(sqlx::AssertSqlSafe(query))
            .bind(ip)
            .fetch_optional(&self.pools.read)
            .await?;
        row.map(DeviceRow::into_device).transpose()
    }

    async fn find_by_id(&self, id: &str) -> anyhow::Result<Option<Device>> {
        let query = format!("SELECT {SELECT_COLS} FROM devices WHERE id = ?");
        let row = sqlx::query_as::<_, DeviceRow>(sqlx::AssertSqlSafe(query))
            .bind(id)
            .fetch_optional(&self.pools.read)
            .await?;
        row.map(DeviceRow::into_device).transpose()
    }

    async fn find_by_mac(&self, mac: &str) -> anyhow::Result<Option<Device>> {
        // Defensive lowercase: MAC is stored lowercase across the codebase
        // (issue #312). Inputs from older callers or external probes may
        // arrive uppercase — normalise here so the WHERE-clause hits the
        // canonical row regardless.
        let query = format!("SELECT {SELECT_COLS} FROM devices WHERE mac = ?");
        let row = sqlx::query_as::<_, DeviceRow>(sqlx::AssertSqlSafe(query))
            .bind(mac.to_lowercase())
            .fetch_optional(&self.pools.read)
            .await?;
        row.map(DeviceRow::into_device).transpose()
    }

    async fn find_all(&self) -> anyhow::Result<Vec<Device>> {
        let query = format!("SELECT {SELECT_COLS} FROM devices ORDER BY last_seen DESC");
        let rows = sqlx::query_as::<_, DeviceRow>(sqlx::AssertSqlSafe(query))
            .fetch_all(&self.pools.read)
            .await?;
        rows.into_iter().map(DeviceRow::into_device).collect()
    }

    async fn insert(&self, device: &InsertDeviceRow) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO devices (id, mac, hostname, manufacturer, device_type, first_seen, last_seen, last_ip) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&device.id)
        .bind(device.mac.to_lowercase())
        .bind(&device.hostname)
        .bind(&device.manufacturer)
        .bind(&device.device_type)
        .bind(&device.first_seen)
        .bind(&device.last_seen)
        .bind(&device.last_ip)
        .execute(&self.pools.write)
        .await?;
        Ok(())
    }

    async fn update_last_seen_and_ip(
        &self,
        id: &str,
        ip: &str,
        last_seen: &str,
    ) -> anyhow::Result<()> {
        sqlx::query("UPDATE devices SET last_seen = ?, last_ip = ? WHERE id = ?")
            .bind(last_seen)
            .bind(ip)
            .bind(id)
            .execute(&self.pools.write)
            .await?;
        Ok(())
    }

    async fn update_last_seen_batch(&self, updates: &[(String, String)]) -> anyhow::Result<()> {
        let mut tx = self.pools.write.begin().await?;
        for (device_id, last_seen) in updates {
            sqlx::query("UPDATE devices SET last_seen = ? WHERE id = ?")
                .bind(last_seen)
                .bind(device_id)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    async fn update_hostname(&self, id: &str, hostname: &str) -> anyhow::Result<()> {
        sqlx::query("UPDATE devices SET hostname = ? WHERE id = ?")
            .bind(hostname)
            .bind(id)
            .execute(&self.pools.write)
            .await?;
        Ok(())
    }

    async fn update_name_and_type(
        &self,
        id: &str,
        name: Option<&str>,
        device_type: &str,
    ) -> anyhow::Result<()> {
        sqlx::query("UPDATE devices SET name = ?, device_type = ? WHERE id = ?")
            .bind(name)
            .bind(device_type)
            .bind(id)
            .execute(&self.pools.write)
            .await?;
        Ok(())
    }

    async fn find_stale(&self, before: &str) -> anyhow::Result<Vec<Device>> {
        let query = format!("SELECT {SELECT_COLS} FROM devices WHERE last_seen < ?");
        let rows = sqlx::query_as::<_, DeviceRow>(sqlx::AssertSqlSafe(query))
            .bind(before)
            .fetch_all(&self.pools.read)
            .await?;
        rows.into_iter().map(DeviceRow::into_device).collect()
    }

    async fn find_rule_for_device(&self, device_id: &str) -> anyhow::Result<Option<RoutingRule>> {
        let row = sqlx::query_as::<_, RuleRow>(
            "SELECT device_id, target_json, created_by FROM routing_rules WHERE device_id = ?",
        )
        .bind(device_id)
        .fetch_optional(&self.pools.read)
        .await?;

        match row {
            Some(r) => {
                let target: RoutingTarget = serde_json::from_str(&r.target_json)?;
                let created_by: RuleCreator =
                    serde_json::from_str(&format!("\"{}\"", r.created_by))
                        .unwrap_or(RuleCreator::User);
                Ok(Some(RoutingRule {
                    device_id: r.device_id.parse()?,
                    target,
                    created_by,
                }))
            }
            None => Ok(None),
        }
    }

    async fn find_all_rules(&self) -> anyhow::Result<Vec<RoutingRule>> {
        let rows = sqlx::query_as::<_, RuleRow>(
            "SELECT device_id, target_json, created_by FROM routing_rules",
        )
        .fetch_all(&self.pools.read)
        .await?;

        rows.into_iter()
            .map(|r| {
                let target: RoutingTarget = serde_json::from_str(&r.target_json)?;
                let created_by: RuleCreator =
                    serde_json::from_str(&format!("\"{}\"", r.created_by))
                        .unwrap_or(RuleCreator::User);
                Ok(RoutingRule {
                    device_id: r.device_id.parse()?,
                    target,
                    created_by,
                })
            })
            .collect()
    }

    async fn upsert_user_rule(
        &self,
        device_id: &str,
        target_json: &str,
        now: &str,
    ) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO routing_rules (id, device_id, target_json, created_by, created_at, updated_at) \
             VALUES (?, ?, ?, 'user', ?, ?) \
             ON CONFLICT(device_id) DO UPDATE SET target_json = excluded.target_json, updated_at = excluded.updated_at",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(device_id)
        .bind(target_json)
        .bind(now)
        .bind(now)
        .execute(&self.pools.write)
        .await?;
        Ok(())
    }

    async fn find_devices_for_tunnel(&self, tunnel_id: &str) -> anyhow::Result<Vec<Device>> {
        // The tunnel_id is embedded in the routing_rules.target_json:
        // {"type":"tunnel","tunnel_id":"<uuid>"}. The JOIN brings both
        // tables into scope so we must qualify every column with the
        // `d.` alias — `routing_rules` also has its own `id` column,
        // which would otherwise raise SQLite's ambiguous-column error.
        let pattern = format!("%\"tunnel_id\":\"{tunnel_id}\"%");
        let query = "SELECT d.id, d.mac, d.name, d.hostname, d.manufacturer, d.device_type, \
             d.first_seen, d.last_seen, d.last_ip, d.admin_locked, \
             d.dns_capture_enabled, d.dns_capture_cap_count, d.dns_capture_cap_days \
             FROM devices d \
             JOIN routing_rules r ON r.device_id = d.id \
             WHERE r.target_json LIKE ? \
             ORDER BY d.last_seen DESC";
        let rows = sqlx::query_as::<_, DeviceRow>(query)
            .bind(&pattern)
            .fetch_all(&self.pools.read)
            .await?;
        rows.into_iter().map(DeviceRow::into_device).collect()
    }

    async fn switch_tunnel_rules_to_direct(
        &self,
        tunnel_id: &str,
        now: &str,
    ) -> anyhow::Result<Vec<String>> {
        // The tunnel_id is embedded in the JSON: {"type":"tunnel","tunnel_id":"<uuid>"}
        let pattern = format!("%\"tunnel_id\":\"{tunnel_id}\"%");
        let direct_json = r#"{"type":"direct"}"#;

        // Find affected device IDs first.
        let device_ids: Vec<String> =
            sqlx::query_scalar("SELECT device_id FROM routing_rules WHERE target_json LIKE ?")
                .bind(&pattern)
                .fetch_all(&self.pools.read)
                .await?;

        if !device_ids.is_empty() {
            sqlx::query(
                "UPDATE routing_rules SET target_json = ?, updated_at = ? WHERE target_json LIKE ?",
            )
            .bind(direct_json)
            .bind(now)
            .bind(&pattern)
            .execute(&self.pools.write)
            .await?;
        }

        Ok(device_ids)
    }

    async fn update_admin_locked(&self, id: &str, locked: bool) -> anyhow::Result<()> {
        sqlx::query("UPDATE devices SET admin_locked = ? WHERE id = ?")
            .bind(locked)
            .bind(id)
            .execute(&self.pools.write)
            .await?;
        Ok(())
    }

    async fn count(&self) -> anyhow::Result<i64> {
        let count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM devices")
            .fetch_one(&self.pools.read)
            .await?;
        Ok(count)
    }

    async fn update_dns_capture_settings(
        &self,
        id: &str,
        enabled: bool,
        cap_count: i64,
        cap_days: i64,
    ) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE devices SET dns_capture_enabled = ?, dns_capture_cap_count = ?, dns_capture_cap_days = ? WHERE id = ?",
        )
        .bind(enabled)
        .bind(cap_count)
        .bind(cap_days)
        .bind(id)
        .execute(&self.pools.write)
        .await?;
        Ok(())
    }

    async fn find_all_capture_enabled_ids(&self) -> anyhow::Result<Vec<String>> {
        let ids =
            sqlx::query_scalar::<_, String>("SELECT id FROM devices WHERE dns_capture_enabled = 1")
                .fetch_all(&self.pools.read)
                .await?;
        Ok(ids)
    }
}
