use async_trait::async_trait;
use chrono::Utc;
use sqlx::SqlitePool;
use wardnet_types::dhcp::{
    DhcpLease, DhcpLeaseEventType, DhcpLeaseLog, DhcpLeaseStatus, DhcpReservation,
};

use super::super::dhcp::{DhcpLeaseLogRow, DhcpLeaseRow, DhcpRepository, DhcpReservationRow};

/// SQLite-backed implementation of [`DhcpRepository`].
pub struct SqliteDhcpRepository {
    pool: SqlitePool,
}

impl SqliteDhcpRepository {
    /// Create a new repository backed by the given connection pool.
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

// ── Internal DB row types ───────────────────────────────────────────────

/// Raw row from the `dhcp_leases` table used for internal mapping.
#[derive(sqlx::FromRow)]
struct DbLeaseRow {
    id: String,
    mac_address: String,
    ip_address: String,
    hostname: Option<String>,
    lease_start: String,
    lease_end: String,
    status: String,
    device_id: Option<String>,
    created_at: String,
    updated_at: String,
}

impl DbLeaseRow {
    /// Convert the raw database row into a domain [`DhcpLease`].
    fn into_lease(self) -> anyhow::Result<DhcpLease> {
        let status = match self.status.as_str() {
            "active" => DhcpLeaseStatus::Active,
            "expired" => DhcpLeaseStatus::Expired,
            "released" => DhcpLeaseStatus::Released,
            other => anyhow::bail!("unknown DHCP lease status: {other}"),
        };

        let device_id = self.device_id.as_deref().map(str::parse).transpose()?;

        Ok(DhcpLease {
            id: self.id.parse()?,
            mac_address: self.mac_address,
            ip_address: self.ip_address.parse()?,
            hostname: self.hostname,
            lease_start: self.lease_start.parse()?,
            lease_end: self.lease_end.parse()?,
            status,
            device_id,
            created_at: self.created_at.parse()?,
            updated_at: self.updated_at.parse()?,
        })
    }
}

/// Raw row from the `dhcp_reservations` table used for internal mapping.
#[derive(sqlx::FromRow)]
struct DbReservationRow {
    id: String,
    mac_address: String,
    ip_address: String,
    hostname: Option<String>,
    description: Option<String>,
    created_at: String,
    updated_at: String,
}

impl DbReservationRow {
    /// Convert the raw database row into a domain [`DhcpReservation`].
    fn into_reservation(self) -> anyhow::Result<DhcpReservation> {
        Ok(DhcpReservation {
            id: self.id.parse()?,
            mac_address: self.mac_address,
            ip_address: self.ip_address.parse()?,
            hostname: self.hostname,
            description: self.description,
            created_at: self.created_at.parse()?,
            updated_at: self.updated_at.parse()?,
        })
    }
}

/// Raw row from the `dhcp_lease_log` table used for internal mapping.
#[derive(sqlx::FromRow)]
struct DbLeaseLogRow {
    id: i64,
    lease_id: String,
    event_type: String,
    details: Option<String>,
    created_at: String,
}

impl DbLeaseLogRow {
    /// Convert the raw database row into a domain [`DhcpLeaseLog`].
    fn into_log(self) -> anyhow::Result<DhcpLeaseLog> {
        let event_type = match self.event_type.as_str() {
            "assigned" => DhcpLeaseEventType::Assigned,
            "renewed" => DhcpLeaseEventType::Renewed,
            "released" => DhcpLeaseEventType::Released,
            "expired" => DhcpLeaseEventType::Expired,
            "conflict" => DhcpLeaseEventType::Conflict,
            other => anyhow::bail!("unknown DHCP lease event type: {other}"),
        };

        Ok(DhcpLeaseLog {
            id: self.id,
            lease_id: self.lease_id.parse()?,
            event_type,
            details: self.details,
            created_at: self.created_at.parse()?,
        })
    }
}

// ── Trait implementation ────────────────────────────────────────────────

#[async_trait]
impl DhcpRepository for SqliteDhcpRepository {
    // ── Leases ──────────────────────────────────────────────────────

    async fn insert_lease(&self, row: &DhcpLeaseRow) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO dhcp_leases (id, mac_address, ip_address, hostname, \
             lease_start, lease_end, status, device_id) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&row.id)
        .bind(&row.mac_address)
        .bind(&row.ip_address)
        .bind(&row.hostname)
        .bind(&row.lease_start)
        .bind(&row.lease_end)
        .bind(&row.status)
        .bind(&row.device_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn find_lease_by_id(&self, id: &str) -> anyhow::Result<Option<DhcpLease>> {
        let row = sqlx::query_as::<_, DbLeaseRow>(
            "SELECT id, mac_address, ip_address, hostname, lease_start, lease_end, \
             status, device_id, created_at, updated_at \
             FROM dhcp_leases WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        row.map(DbLeaseRow::into_lease).transpose()
    }

    async fn find_active_lease_by_mac(&self, mac: &str) -> anyhow::Result<Option<DhcpLease>> {
        let row = sqlx::query_as::<_, DbLeaseRow>(
            "SELECT id, mac_address, ip_address, hostname, lease_start, lease_end, \
             status, device_id, created_at, updated_at \
             FROM dhcp_leases WHERE mac_address = ? AND status = 'active'",
        )
        .bind(mac)
        .fetch_optional(&self.pool)
        .await?;

        row.map(DbLeaseRow::into_lease).transpose()
    }

    async fn find_active_lease_by_ip(&self, ip: &str) -> anyhow::Result<Option<DhcpLease>> {
        let row = sqlx::query_as::<_, DbLeaseRow>(
            "SELECT id, mac_address, ip_address, hostname, lease_start, lease_end, \
             status, device_id, created_at, updated_at \
             FROM dhcp_leases WHERE ip_address = ? AND status = 'active'",
        )
        .bind(ip)
        .fetch_optional(&self.pool)
        .await?;

        row.map(DbLeaseRow::into_lease).transpose()
    }

    async fn list_active_leases(&self) -> anyhow::Result<Vec<DhcpLease>> {
        let rows = sqlx::query_as::<_, DbLeaseRow>(
            "SELECT id, mac_address, ip_address, hostname, lease_start, lease_end, \
             status, device_id, created_at, updated_at \
             FROM dhcp_leases WHERE status = 'active'",
        )
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(DbLeaseRow::into_lease).collect()
    }

    async fn update_lease_status(&self, id: &str, status: &str) -> anyhow::Result<()> {
        let now = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        sqlx::query("UPDATE dhcp_leases SET status = ?, updated_at = ? WHERE id = ?")
            .bind(status)
            .bind(&now)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn renew_lease(&self, id: &str, new_end: &str) -> anyhow::Result<()> {
        let now = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        sqlx::query("UPDATE dhcp_leases SET lease_end = ?, updated_at = ? WHERE id = ?")
            .bind(new_end)
            .bind(&now)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn expire_stale_leases(&self) -> anyhow::Result<u64> {
        let now = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        let result = sqlx::query(
            "UPDATE dhcp_leases SET status = 'expired', updated_at = ? \
             WHERE status = 'active' AND lease_end < ?",
        )
        .bind(&now)
        .bind(&now)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected())
    }

    // ── Reservations ────────────────────────────────────────────────

    async fn insert_reservation(&self, row: &DhcpReservationRow) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO dhcp_reservations (id, mac_address, ip_address, hostname, description) \
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&row.id)
        .bind(&row.mac_address)
        .bind(&row.ip_address)
        .bind(&row.hostname)
        .bind(&row.description)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn delete_reservation(&self, id: &str) -> anyhow::Result<()> {
        sqlx::query("DELETE FROM dhcp_reservations WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn list_reservations(&self) -> anyhow::Result<Vec<DhcpReservation>> {
        let rows = sqlx::query_as::<_, DbReservationRow>(
            "SELECT id, mac_address, ip_address, hostname, description, created_at, updated_at \
             FROM dhcp_reservations",
        )
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter()
            .map(DbReservationRow::into_reservation)
            .collect()
    }

    async fn find_reservation_by_mac(&self, mac: &str) -> anyhow::Result<Option<DhcpReservation>> {
        let row = sqlx::query_as::<_, DbReservationRow>(
            "SELECT id, mac_address, ip_address, hostname, description, created_at, updated_at \
             FROM dhcp_reservations WHERE mac_address = ?",
        )
        .bind(mac)
        .fetch_optional(&self.pool)
        .await?;

        row.map(DbReservationRow::into_reservation).transpose()
    }

    async fn find_reservation_by_ip(&self, ip: &str) -> anyhow::Result<Option<DhcpReservation>> {
        let row = sqlx::query_as::<_, DbReservationRow>(
            "SELECT id, mac_address, ip_address, hostname, description, created_at, updated_at \
             FROM dhcp_reservations WHERE ip_address = ?",
        )
        .bind(ip)
        .fetch_optional(&self.pool)
        .await?;

        row.map(DbReservationRow::into_reservation).transpose()
    }

    // ── Lease log ───────────────────────────────────────────────────

    async fn insert_lease_log(&self, row: &DhcpLeaseLogRow) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO dhcp_lease_log (lease_id, event_type, details) \
             VALUES (?, ?, ?)",
        )
        .bind(&row.lease_id)
        .bind(&row.event_type)
        .bind(&row.details)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn find_lease_logs(&self, lease_id: &str) -> anyhow::Result<Vec<DhcpLeaseLog>> {
        let rows = sqlx::query_as::<_, DbLeaseLogRow>(
            "SELECT id, lease_id, event_type, details, created_at \
             FROM dhcp_lease_log WHERE lease_id = ? ORDER BY created_at",
        )
        .bind(lease_id)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(DbLeaseLogRow::into_log).collect()
    }
}
