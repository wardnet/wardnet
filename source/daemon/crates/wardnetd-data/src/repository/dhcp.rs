use async_trait::async_trait;
use wardnet_common::dhcp::{DhcpLease, DhcpLeaseLog, DhcpReservation};

/// Row struct for inserting a new DHCP lease.
#[derive(Debug, Clone)]
pub struct DhcpLeaseRow {
    /// UUID primary key.
    pub id: String,
    /// Client MAC address.
    pub mac_address: String,
    /// Assigned IPv4 address.
    pub ip_address: String,
    /// Client-provided hostname (option 12).
    pub hostname: Option<String>,
    /// ISO-8601 UTC timestamp when the lease starts.
    pub lease_start: String,
    /// ISO-8601 UTC timestamp when the lease expires.
    pub lease_end: String,
    /// Lease status string (`"active"`, `"expired"`, `"released"`).
    pub status: String,
    /// Optional link to a known device.
    pub device_id: Option<String>,
}

/// Row struct for inserting a new DHCP reservation.
#[derive(Debug, Clone)]
pub struct DhcpReservationRow {
    /// UUID primary key.
    pub id: String,
    /// MAC address to reserve for.
    pub mac_address: String,
    /// Static IPv4 address to assign.
    pub ip_address: String,
    /// Optional hostname to push to the client.
    pub hostname: Option<String>,
    /// Optional human-readable description.
    pub description: Option<String>,
}

/// Row struct for inserting a DHCP lease log entry.
#[derive(Debug, Clone)]
pub struct DhcpLeaseLogRow {
    /// The lease this event belongs to.
    pub lease_id: String,
    /// Event type string (`"assigned"`, `"renewed"`, `"released"`, `"expired"`, `"conflict"`).
    pub event_type: String,
    /// Optional free-form details about the event.
    pub details: Option<String>,
}

/// Repository trait for DHCP lease and reservation persistence.
///
/// Abstracts data access for the built-in DHCP server. Business logic
/// (pool allocation, conflict detection) belongs in the service layer.
#[async_trait]
pub trait DhcpRepository: Send + Sync {
    // ── Leases ──────────────────────────────────────────────────────

    /// Insert a new DHCP lease record.
    async fn insert_lease(&self, row: &DhcpLeaseRow) -> anyhow::Result<()>;

    /// Find a lease by primary key.
    async fn find_lease_by_id(&self, id: &str) -> anyhow::Result<Option<DhcpLease>>;

    /// Find the currently active lease for a given MAC address.
    async fn find_active_lease_by_mac(&self, mac: &str) -> anyhow::Result<Option<DhcpLease>>;

    /// Find the currently active lease for a given IP address.
    async fn find_active_lease_by_ip(&self, ip: &str) -> anyhow::Result<Option<DhcpLease>>;

    /// Return all leases with `status = 'active'`.
    async fn list_active_leases(&self) -> anyhow::Result<Vec<DhcpLease>>;

    /// Update the status of a lease.
    async fn update_lease_status(&self, id: &str, status: &str) -> anyhow::Result<()>;

    /// Extend a lease by updating its `lease_end` timestamp.
    async fn renew_lease(&self, id: &str, new_end: &str) -> anyhow::Result<()>;

    /// Mark all active leases whose `lease_end` is in the past as expired.
    ///
    /// Returns the number of rows affected.
    async fn expire_stale_leases(&self) -> anyhow::Result<u64>;

    // ── Reservations ────────────────────────────────────────────────

    /// Insert a new static MAC-to-IP reservation.
    async fn insert_reservation(&self, row: &DhcpReservationRow) -> anyhow::Result<()>;

    /// Delete a reservation by primary key.
    async fn delete_reservation(&self, id: &str) -> anyhow::Result<()>;

    /// Return all reservations.
    async fn list_reservations(&self) -> anyhow::Result<Vec<DhcpReservation>>;

    /// Find a reservation by MAC address.
    async fn find_reservation_by_mac(&self, mac: &str) -> anyhow::Result<Option<DhcpReservation>>;

    /// Find a reservation by IP address.
    async fn find_reservation_by_ip(&self, ip: &str) -> anyhow::Result<Option<DhcpReservation>>;

    // ── Lease log ───────────────────────────────────────────────────

    /// Insert a new audit-log entry for a lease event.
    async fn insert_lease_log(&self, row: &DhcpLeaseLogRow) -> anyhow::Result<()>;

    /// Return all log entries for a given lease, ordered by creation time.
    async fn find_lease_logs(&self, lease_id: &str) -> anyhow::Result<Vec<DhcpLeaseLog>>;
}
