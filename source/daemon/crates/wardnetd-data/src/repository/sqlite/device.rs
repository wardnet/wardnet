use async_trait::async_trait;
use sqlx::SqlitePool;
use wardnet_common::device::{Device, DeviceConnectionMode, DeviceType, ManufacturerSource};
use wardnet_common::routing::{RoutingRule, RoutingTarget, RuleCreator};

use super::super::DeviceRepository;
use super::super::device::{DeviceRow as InsertDeviceRow, PrunedDevice};
use crate::db::DbPools;
use crate::repository::device::UnknownOwnerError;

/// `SQLite`-backed implementation of [`DeviceRepository`].
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
    manufacturer_source: Option<String>,
    is_randomized: i32,
    device_type: String,
    first_seen: String,
    last_seen: String,
    last_ip: String,
    admin_locked: i32,
    zone_id: String,
    owner_user_id: Option<String>,
    dns_capture_enabled: i32,
    dns_capture_cap_count: i64,
    dns_capture_cap_days: i64,
    connection_mode: String,
    managed: i32,
}

impl DeviceRow {
    fn into_device(self) -> anyhow::Result<Device> {
        let device_type: DeviceType = serde_json::from_str(&format!("\"{}\"", self.device_type))
            .unwrap_or(DeviceType::Unknown);
        let connection_mode: DeviceConnectionMode =
            serde_json::from_str(&format!("\"{}\"", self.connection_mode))
                .unwrap_or(DeviceConnectionMode::Lan);
        Ok(Device {
            id: self.id.parse()?,
            mac: self.mac,
            name: self.name,
            hostname: self.hostname,
            manufacturer: self.manufacturer.clone(),
            // Provenance is only meaningful alongside a name. A stored source
            // with no manufacturer would be a contradiction, so drop it rather
            // than surfacing a dangling "likely" with nothing to qualify.
            manufacturer_source: self
                .manufacturer_source
                .as_deref()
                .filter(|_| self.manufacturer.is_some())
                .and_then(manufacturer_source_from_db),
            is_randomized: self.is_randomized != 0,
            device_type,
            first_seen: self.first_seen.parse()?,
            last_seen: self.last_seen.parse()?,
            last_ip: self.last_ip,
            admin_locked: self.admin_locked != 0,
            zone_id: self.zone_id.parse()?,
            // A malformed owner id is dropped rather than failing the read: it
            // is attribution, and a device must stay visible and manageable
            // even if this column is somehow corrupt.
            owner_user_id: self.owner_user_id.as_deref().and_then(|s| s.parse().ok()),
            dns_capture_enabled: self.dns_capture_enabled != 0,
            dns_capture_cap_count: self.dns_capture_cap_count,
            dns_capture_cap_days: self.dns_capture_cap_days,
            connection_mode,
            managed: self.managed != 0,
        })
    }
}

/// Parse a stored `manufacturer_source` string back into the enum.
///
/// Returns `None` for an unrecognised value rather than defaulting to a
/// variant: guessing here would silently upgrade a hedged `catalog` guess into
/// an authoritative `ieee` claim, or vice versa.
fn manufacturer_source_from_db(raw: &str) -> Option<ManufacturerSource> {
    serde_json::from_str(&format!("\"{raw}\"")).ok()
}

/// Serialize a [`ManufacturerSource`] to its stored `snake_case` string form
/// (mirrors how `device_type` and `connection_mode` are persisted).
fn manufacturer_source_to_db(source: ManufacturerSource) -> Option<String> {
    serde_json::to_string(&source)
        .ok()
        .map(|s| s.trim_matches('"').to_owned())
}

/// Serialize a [`DeviceConnectionMode`] to its stored `snake_case` string form
/// (mirrors how `device_type` is persisted). Infallible for this closed enum;
/// falls back to `"lan"` defensively.
fn connection_mode_to_db(mode: DeviceConnectionMode) -> String {
    serde_json::to_string(&mode)
        .map_or_else(|_| "lan".to_owned(), |s| s.trim_matches('"').to_owned())
}

/// Raw row from the `routing_rules` table used for internal mapping.
#[derive(sqlx::FromRow)]
struct RuleRow {
    device_id: String,
    target_json: String,
    created_by: String,
}

// Static SQL strings — every column list is inlined so no runtime format!()
// allocation is needed for these fixed SELECTs. The genuinely dynamic queries
// further down (JOIN/LIKE against routing_rules) keep their own literals.
const FIND_BY_IP_SQL: &str = "SELECT id, mac, name, hostname, manufacturer, manufacturer_source, is_randomized, device_type, first_seen, last_seen, last_ip, admin_locked, zone_id, owner_user_id, dns_capture_enabled, dns_capture_cap_count, dns_capture_cap_days, connection_mode, managed \
     FROM devices WHERE last_ip = ? ORDER BY last_seen DESC LIMIT 1";
const FIND_ALL_BY_IP_SQL: &str = "SELECT id, mac, name, hostname, manufacturer, manufacturer_source, is_randomized, device_type, first_seen, last_seen, last_ip, admin_locked, zone_id, owner_user_id, dns_capture_enabled, dns_capture_cap_count, dns_capture_cap_days, connection_mode, managed \
     FROM devices WHERE last_ip = ? ORDER BY last_seen DESC";
const FIND_BY_ID_SQL: &str = "SELECT id, mac, name, hostname, manufacturer, manufacturer_source, is_randomized, device_type, first_seen, last_seen, last_ip, admin_locked, zone_id, owner_user_id, dns_capture_enabled, dns_capture_cap_count, dns_capture_cap_days, connection_mode, managed \
     FROM devices WHERE id = ?";
const FIND_BY_MAC_SQL: &str = "SELECT id, mac, name, hostname, manufacturer, manufacturer_source, is_randomized, device_type, first_seen, last_seen, last_ip, admin_locked, zone_id, owner_user_id, dns_capture_enabled, dns_capture_cap_count, dns_capture_cap_days, connection_mode, managed \
     FROM devices WHERE mac = ?";
const FIND_ALL_SQL: &str = "SELECT id, mac, name, hostname, manufacturer, manufacturer_source, is_randomized, device_type, first_seen, last_seen, last_ip, admin_locked, zone_id, owner_user_id, dns_capture_enabled, dns_capture_cap_count, dns_capture_cap_days, connection_mode, managed \
     FROM devices ORDER BY last_seen DESC";
const DELETE_UNMANAGED_BEFORE_SQL: &str =
    "DELETE FROM devices WHERE managed = 0 AND last_seen < ? RETURNING id, mac, last_seen";
const FIND_STALE_SQL: &str = "SELECT id, mac, name, hostname, manufacturer, manufacturer_source, is_randomized, device_type, first_seen, last_seen, last_ip, admin_locked, zone_id, owner_user_id, dns_capture_enabled, dns_capture_cap_count, dns_capture_cap_days, connection_mode, managed \
     FROM devices WHERE last_seen < ?";

/// Re-wrap a `SQLite` foreign-key failure on `owner_user_id` as
/// [`UnknownOwnerError`], passing everything else through unchanged.
fn map_unknown_owner(err: sqlx::Error) -> anyhow::Error {
    if let sqlx::Error::Database(ref db_err) = err
        && db_err.message().contains("FOREIGN KEY constraint failed")
    {
        return anyhow::Error::new(UnknownOwnerError);
    }
    anyhow::Error::new(err)
}

#[async_trait]
impl DeviceRepository for SqliteDeviceRepository {
    async fn find_by_ip(&self, ip: &str) -> anyhow::Result<Option<Device>> {
        // Empty string is the "no known address" sentinel written to departed
        // devices (see `clear_last_ip` / discovery `restore_devices`), not a
        // real address. Never match on it: every departed device shares it, so
        // an empty lookup would otherwise resolve to an arbitrary departed row
        // (mirrors the `is_empty` guard in `DeviceIpSnapshot`).
        if ip.is_empty() {
            return Ok(None);
        }

        // `last_ip` is not unique: departed devices keep their row, so DHCP
        // recycling an address to a new device leaves two rows sharing an IP.
        // Order by `last_seen DESC` so the lookup always resolves to the most
        // recently seen (live) device rather than an arbitrary rowid-ordered
        // (stale) one — the IP-keyed self-service auth path relies on this to
        // identify the caller's own device.
        let row = sqlx::query_as::<_, DeviceRow>(FIND_BY_IP_SQL)
            .bind(ip)
            .fetch_optional(&self.pools.read)
            .await?;
        row.map(DeviceRow::into_device).transpose()
    }

    async fn find_all_by_ip(&self, ip: &str) -> anyhow::Result<Vec<Device>> {
        // Empty string is the departed-device sentinel (see `find_by_ip`); never
        // match it, or every departed row would resolve to one address.
        if ip.is_empty() {
            return Ok(Vec::new());
        }
        // Indexed `last_ip` lookup rather than the trait's default full-table
        // scan: the mDNS observer resolves against this on every advertisement.
        let rows = sqlx::query_as::<_, DeviceRow>(FIND_ALL_BY_IP_SQL)
            .bind(ip)
            .fetch_all(&self.pools.read)
            .await?;
        rows.into_iter().map(DeviceRow::into_device).collect()
    }

    async fn find_by_id(&self, id: &str) -> anyhow::Result<Option<Device>> {
        let row = sqlx::query_as::<_, DeviceRow>(FIND_BY_ID_SQL)
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
        let row = sqlx::query_as::<_, DeviceRow>(FIND_BY_MAC_SQL)
            .bind(mac.to_lowercase())
            .fetch_optional(&self.pools.read)
            .await?;
        row.map(DeviceRow::into_device).transpose()
    }

    async fn find_all(&self) -> anyhow::Result<Vec<Device>> {
        let rows = sqlx::query_as::<_, DeviceRow>(FIND_ALL_SQL)
            .fetch_all(&self.pools.read)
            .await?;
        rows.into_iter().map(DeviceRow::into_device).collect()
    }

    async fn insert(&self, device: &InsertDeviceRow) -> anyhow::Result<()> {
        // `managed` is deliberately absent from the column list: discovery is
        // the only path that inserts, and a freshly discovered device is never
        // managed. It takes the schema DEFAULT 0 and is promoted later, only by
        // an explicit admin configuration act (`set_managed`).
        sqlx::query(
            "INSERT INTO devices (id, mac, hostname, manufacturer, manufacturer_source, is_randomized, device_type, first_seen, last_seen, last_ip, zone_id, connection_mode) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&device.id)
        .bind(device.mac.to_lowercase())
        .bind(&device.hostname)
        .bind(&device.manufacturer)
        .bind(device.manufacturer_source.and_then(manufacturer_source_to_db))
        .bind(i32::from(device.is_randomized))
        .bind(&device.device_type)
        .bind(&device.first_seen)
        .bind(&device.last_seen)
        .bind(&device.last_ip)
        .bind(&device.zone_id)
        .bind(connection_mode_to_db(device.connection_mode))
        .execute(&self.pools.write)
        .await?;
        Ok(())
    }

    async fn update_last_seen_and_ip(
        &self,
        id: &str,
        ip: &str,
        last_seen: &str,
        mode: DeviceConnectionMode,
    ) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE devices SET last_seen = ?, last_ip = ?, connection_mode = ? WHERE id = ?",
        )
        .bind(last_seen)
        .bind(ip)
        .bind(connection_mode_to_db(mode))
        .bind(id)
        .execute(&self.pools.write)
        .await?;
        Ok(())
    }

    async fn clear_last_ip(&self, id: &str) -> anyhow::Result<()> {
        // Empty the address rather than deleting the row: the device history is
        // still wanted, but a departed row must never again match a live
        // request's source IP in `find_by_ip`. Empty string is the established
        // "no known address" sentinel (see discovery `restore_devices`); the
        // NOT NULL column forbids a true NULL.
        sqlx::query("UPDATE devices SET last_ip = '' WHERE id = ?")
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

    async fn update_connection_mode(
        &self,
        id: &str,
        mode: DeviceConnectionMode,
    ) -> anyhow::Result<()> {
        sqlx::query("UPDATE devices SET connection_mode = ? WHERE id = ?")
            .bind(connection_mode_to_db(mode))
            .bind(id)
            .execute(&self.pools.write)
            .await?;
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
        let rows = sqlx::query_as::<_, DeviceRow>(FIND_STALE_SQL)
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

    async fn delete_rule_for_device(&self, device_id: &str) -> anyhow::Result<()> {
        sqlx::query("DELETE FROM routing_rules WHERE device_id = ?")
            .bind(device_id)
            .execute(&self.pools.write)
            .await?;
        Ok(())
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
        let query = "SELECT d.id, d.mac, d.name, d.hostname, d.manufacturer, \
             d.manufacturer_source, d.is_randomized, d.device_type, \
             d.first_seen, d.last_seen, d.last_ip, d.admin_locked, d.zone_id, \
             d.owner_user_id, \
             d.dns_capture_enabled, d.dns_capture_cap_count, d.dns_capture_cap_days, \
             d.connection_mode, d.managed \
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

    async fn assign_zone(&self, device_id: &str, zone_id: &str) -> anyhow::Result<bool> {
        let result = sqlx::query("UPDATE devices SET zone_id = ? WHERE id = ?")
            .bind(zone_id)
            .bind(device_id)
            .execute(&self.pools.write)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn set_owner(
        &self,
        device_id: &str,
        owner_user_id: Option<&str>,
    ) -> anyhow::Result<bool> {
        let result = sqlx::query("UPDATE devices SET owner_user_id = ? WHERE id = ?")
            .bind(owner_user_id)
            .bind(device_id)
            .execute(&self.pools.write)
            .await
            .map_err(map_unknown_owner)?;
        Ok(result.rows_affected() > 0)
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
        enabled: Option<bool>,
        cap_count: Option<i64>,
        cap_days: Option<i64>,
    ) -> anyhow::Result<bool> {
        let result = sqlx::query(
            "UPDATE devices \
             SET dns_capture_enabled   = COALESCE(?, dns_capture_enabled), \
                 dns_capture_cap_count = COALESCE(?, dns_capture_cap_count), \
                 dns_capture_cap_days  = COALESCE(?, dns_capture_cap_days) \
             WHERE id = ?",
        )
        .bind(enabled)
        .bind(cap_count)
        .bind(cap_days)
        .bind(id)
        .execute(&self.pools.write)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn find_all_capture_enabled_ids(&self) -> anyhow::Result<Vec<String>> {
        let ids =
            sqlx::query_scalar::<_, String>("SELECT id FROM devices WHERE dns_capture_enabled = 1")
                .fetch_all(&self.pools.read)
                .await?;
        Ok(ids)
    }

    async fn set_managed(&self, id: &str, managed: bool) -> anyhow::Result<()> {
        sqlx::query("UPDATE devices SET managed = ? WHERE id = ?")
            .bind(i32::from(managed))
            .bind(id)
            .execute(&self.pools.write)
            .await?;
        Ok(())
    }

    async fn delete_unmanaged_before(&self, cutoff: &str) -> anyhow::Result<Vec<PrunedDevice>> {
        // `managed = 0` is the whole safety argument: an unmanaged device has
        // no admin artefacts, so this DELETE cannot orphan anything. The
        // per-device rows that do exist are either ON DELETE CASCADE
        // (dns_events, device_signals, …), ON DELETE SET NULL (dhcp_leases),
        // self-expiring (dns_query_log), or retained as historical truth
        // (stats_*, notifications).
        //
        // RETURNING (precedent: sqlite/dns_local.rs) makes the delete and the
        // "what was deleted" read one atomic statement — a separate SELECT
        // could race a concurrent observation and report a row that survived.
        let rows = sqlx::query_as::<_, (String, String, String)>(DELETE_UNMANAGED_BEFORE_SQL)
            .bind(cutoff)
            .fetch_all(&self.pools.write)
            .await?;
        Ok(rows
            .into_iter()
            .map(|(id, mac, last_seen)| PrunedDevice { id, mac, last_seen })
            .collect())
    }
}
