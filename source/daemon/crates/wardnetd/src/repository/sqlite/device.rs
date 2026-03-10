use async_trait::async_trait;
use sqlx::SqlitePool;
use wardnet_types::device::{Device, DeviceType};
use wardnet_types::routing::{RoutingRule, RoutingTarget, RuleCreator};

use super::super::DeviceRepository;
use super::super::device::DeviceRow as InsertDeviceRow;

/// SQLite-backed implementation of [`DeviceRepository`].
pub struct SqliteDeviceRepository {
    pool: SqlitePool,
}

impl SqliteDeviceRepository {
    /// Create a new repository backed by the given connection pool.
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
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

const SELECT_COLS: &str = "id, mac, name, hostname, manufacturer, device_type, first_seen, last_seen, last_ip, admin_locked";

#[async_trait]
impl DeviceRepository for SqliteDeviceRepository {
    async fn find_by_ip(&self, ip: &str) -> anyhow::Result<Option<Device>> {
        let query = format!("SELECT {SELECT_COLS} FROM devices WHERE last_ip = ?");
        let row = sqlx::query_as::<_, DeviceRow>(&query)
            .bind(ip)
            .fetch_optional(&self.pool)
            .await?;
        row.map(DeviceRow::into_device).transpose()
    }

    async fn find_by_id(&self, id: &str) -> anyhow::Result<Option<Device>> {
        let query = format!("SELECT {SELECT_COLS} FROM devices WHERE id = ?");
        let row = sqlx::query_as::<_, DeviceRow>(&query)
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        row.map(DeviceRow::into_device).transpose()
    }

    async fn find_by_mac(&self, mac: &str) -> anyhow::Result<Option<Device>> {
        let query = format!("SELECT {SELECT_COLS} FROM devices WHERE mac = ?");
        let row = sqlx::query_as::<_, DeviceRow>(&query)
            .bind(mac)
            .fetch_optional(&self.pool)
            .await?;
        row.map(DeviceRow::into_device).transpose()
    }

    async fn find_all(&self) -> anyhow::Result<Vec<Device>> {
        let query = format!("SELECT {SELECT_COLS} FROM devices ORDER BY last_seen DESC");
        let rows = sqlx::query_as::<_, DeviceRow>(&query)
            .fetch_all(&self.pool)
            .await?;
        rows.into_iter().map(DeviceRow::into_device).collect()
    }

    async fn insert(&self, device: &InsertDeviceRow) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO devices (id, mac, hostname, manufacturer, device_type, first_seen, last_seen, last_ip) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&device.id)
        .bind(&device.mac)
        .bind(&device.hostname)
        .bind(&device.manufacturer)
        .bind(&device.device_type)
        .bind(&device.first_seen)
        .bind(&device.last_seen)
        .bind(&device.last_ip)
        .execute(&self.pool)
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
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn update_last_seen_batch(&self, updates: &[(String, String)]) -> anyhow::Result<()> {
        let mut tx = self.pool.begin().await?;
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
            .execute(&self.pool)
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
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn find_stale(&self, before: &str) -> anyhow::Result<Vec<Device>> {
        let query = format!("SELECT {SELECT_COLS} FROM devices WHERE last_seen < ?");
        let rows = sqlx::query_as::<_, DeviceRow>(&query)
            .bind(before)
            .fetch_all(&self.pool)
            .await?;
        rows.into_iter().map(DeviceRow::into_device).collect()
    }

    async fn find_rule_for_device(&self, device_id: &str) -> anyhow::Result<Option<RoutingRule>> {
        let row = sqlx::query_as::<_, RuleRow>(
            "SELECT device_id, target_json, created_by FROM routing_rules WHERE device_id = ?",
        )
        .bind(device_id)
        .fetch_optional(&self.pool)
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
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn update_admin_locked(&self, id: &str, locked: bool) -> anyhow::Result<()> {
        sqlx::query("UPDATE devices SET admin_locked = ? WHERE id = ?")
            .bind(locked)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn count(&self) -> anyhow::Result<i64> {
        let count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM devices")
            .fetch_one(&self.pool)
            .await?;
        Ok(count)
    }
}
