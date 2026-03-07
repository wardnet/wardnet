use async_trait::async_trait;
use sqlx::SqlitePool;
use wardnet_types::device::{Device, DeviceType};
use wardnet_types::routing::{RoutingRule, RoutingTarget, RuleCreator};

/// Data access for devices and their routing rules.
///
/// Provides lookups by IP and ID, routing rule queries, and upserts.
/// All business logic (e.g. admin-lock checks) belongs in
/// [`DeviceService`](crate::service::DeviceService).
#[async_trait]
pub trait DeviceRepository: Send + Sync {
    /// Find a device by its most recently observed IP address.
    async fn find_by_ip(&self, ip: &str) -> anyhow::Result<Option<Device>>;

    /// Find a device by its primary key.
    async fn find_by_id(&self, id: &str) -> anyhow::Result<Option<Device>>;

    /// Return the routing rule for a device, if one exists.
    async fn find_rule_for_device(&self, device_id: &str) -> anyhow::Result<Option<RoutingRule>>;

    /// Insert or update a user-created routing rule for a device.
    async fn upsert_user_rule(
        &self,
        device_id: &str,
        target_json: &str,
        now: &str,
    ) -> anyhow::Result<()>;

    /// Return the total number of devices.
    async fn count(&self) -> anyhow::Result<i64>;
}

/// SQLite-backed implementation of [`DeviceRepository`].
pub struct SqliteDeviceRepository {
    pool: SqlitePool,
}

impl SqliteDeviceRepository {
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

#[async_trait]
impl DeviceRepository for SqliteDeviceRepository {
    async fn find_by_ip(&self, ip: &str) -> anyhow::Result<Option<Device>> {
        let row = sqlx::query_as::<_, DeviceRow>(
            "SELECT id, mac, name, hostname, device_type, first_seen, last_seen, last_ip, admin_locked \
             FROM devices WHERE last_ip = ?",
        )
        .bind(ip)
        .fetch_optional(&self.pool)
        .await?;
        row.map(DeviceRow::into_device).transpose()
    }

    async fn find_by_id(&self, id: &str) -> anyhow::Result<Option<Device>> {
        let row = sqlx::query_as::<_, DeviceRow>(
            "SELECT id, mac, name, hostname, device_type, first_seen, last_seen, last_ip, admin_locked \
             FROM devices WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(DeviceRow::into_device).transpose()
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

    async fn count(&self) -> anyhow::Result<i64> {
        let count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM devices")
            .fetch_one(&self.pool)
            .await?;
        Ok(count)
    }
}
