use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::Utc;
use uuid::Uuid;
use wardnet_types::api::{
    CreateDhcpReservationRequest, ToggleDhcpRequest, UpdateDhcpConfigRequest,
};
use wardnet_types::auth::AuthContext;
use wardnet_types::dhcp::{DhcpLease, DhcpLeaseLog, DhcpLeaseStatus, DhcpReservation};

use crate::auth_context;
use crate::error::AppError;
use crate::repository::{
    DhcpLeaseLogRow, DhcpLeaseRow, DhcpRepository, DhcpReservationRow, SystemConfigRepository,
};
use crate::service::{DhcpService, DhcpServiceImpl};

// -- Mock DhcpRepository ---------------------------------------------------

/// In-memory mock for `DhcpRepository`.
struct MockDhcpRepository {
    leases: Mutex<Vec<DhcpLeaseRow>>,
    reservations: Mutex<Vec<DhcpReservationRow>>,
    logs: Mutex<Vec<DhcpLeaseLogRow>>,
}

impl MockDhcpRepository {
    fn new() -> Self {
        Self {
            leases: Mutex::new(Vec::new()),
            reservations: Mutex::new(Vec::new()),
            logs: Mutex::new(Vec::new()),
        }
    }
}

#[async_trait]
impl DhcpRepository for MockDhcpRepository {
    async fn insert_lease(&self, row: &DhcpLeaseRow) -> anyhow::Result<()> {
        self.leases.lock().unwrap().push(row.clone());
        Ok(())
    }

    async fn find_lease_by_id(&self, id: &str) -> anyhow::Result<Option<DhcpLease>> {
        let rows = self.leases.lock().unwrap();
        let row = rows.iter().find(|r| r.id == id);
        Ok(row.map(row_to_lease).transpose()?)
    }

    async fn find_active_lease_by_mac(&self, mac: &str) -> anyhow::Result<Option<DhcpLease>> {
        let rows = self.leases.lock().unwrap();
        let row = rows
            .iter()
            .find(|r| r.mac_address == mac && r.status == "active");
        Ok(row.map(row_to_lease).transpose()?)
    }

    async fn find_active_lease_by_ip(&self, ip: &str) -> anyhow::Result<Option<DhcpLease>> {
        let rows = self.leases.lock().unwrap();
        let row = rows
            .iter()
            .find(|r| r.ip_address == ip && r.status == "active");
        Ok(row.map(row_to_lease).transpose()?)
    }

    async fn list_active_leases(&self) -> anyhow::Result<Vec<DhcpLease>> {
        let rows = self.leases.lock().unwrap();
        rows.iter()
            .filter(|r| r.status == "active")
            .map(row_to_lease)
            .collect()
    }

    async fn update_lease_status(&self, id: &str, status: &str) -> anyhow::Result<()> {
        let mut rows = self.leases.lock().unwrap();
        if let Some(r) = rows.iter_mut().find(|r| r.id == id) {
            r.status = status.to_owned();
        }
        Ok(())
    }

    async fn renew_lease(&self, id: &str, new_end: &str) -> anyhow::Result<()> {
        let mut rows = self.leases.lock().unwrap();
        if let Some(r) = rows.iter_mut().find(|r| r.id == id) {
            r.lease_end = new_end.to_owned();
        }
        Ok(())
    }

    async fn expire_stale_leases(&self) -> anyhow::Result<u64> {
        let mut rows = self.leases.lock().unwrap();
        let now = Utc::now();
        let mut count = 0u64;
        for r in rows.iter_mut() {
            if r.status == "active"
                && let Ok(end) = r.lease_end.parse::<chrono::DateTime<Utc>>()
                && end < now
            {
                r.status = "expired".to_owned();
                count += 1;
            }
        }
        Ok(count)
    }

    async fn insert_reservation(&self, row: &DhcpReservationRow) -> anyhow::Result<()> {
        self.reservations.lock().unwrap().push(row.clone());
        Ok(())
    }

    async fn delete_reservation(&self, id: &str) -> anyhow::Result<()> {
        self.reservations.lock().unwrap().retain(|r| r.id != id);
        Ok(())
    }

    async fn list_reservations(&self) -> anyhow::Result<Vec<DhcpReservation>> {
        let rows = self.reservations.lock().unwrap();
        rows.iter().map(row_to_reservation).collect()
    }

    async fn find_reservation_by_mac(&self, mac: &str) -> anyhow::Result<Option<DhcpReservation>> {
        let rows = self.reservations.lock().unwrap();
        let row = rows.iter().find(|r| r.mac_address == mac);
        Ok(row.map(row_to_reservation).transpose()?)
    }

    async fn find_reservation_by_ip(&self, ip: &str) -> anyhow::Result<Option<DhcpReservation>> {
        let rows = self.reservations.lock().unwrap();
        let row = rows.iter().find(|r| r.ip_address == ip);
        Ok(row.map(row_to_reservation).transpose()?)
    }

    async fn insert_lease_log(&self, row: &DhcpLeaseLogRow) -> anyhow::Result<()> {
        self.logs.lock().unwrap().push(row.clone());
        Ok(())
    }

    async fn find_lease_logs(&self, lease_id: &str) -> anyhow::Result<Vec<DhcpLeaseLog>> {
        let rows = self.logs.lock().unwrap();
        Ok(rows
            .iter()
            .filter(|r| r.lease_id == lease_id)
            .map(|r| DhcpLeaseLog {
                id: 0,
                lease_id: r.lease_id.parse().unwrap(),
                event_type: match r.event_type.as_str() {
                    "assigned" => wardnet_types::dhcp::DhcpLeaseEventType::Assigned,
                    "renewed" => wardnet_types::dhcp::DhcpLeaseEventType::Renewed,
                    "released" => wardnet_types::dhcp::DhcpLeaseEventType::Released,
                    "expired" => wardnet_types::dhcp::DhcpLeaseEventType::Expired,
                    _ => wardnet_types::dhcp::DhcpLeaseEventType::Conflict,
                },
                details: r.details.clone(),
                created_at: Utc::now(),
            })
            .collect())
    }
}

/// Convert a `DhcpLeaseRow` to a `DhcpLease`.
fn row_to_lease(r: &DhcpLeaseRow) -> anyhow::Result<DhcpLease> {
    let status = match r.status.as_str() {
        "active" => DhcpLeaseStatus::Active,
        "expired" => DhcpLeaseStatus::Expired,
        _ => DhcpLeaseStatus::Released,
    };
    Ok(DhcpLease {
        id: r.id.parse()?,
        mac_address: r.mac_address.clone(),
        ip_address: r.ip_address.parse()?,
        hostname: r.hostname.clone(),
        lease_start: r.lease_start.parse()?,
        lease_end: r.lease_end.parse()?,
        status,
        device_id: r.device_id.as_ref().map(|s| s.parse()).transpose()?,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    })
}

/// Convert a `DhcpReservationRow` to a `DhcpReservation`.
fn row_to_reservation(r: &DhcpReservationRow) -> anyhow::Result<DhcpReservation> {
    Ok(DhcpReservation {
        id: r.id.parse()?,
        mac_address: r.mac_address.clone(),
        ip_address: r.ip_address.parse()?,
        hostname: r.hostname.clone(),
        description: r.description.clone(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    })
}

// -- Mock SystemConfigRepository -------------------------------------------

/// In-memory mock for `SystemConfigRepository`.
struct MockSystemConfigRepository {
    data: Mutex<HashMap<String, String>>,
}

impl MockSystemConfigRepository {
    fn new() -> Self {
        let mut data = HashMap::new();
        data.insert("dhcp_enabled".to_owned(), "false".to_owned());
        data.insert("dhcp_pool_start".to_owned(), "192.168.1.100".to_owned());
        data.insert("dhcp_pool_end".to_owned(), "192.168.1.200".to_owned());
        data.insert("dhcp_subnet_mask".to_owned(), "255.255.255.0".to_owned());
        data.insert(
            "dhcp_upstream_dns".to_owned(),
            r#"["1.1.1.1","8.8.8.8"]"#.to_owned(),
        );
        data.insert("dhcp_lease_duration_secs".to_owned(), "86400".to_owned());
        data.insert("dhcp_router_ip".to_owned(), String::new());
        Self {
            data: Mutex::new(data),
        }
    }
}

#[async_trait]
impl SystemConfigRepository for MockSystemConfigRepository {
    async fn get(&self, key: &str) -> anyhow::Result<Option<String>> {
        Ok(self.data.lock().unwrap().get(key).cloned())
    }

    async fn set(&self, key: &str, value: &str) -> anyhow::Result<()> {
        self.data
            .lock()
            .unwrap()
            .insert(key.to_owned(), value.to_owned());
        Ok(())
    }

    async fn device_count(&self) -> anyhow::Result<i64> {
        Ok(0)
    }

    async fn tunnel_count(&self) -> anyhow::Result<i64> {
        Ok(0)
    }

    async fn db_size_bytes(&self) -> anyhow::Result<u64> {
        Ok(0)
    }
}

// -- Helpers ---------------------------------------------------------------

/// Helper to create an admin auth context for tests.
fn admin_ctx() -> AuthContext {
    AuthContext::Admin {
        admin_id: Uuid::new_v4(),
    }
}

/// Build a `DhcpServiceImpl` with mock dependencies.
fn build_service() -> DhcpServiceImpl {
    let dhcp = Arc::new(MockDhcpRepository::new());
    let system_config = Arc::new(MockSystemConfigRepository::new());
    DhcpServiceImpl::new(dhcp, system_config)
}

// -- Tests -----------------------------------------------------------------

#[tokio::test]
async fn get_config_admin_success() {
    let svc = build_service();
    let resp = auth_context::with_context(admin_ctx(), svc.get_config())
        .await
        .unwrap();
    assert_eq!(resp.config.pool_start.to_string(), "192.168.1.100");
    assert_eq!(resp.config.pool_end.to_string(), "192.168.1.200");
    assert_eq!(resp.config.subnet_mask.to_string(), "255.255.255.0");
    assert!(!resp.config.enabled);
    assert_eq!(resp.config.lease_duration_secs, 86400);
    assert_eq!(resp.config.upstream_dns.len(), 2);
}

#[tokio::test]
async fn get_config_anonymous_forbidden() {
    let svc = build_service();
    let result = auth_context::with_context(AuthContext::Anonymous, svc.get_config()).await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}

#[tokio::test]
async fn update_config_success() {
    let svc = build_service();
    let req = UpdateDhcpConfigRequest {
        pool_start: "10.0.0.50".to_owned(),
        pool_end: "10.0.0.150".to_owned(),
        subnet_mask: "255.255.255.0".to_owned(),
        upstream_dns: vec!["1.1.1.1".to_owned()],
        lease_duration_secs: 3600,
        router_ip: None,
    };
    let resp = auth_context::with_context(admin_ctx(), svc.update_config(req))
        .await
        .unwrap();
    assert_eq!(resp.config.pool_start.to_string(), "10.0.0.50");
    assert_eq!(resp.config.pool_end.to_string(), "10.0.0.150");
    assert_eq!(resp.config.lease_duration_secs, 3600);
}

#[tokio::test]
async fn update_config_invalid_ip() {
    let svc = build_service();
    let req = UpdateDhcpConfigRequest {
        pool_start: "not.an.ip".to_owned(),
        pool_end: "192.168.1.200".to_owned(),
        subnet_mask: "255.255.255.0".to_owned(),
        upstream_dns: vec!["1.1.1.1".to_owned()],
        lease_duration_secs: 86400,
        router_ip: None,
    };
    let result = auth_context::with_context(admin_ctx(), svc.update_config(req)).await;
    assert!(matches!(result, Err(AppError::BadRequest(_))));
}

#[tokio::test]
async fn update_config_pool_end_before_start() {
    let svc = build_service();
    let req = UpdateDhcpConfigRequest {
        pool_start: "192.168.1.200".to_owned(),
        pool_end: "192.168.1.100".to_owned(),
        subnet_mask: "255.255.255.0".to_owned(),
        upstream_dns: vec!["1.1.1.1".to_owned()],
        lease_duration_secs: 86400,
        router_ip: None,
    };
    let result = auth_context::with_context(admin_ctx(), svc.update_config(req)).await;
    assert!(matches!(result, Err(AppError::BadRequest(_))));
}

#[tokio::test]
async fn toggle_enabled() {
    let svc = build_service();

    // Start disabled (default).
    let resp = auth_context::with_context(admin_ctx(), svc.get_config())
        .await
        .unwrap();
    assert!(!resp.config.enabled);

    // Toggle on.
    let req = ToggleDhcpRequest { enabled: true };
    let resp = auth_context::with_context(admin_ctx(), svc.toggle(req))
        .await
        .unwrap();
    assert!(resp.config.enabled);

    // Verify it persisted.
    let resp = auth_context::with_context(admin_ctx(), svc.get_config())
        .await
        .unwrap();
    assert!(resp.config.enabled);
}

#[tokio::test]
async fn list_leases_empty() {
    let svc = build_service();
    let resp = auth_context::with_context(admin_ctx(), svc.list_leases())
        .await
        .unwrap();
    assert!(resp.leases.is_empty());
}

#[tokio::test]
async fn create_reservation_success() {
    let svc = build_service();
    let req = CreateDhcpReservationRequest {
        mac_address: "AA:BB:CC:DD:EE:01".to_owned(),
        ip_address: "192.168.1.50".to_owned(),
        hostname: Some("my-host".to_owned()),
        description: Some("test reservation".to_owned()),
    };
    let resp = auth_context::with_context(admin_ctx(), svc.create_reservation(req))
        .await
        .unwrap();
    // MAC is normalized to lowercase.
    assert_eq!(resp.reservation.mac_address, "aa:bb:cc:dd:ee:01");
    assert_eq!(resp.reservation.ip_address.to_string(), "192.168.1.50");
    assert_eq!(resp.message, "reservation created");

    // Verify it shows up in the list.
    let list = auth_context::with_context(admin_ctx(), svc.list_reservations())
        .await
        .unwrap();
    assert_eq!(list.reservations.len(), 1);
}

#[tokio::test]
async fn create_reservation_duplicate_mac() {
    let svc = build_service();
    let req = CreateDhcpReservationRequest {
        mac_address: "AA:BB:CC:DD:EE:02".to_owned(),
        ip_address: "192.168.1.51".to_owned(),
        hostname: None,
        description: None,
    };
    // Use different case to verify normalization catches it.
    let req_dup = CreateDhcpReservationRequest {
        mac_address: "aa:bb:cc:dd:ee:02".to_owned(),
        ip_address: "192.168.1.52".to_owned(),
        hostname: None,
        description: None,
    };
    auth_context::with_context(admin_ctx(), svc.create_reservation(req))
        .await
        .unwrap();

    // Second reservation with the same MAC (different case) should conflict.
    let req2 = req_dup;
    let result = auth_context::with_context(admin_ctx(), svc.create_reservation(req2)).await;
    assert!(matches!(result, Err(AppError::Conflict(_))));
}

#[tokio::test]
async fn create_reservation_duplicate_ip() {
    let svc = build_service();
    let req = CreateDhcpReservationRequest {
        mac_address: "AA:BB:CC:DD:EE:01".to_owned(),
        ip_address: "192.168.1.50".to_owned(),
        hostname: None,
        description: None,
    };
    auth_context::with_context(admin_ctx(), svc.create_reservation(req))
        .await
        .unwrap();

    // Different MAC, same IP should conflict.
    let req2 = CreateDhcpReservationRequest {
        mac_address: "AA:BB:CC:DD:EE:99".to_owned(),
        ip_address: "192.168.1.50".to_owned(),
        hostname: None,
        description: None,
    };
    let result = auth_context::with_context(admin_ctx(), svc.create_reservation(req2)).await;
    assert!(matches!(result, Err(AppError::Conflict(_))));
}

#[tokio::test]
async fn delete_reservation_not_found() {
    let svc = build_service();
    let result =
        auth_context::with_context(admin_ctx(), svc.delete_reservation(Uuid::new_v4())).await;
    assert!(matches!(result, Err(AppError::NotFound(_))));
}

#[tokio::test]
async fn revoke_lease_not_found() {
    let svc = build_service();
    let result = auth_context::with_context(admin_ctx(), svc.revoke_lease(Uuid::new_v4())).await;
    assert!(matches!(result, Err(AppError::NotFound(_))));
}

#[tokio::test]
async fn status_returns_pool_info() {
    let svc = build_service();
    let resp = auth_context::with_context(admin_ctx(), svc.status())
        .await
        .unwrap();
    // Pool from 192.168.1.100 to 192.168.1.200 inclusive = 101 addresses.
    assert_eq!(resp.pool_total, 101);
    assert_eq!(resp.active_lease_count, 0);
    assert_eq!(resp.pool_used, 0);
    assert!(!resp.enabled);
}

#[tokio::test]
async fn status_includes_reservations_in_pool_used() {
    let (svc, dhcp, _cfg) = build_service_with_deps();

    // Seed a reservation within the pool range.
    seed_reservation(&dhcp, "aa:bb:cc:dd:ee:01", "192.168.1.150");

    // Seed an active lease.
    seed_active_lease(&dhcp, "aa:bb:cc:dd:ee:02", "192.168.1.100");

    let resp = auth_context::with_context(admin_ctx(), svc.status())
        .await
        .unwrap();
    assert_eq!(resp.active_lease_count, 1);
    // pool_used = 1 active lease + 1 reservation in pool range = 2.
    assert_eq!(resp.pool_used, 2);
}

#[tokio::test]
async fn status_excludes_reservations_outside_pool() {
    let (svc, dhcp, _cfg) = build_service_with_deps();

    // Seed a reservation OUTSIDE the pool range (pool is 192.168.1.100-200).
    seed_reservation(&dhcp, "aa:bb:cc:dd:ee:01", "192.168.1.50");

    let resp = auth_context::with_context(admin_ctx(), svc.status())
        .await
        .unwrap();
    // Reservation is outside pool, so pool_used should be 0.
    assert_eq!(resp.pool_used, 0);
}

// -- Shared helpers for runtime tests --------------------------------------

/// Build a `DhcpServiceImpl` and return the inner mock repositories so tests
/// can seed data before calling service methods.
fn build_service_with_deps() -> (
    DhcpServiceImpl,
    Arc<MockDhcpRepository>,
    Arc<MockSystemConfigRepository>,
) {
    let dhcp = Arc::new(MockDhcpRepository::new());
    let system_config = Arc::new(MockSystemConfigRepository::new());
    let svc = DhcpServiceImpl::new(dhcp.clone(), system_config.clone());
    (svc, dhcp, system_config)
}

/// Insert a pre-existing active lease into the mock repository.
///
/// Uses lowercase MAC addresses to match normalized lookups.
fn seed_active_lease(dhcp: &MockDhcpRepository, mac: &str, ip: &str) -> String {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now();
    let lease_end = now + chrono::Duration::hours(24);
    dhcp.leases.lock().unwrap().push(DhcpLeaseRow {
        id: id.clone(),
        mac_address: mac.to_owned(),
        ip_address: ip.to_owned(),
        hostname: None,
        lease_start: now.to_rfc3339(),
        lease_end: lease_end.to_rfc3339(),
        status: "active".to_owned(),
        device_id: None,
    });
    id
}

/// Insert a pre-existing expired lease into the mock repository.
fn seed_expired_lease(dhcp: &MockDhcpRepository, mac: &str, ip: &str) -> String {
    let id = Uuid::new_v4().to_string();
    let past = Utc::now() - chrono::Duration::hours(48);
    let lease_end = Utc::now() - chrono::Duration::hours(24);
    dhcp.leases.lock().unwrap().push(DhcpLeaseRow {
        id: id.clone(),
        mac_address: mac.to_owned(),
        ip_address: ip.to_owned(),
        hostname: None,
        lease_start: past.to_rfc3339(),
        lease_end: lease_end.to_rfc3339(),
        status: "active".to_owned(),
        device_id: None,
    });
    id
}

/// Insert a static reservation into the mock repository.
fn seed_reservation(dhcp: &MockDhcpRepository, mac: &str, ip: &str) {
    dhcp.reservations.lock().unwrap().push(DhcpReservationRow {
        id: Uuid::new_v4().to_string(),
        mac_address: mac.to_owned(),
        ip_address: ip.to_owned(),
        hostname: None,
        description: None,
    });
}

// =========================================================================
// assign_lease tests
// =========================================================================

#[tokio::test]
async fn assign_lease_allocates_from_pool() {
    let (svc, _dhcp, _cfg) = build_service_with_deps();

    let lease = auth_context::with_context(
        admin_ctx(),
        svc.assign_lease("AA:BB:CC:DD:EE:01", Some("host1")),
    )
    .await
    .unwrap();

    // MAC is normalized to lowercase.
    assert_eq!(lease.mac_address, "aa:bb:cc:dd:ee:01");
    assert_eq!(lease.hostname.as_deref(), Some("host1"));
    assert_eq!(lease.status, DhcpLeaseStatus::Active);
    // Should be the first IP in the default pool.
    assert_eq!(lease.ip_address, Ipv4Addr::new(192, 168, 1, 100));
}

#[tokio::test]
async fn assign_lease_allocates_from_pool_no_hostname() {
    let (svc, _dhcp, _cfg) = build_service_with_deps();

    let lease =
        auth_context::with_context(admin_ctx(), svc.assign_lease("AA:BB:CC:DD:EE:02", None))
            .await
            .unwrap();

    assert!(lease.hostname.is_none());
    assert_eq!(lease.ip_address, Ipv4Addr::new(192, 168, 1, 100));
}

#[tokio::test]
async fn assign_lease_reuses_existing_active_lease() {
    let (svc, dhcp, _cfg) = build_service_with_deps();

    // Seed an existing active lease for this MAC (lowercase to match normalization).
    let existing_id = seed_active_lease(&dhcp, "aa:bb:cc:dd:ee:03", "192.168.1.150");

    let lease = auth_context::with_context(
        admin_ctx(),
        svc.assign_lease("AA:BB:CC:DD:EE:03", Some("other-host")),
    )
    .await
    .unwrap();

    // Should return the existing lease, not create a new one.
    assert_eq!(lease.id.to_string(), existing_id);
    assert_eq!(lease.ip_address, Ipv4Addr::new(192, 168, 1, 150));
    // The hostname should be from the existing lease (None), not the new request.
    assert!(lease.hostname.is_none());
}

#[tokio::test]
async fn assign_lease_uses_static_reservation() {
    let (svc, dhcp, _cfg) = build_service_with_deps();

    // Seed a reservation for a specific MAC (lowercase to match normalization).
    seed_reservation(&dhcp, "aa:bb:cc:dd:ee:04", "192.168.1.42");

    let lease =
        auth_context::with_context(admin_ctx(), svc.assign_lease("AA:BB:CC:DD:EE:04", None))
            .await
            .unwrap();

    // Should get the reserved IP, not the first pool IP.
    assert_eq!(lease.ip_address, Ipv4Addr::new(192, 168, 1, 42));
    assert_eq!(lease.mac_address, "aa:bb:cc:dd:ee:04");
}

#[tokio::test]
async fn assign_lease_skips_occupied_ips() {
    let (svc, dhcp, _cfg) = build_service_with_deps();

    // Occupy the first IP in the pool.
    seed_active_lease(&dhcp, "aa:bb:cc:dd:ee:05", "192.168.1.100");

    let lease =
        auth_context::with_context(admin_ctx(), svc.assign_lease("AA:BB:CC:DD:EE:06", None))
            .await
            .unwrap();

    // Should skip .100 (occupied) and assign .101.
    assert_eq!(lease.ip_address, Ipv4Addr::new(192, 168, 1, 101));
}

#[tokio::test]
async fn assign_lease_skips_reserved_ips() {
    let (svc, dhcp, _cfg) = build_service_with_deps();

    // Reserve the first pool IP for a different MAC.
    seed_reservation(&dhcp, "aa:bb:cc:dd:ee:07", "192.168.1.100");

    // A different MAC requests a lease -- should skip .100 (reserved).
    let lease =
        auth_context::with_context(admin_ctx(), svc.assign_lease("AA:BB:CC:DD:EE:08", None))
            .await
            .unwrap();

    assert_eq!(lease.ip_address, Ipv4Addr::new(192, 168, 1, 101));
}

#[tokio::test]
async fn assign_lease_pool_exhausted() {
    let (svc, dhcp, cfg) = build_service_with_deps();

    // Shrink the pool to a single IP.
    cfg.set("dhcp_pool_start", "192.168.1.100").await.unwrap();
    cfg.set("dhcp_pool_end", "192.168.1.100").await.unwrap();

    // Occupy that single IP.
    seed_active_lease(&dhcp, "aa:bb:cc:dd:ee:09", "192.168.1.100");

    let result =
        auth_context::with_context(admin_ctx(), svc.assign_lease("AA:BB:CC:DD:EE:10", None)).await;

    assert!(matches!(result, Err(AppError::Conflict(_))));
}

#[tokio::test]
async fn assign_lease_creates_audit_log() {
    let (svc, dhcp, _cfg) = build_service_with_deps();

    let lease = auth_context::with_context(
        admin_ctx(),
        svc.assign_lease("AA:BB:CC:DD:EE:11", Some("myhost")),
    )
    .await
    .unwrap();

    let logs = dhcp.logs.lock().unwrap();
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].lease_id, lease.id.to_string());
    assert_eq!(logs[0].event_type, "assigned");
    assert_eq!(logs[0].details.as_deref(), Some("hostname: myhost"));
}

#[tokio::test]
async fn assign_lease_audit_log_no_hostname() {
    let (svc, dhcp, _cfg) = build_service_with_deps();

    auth_context::with_context(admin_ctx(), svc.assign_lease("AA:BB:CC:DD:EE:12", None))
        .await
        .unwrap();

    let logs = dhcp.logs.lock().unwrap();
    assert_eq!(logs.len(), 1);
    assert!(logs[0].details.is_none());
}

// =========================================================================
// renew_lease tests
// =========================================================================

#[tokio::test]
async fn renew_lease_extends_existing() {
    let (svc, dhcp, _cfg) = build_service_with_deps();

    let existing_id = seed_active_lease(&dhcp, "aa:bb:cc:dd:ee:20", "192.168.1.105");

    let lease = auth_context::with_context(admin_ctx(), svc.renew_lease("AA:BB:CC:DD:EE:20"))
        .await
        .unwrap();

    // Should return the same lease ID.
    assert_eq!(lease.id.to_string(), existing_id);
    assert_eq!(lease.mac_address, "aa:bb:cc:dd:ee:20");
    assert_eq!(lease.ip_address, Ipv4Addr::new(192, 168, 1, 105));

    // Verify a "renewed" log was created.
    let logs = dhcp.logs.lock().unwrap();
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].event_type, "renewed");
    assert!(logs[0].details.as_ref().unwrap().contains("new expiry"));
}

#[tokio::test]
async fn renew_lease_assigns_new_when_no_active_lease() {
    let (svc, dhcp, _cfg) = build_service_with_deps();

    // No existing lease for this MAC.
    let lease = auth_context::with_context(admin_ctx(), svc.renew_lease("AA:BB:CC:DD:EE:21"))
        .await
        .unwrap();

    // Should create a brand new lease via assign_lease.
    assert_eq!(lease.mac_address, "aa:bb:cc:dd:ee:21");
    assert_eq!(lease.ip_address, Ipv4Addr::new(192, 168, 1, 100));
    assert_eq!(lease.status, DhcpLeaseStatus::Active);

    // Verify an "assigned" log was created (not "renewed").
    let logs = dhcp.logs.lock().unwrap();
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].event_type, "assigned");
}

#[tokio::test]
async fn renew_lease_extends_expiry_into_the_future() {
    let (svc, dhcp, _cfg) = build_service_with_deps();

    // Seed a lease whose end is close to now.
    let id = Uuid::new_v4().to_string();
    let now = Utc::now();
    let almost_expired = now + chrono::Duration::minutes(5);
    dhcp.leases.lock().unwrap().push(DhcpLeaseRow {
        id: id.clone(),
        mac_address: "aa:bb:cc:dd:ee:22".to_owned(),
        ip_address: "192.168.1.110".to_owned(),
        hostname: None,
        lease_start: (now - chrono::Duration::hours(23)).to_rfc3339(),
        lease_end: almost_expired.to_rfc3339(),
        status: "active".to_owned(),
        device_id: None,
    });

    let lease = auth_context::with_context(admin_ctx(), svc.renew_lease("AA:BB:CC:DD:EE:22"))
        .await
        .unwrap();

    // The renewed lease_end should be roughly 24 hours from now (default duration).
    let expected_end = Utc::now() + chrono::Duration::seconds(86400);
    let diff = (lease.lease_end - expected_end).num_seconds().abs();
    assert!(
        diff < 5,
        "renewed lease_end should be ~24h from now, diff={diff}s"
    );
}

// =========================================================================
// release_lease tests
// =========================================================================

#[tokio::test]
async fn release_lease_marks_active_as_released() {
    let (svc, dhcp, _cfg) = build_service_with_deps();

    let lease_id = seed_active_lease(&dhcp, "aa:bb:cc:dd:ee:30", "192.168.1.120");

    auth_context::with_context(admin_ctx(), svc.release_lease("AA:BB:CC:DD:EE:30"))
        .await
        .unwrap();

    // Verify the status was changed.
    let rows = dhcp.leases.lock().unwrap();
    let row = rows.iter().find(|r| r.id == lease_id).unwrap();
    assert_eq!(row.status, "released");

    // Verify a "released" log was created.
    let logs = dhcp.logs.lock().unwrap();
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].event_type, "released");
    assert_eq!(logs[0].details.as_deref(), Some("client DHCPRELEASE"));
}

#[tokio::test]
async fn release_lease_noop_when_no_active_lease() {
    let (svc, dhcp, _cfg) = build_service_with_deps();

    // No active lease exists for this MAC -- should succeed silently.
    auth_context::with_context(admin_ctx(), svc.release_lease("AA:BB:CC:DD:EE:31"))
        .await
        .unwrap();

    // No logs should be created.
    let logs = dhcp.logs.lock().unwrap();
    assert!(logs.is_empty());
}

#[tokio::test]
async fn release_lease_frees_ip_for_reassignment() {
    let (svc, _dhcp, system_config) = build_service_with_deps();

    // Single-IP pool.
    system_config
        .set("dhcp_pool_start", "192.168.1.100")
        .await
        .unwrap();
    system_config
        .set("dhcp_pool_end", "192.168.1.100")
        .await
        .unwrap();

    // Assign the only IP.
    let lease =
        auth_context::with_context(admin_ctx(), svc.assign_lease("AA:BB:CC:DD:EE:32", None))
            .await
            .unwrap();
    assert_eq!(lease.ip_address, Ipv4Addr::new(192, 168, 1, 100));

    // Pool should now be exhausted.
    let result =
        auth_context::with_context(admin_ctx(), svc.assign_lease("AA:BB:CC:DD:EE:33", None)).await;
    assert!(matches!(result, Err(AppError::Conflict(_))));

    // Release the first lease.
    auth_context::with_context(admin_ctx(), svc.release_lease("AA:BB:CC:DD:EE:32"))
        .await
        .unwrap();

    // Now a new MAC can get the released IP.
    let new_lease =
        auth_context::with_context(admin_ctx(), svc.assign_lease("AA:BB:CC:DD:EE:33", None))
            .await
            .unwrap();
    assert_eq!(new_lease.ip_address, Ipv4Addr::new(192, 168, 1, 100));
}

// =========================================================================
// cleanup_expired tests
// =========================================================================

#[tokio::test]
async fn cleanup_expired_returns_count() {
    let (svc, dhcp, _cfg) = build_service_with_deps();

    // Seed two expired leases and one active.
    seed_expired_lease(&dhcp, "aa:bb:cc:dd:ee:40", "192.168.1.130");
    seed_expired_lease(&dhcp, "aa:bb:cc:dd:ee:41", "192.168.1.131");
    seed_active_lease(&dhcp, "aa:bb:cc:dd:ee:42", "192.168.1.132");

    let count = auth_context::with_context(admin_ctx(), svc.cleanup_expired())
        .await
        .unwrap();

    assert_eq!(count, 2);
}

#[tokio::test]
async fn cleanup_expired_zero_when_no_stale() {
    let (svc, dhcp, _cfg) = build_service_with_deps();

    // Only active (non-expired) leases.
    seed_active_lease(&dhcp, "aa:bb:cc:dd:ee:43", "192.168.1.140");

    let count = auth_context::with_context(admin_ctx(), svc.cleanup_expired())
        .await
        .unwrap();

    assert_eq!(count, 0);
}

#[tokio::test]
async fn cleanup_expired_zero_when_empty() {
    let svc = build_service();

    let count = auth_context::with_context(admin_ctx(), svc.cleanup_expired())
        .await
        .unwrap();

    assert_eq!(count, 0);
}

#[tokio::test]
async fn cleanup_expired_marks_status_as_expired() {
    let (svc, dhcp, _cfg) = build_service_with_deps();

    let lease_id = seed_expired_lease(&dhcp, "aa:bb:cc:dd:ee:44", "192.168.1.141");

    auth_context::with_context(admin_ctx(), svc.cleanup_expired())
        .await
        .unwrap();

    let rows = dhcp.leases.lock().unwrap();
    let row = rows.iter().find(|r| r.id == lease_id).unwrap();
    assert_eq!(row.status, "expired");
}

// =========================================================================
// get_dhcp_config tests
// =========================================================================

#[tokio::test]
async fn get_dhcp_config_returns_defaults() {
    let svc = build_service();

    let config = auth_context::with_context(admin_ctx(), svc.get_dhcp_config())
        .await
        .unwrap();

    assert!(!config.enabled);
    assert_eq!(config.pool_start, Ipv4Addr::new(192, 168, 1, 100));
    assert_eq!(config.pool_end, Ipv4Addr::new(192, 168, 1, 200));
    assert_eq!(config.subnet_mask, Ipv4Addr::new(255, 255, 255, 0));
    assert_eq!(config.lease_duration_secs, 86400);
    assert_eq!(config.upstream_dns.len(), 2);
    assert!(config.router_ip.is_none());
}

#[tokio::test]
async fn get_dhcp_config_anonymous_forbidden() {
    let svc = build_service();

    // Calling without admin auth context should fail.
    let result = auth_context::with_context(AuthContext::Anonymous, svc.get_dhcp_config()).await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}

#[tokio::test]
async fn get_dhcp_config_reflects_updated_values() {
    let (svc, _dhcp, system_config) = build_service_with_deps();

    system_config.set("dhcp_enabled", "true").await.unwrap();
    system_config
        .set("dhcp_pool_start", "10.0.0.1")
        .await
        .unwrap();
    system_config
        .set("dhcp_pool_end", "10.0.0.50")
        .await
        .unwrap();
    system_config
        .set("dhcp_subnet_mask", "255.255.0.0")
        .await
        .unwrap();
    system_config
        .set("dhcp_lease_duration_secs", "3600")
        .await
        .unwrap();
    system_config
        .set("dhcp_router_ip", "10.0.0.254")
        .await
        .unwrap();
    system_config
        .set("dhcp_upstream_dns", r#"["9.9.9.9"]"#)
        .await
        .unwrap();

    let config = auth_context::with_context(admin_ctx(), svc.get_dhcp_config())
        .await
        .unwrap();

    assert!(config.enabled);
    assert_eq!(config.pool_start, Ipv4Addr::new(10, 0, 0, 1));
    assert_eq!(config.pool_end, Ipv4Addr::new(10, 0, 0, 50));
    assert_eq!(config.subnet_mask, Ipv4Addr::new(255, 255, 0, 0));
    assert_eq!(config.lease_duration_secs, 3600);
    assert_eq!(config.router_ip, Some(Ipv4Addr::new(10, 0, 0, 254)));
    assert_eq!(config.upstream_dns, vec![Ipv4Addr::new(9, 9, 9, 9)]);
}

// =========================================================================
// load_config edge-case tests (via get_dhcp_config)
// =========================================================================

#[tokio::test]
async fn load_config_missing_keys_uses_defaults() {
    // A completely empty config store should fall back to defaults.
    let dhcp = Arc::new(MockDhcpRepository::new());
    let system_config = Arc::new(MockSystemConfigRepository {
        data: Mutex::new(HashMap::new()),
    });
    let svc = DhcpServiceImpl::new(dhcp, system_config);

    let config = auth_context::with_context(admin_ctx(), svc.get_dhcp_config())
        .await
        .unwrap();

    assert!(!config.enabled);
    assert_eq!(config.pool_start, Ipv4Addr::new(192, 168, 1, 100));
    assert_eq!(config.pool_end, Ipv4Addr::new(192, 168, 1, 200));
    assert_eq!(config.subnet_mask, Ipv4Addr::new(255, 255, 255, 0));
    assert_eq!(config.lease_duration_secs, 86400);
    assert!(config.router_ip.is_none());
}

#[tokio::test]
async fn load_config_invalid_pool_start_returns_error() {
    let (svc, _dhcp, system_config) = build_service_with_deps();
    system_config
        .set("dhcp_pool_start", "not-an-ip")
        .await
        .unwrap();

    let result = auth_context::with_context(admin_ctx(), svc.get_dhcp_config()).await;

    assert!(matches!(result, Err(AppError::Internal(_))));
}

#[tokio::test]
async fn load_config_invalid_pool_end_returns_error() {
    let (svc, _dhcp, system_config) = build_service_with_deps();
    system_config.set("dhcp_pool_end", "garbage").await.unwrap();

    let result = auth_context::with_context(admin_ctx(), svc.get_dhcp_config()).await;

    assert!(matches!(result, Err(AppError::Internal(_))));
}

#[tokio::test]
async fn load_config_invalid_subnet_mask_returns_error() {
    let (svc, _dhcp, system_config) = build_service_with_deps();
    system_config.set("dhcp_subnet_mask", "abc").await.unwrap();

    let result = auth_context::with_context(admin_ctx(), svc.get_dhcp_config()).await;

    assert!(matches!(result, Err(AppError::Internal(_))));
}

#[tokio::test]
async fn load_config_invalid_lease_duration_returns_error() {
    let (svc, _dhcp, system_config) = build_service_with_deps();
    system_config
        .set("dhcp_lease_duration_secs", "not_a_number")
        .await
        .unwrap();

    let result = auth_context::with_context(admin_ctx(), svc.get_dhcp_config()).await;

    assert!(matches!(result, Err(AppError::Internal(_))));
}

#[tokio::test]
async fn load_config_invalid_upstream_dns_json_returns_error() {
    let (svc, _dhcp, system_config) = build_service_with_deps();
    system_config
        .set("dhcp_upstream_dns", "not json")
        .await
        .unwrap();

    let result = auth_context::with_context(admin_ctx(), svc.get_dhcp_config()).await;

    assert!(matches!(result, Err(AppError::Internal(_))));
}

#[tokio::test]
async fn load_config_invalid_upstream_dns_ip_returns_error() {
    let (svc, _dhcp, system_config) = build_service_with_deps();
    system_config
        .set("dhcp_upstream_dns", r#"["not.an.ip.addr"]"#)
        .await
        .unwrap();

    let result = auth_context::with_context(admin_ctx(), svc.get_dhcp_config()).await;

    assert!(matches!(result, Err(AppError::Internal(_))));
}

#[tokio::test]
async fn load_config_invalid_router_ip_returns_error() {
    let (svc, _dhcp, system_config) = build_service_with_deps();
    system_config.set("dhcp_router_ip", "bad-ip").await.unwrap();

    let result = auth_context::with_context(admin_ctx(), svc.get_dhcp_config()).await;

    assert!(matches!(result, Err(AppError::Internal(_))));
}

#[tokio::test]
async fn load_config_empty_router_ip_is_none() {
    let (svc, _dhcp, system_config) = build_service_with_deps();
    system_config.set("dhcp_router_ip", "").await.unwrap();

    let config = auth_context::with_context(admin_ctx(), svc.get_dhcp_config())
        .await
        .unwrap();

    assert!(config.router_ip.is_none());
}

// =========================================================================
// Integration-style scenarios
// =========================================================================

#[tokio::test]
async fn full_lease_lifecycle_assign_renew_release() {
    let (svc, dhcp, _cfg) = build_service_with_deps();

    // 1. Assign a new lease.
    let lease = auth_context::with_context(
        admin_ctx(),
        svc.assign_lease("AA:BB:CC:DD:EE:50", Some("laptop")),
    )
    .await
    .unwrap();
    assert_eq!(lease.status, DhcpLeaseStatus::Active);
    let original_end = lease.lease_end;

    // 2. Renew the lease -- should extend expiry.
    let renewed = auth_context::with_context(admin_ctx(), svc.renew_lease("AA:BB:CC:DD:EE:50"))
        .await
        .unwrap();
    assert_eq!(renewed.id, lease.id);
    assert!(renewed.lease_end >= original_end);

    // 3. Release the lease.
    auth_context::with_context(admin_ctx(), svc.release_lease("AA:BB:CC:DD:EE:50"))
        .await
        .unwrap();

    // 4. Verify it is no longer active.
    let rows = dhcp.leases.lock().unwrap();
    let row = rows.iter().find(|r| r.id == lease.id.to_string()).unwrap();
    assert_eq!(row.status, "released");

    // 5. Check audit log has all three events: assigned, renewed, released.
    let logs = dhcp.logs.lock().unwrap();
    let events: Vec<&str> = logs.iter().map(|l| l.event_type.as_str()).collect();
    assert_eq!(events, vec!["assigned", "renewed", "released"]);
}

#[tokio::test]
async fn multiple_macs_get_distinct_ips() {
    let (svc, _dhcp, _cfg) = build_service_with_deps();

    let lease1 =
        auth_context::with_context(admin_ctx(), svc.assign_lease("AA:BB:CC:DD:EE:60", None))
            .await
            .unwrap();
    let lease2 =
        auth_context::with_context(admin_ctx(), svc.assign_lease("AA:BB:CC:DD:EE:61", None))
            .await
            .unwrap();
    let lease3 =
        auth_context::with_context(admin_ctx(), svc.assign_lease("AA:BB:CC:DD:EE:62", None))
            .await
            .unwrap();

    // All three should have different IPs.
    assert_ne!(lease1.ip_address, lease2.ip_address);
    assert_ne!(lease2.ip_address, lease3.ip_address);
    assert_ne!(lease1.ip_address, lease3.ip_address);

    // Should be sequential from pool start.
    assert_eq!(lease1.ip_address, Ipv4Addr::new(192, 168, 1, 100));
    assert_eq!(lease2.ip_address, Ipv4Addr::new(192, 168, 1, 101));
    assert_eq!(lease3.ip_address, Ipv4Addr::new(192, 168, 1, 102));
}

// =========================================================================
// create_reservation: invalid IP address
// =========================================================================

#[tokio::test]
async fn create_reservation_invalid_ip() {
    let svc = build_service();
    let req = CreateDhcpReservationRequest {
        mac_address: "AA:BB:CC:DD:EE:01".to_owned(),
        ip_address: "not-an-ip".to_owned(),
        hostname: None,
        description: None,
    };
    let result = auth_context::with_context(admin_ctx(), svc.create_reservation(req)).await;
    assert!(matches!(result, Err(AppError::BadRequest(_))));
}

// =========================================================================
// revoke_lease: lease exists but is not active
// =========================================================================

#[tokio::test]
async fn revoke_lease_not_active() {
    let (svc, dhcp, _cfg) = build_service_with_deps();

    // Seed a lease, then mark it as released.
    let lease_id = seed_active_lease(&dhcp, "aa:bb:cc:dd:ee:70", "192.168.1.170");
    dhcp.leases
        .lock()
        .unwrap()
        .iter_mut()
        .find(|r| r.id == lease_id)
        .unwrap()
        .status = "released".to_owned();

    let id: Uuid = lease_id.parse().unwrap();
    let result = auth_context::with_context(admin_ctx(), svc.revoke_lease(id)).await;
    assert!(
        matches!(result, Err(AppError::BadRequest(_))),
        "revoking a non-active lease should return BadRequest"
    );
}

// =========================================================================
// revoke_lease: success path
// =========================================================================

#[tokio::test]
async fn revoke_lease_success() {
    let (svc, dhcp, _cfg) = build_service_with_deps();

    let lease_id = seed_active_lease(&dhcp, "aa:bb:cc:dd:ee:71", "192.168.1.171");
    let id: Uuid = lease_id.parse().unwrap();

    let resp = auth_context::with_context(admin_ctx(), svc.revoke_lease(id))
        .await
        .unwrap();

    assert!(resp.message.contains(&id.to_string()));

    // Verify the status was changed.
    let rows = dhcp.leases.lock().unwrap();
    let row = rows.iter().find(|r| r.id == lease_id).unwrap();
    assert_eq!(row.status, "released");

    // Verify an audit log was created.
    let logs = dhcp.logs.lock().unwrap();
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].event_type, "released");
    assert_eq!(logs[0].details.as_deref(), Some("admin revoked"));
}

// =========================================================================
// delete_reservation: success path
// =========================================================================

#[tokio::test]
async fn delete_reservation_success() {
    let (svc, dhcp, _cfg) = build_service_with_deps();

    // Create a reservation via the service.
    let req = CreateDhcpReservationRequest {
        mac_address: "AA:BB:CC:DD:EE:80".to_owned(),
        ip_address: "192.168.1.80".to_owned(),
        hostname: Some("res-host".to_owned()),
        description: None,
    };
    let resp = auth_context::with_context(admin_ctx(), svc.create_reservation(req))
        .await
        .unwrap();

    // Delete it.
    let del_resp =
        auth_context::with_context(admin_ctx(), svc.delete_reservation(resp.reservation.id))
            .await
            .unwrap();

    assert!(del_resp.message.contains(&resp.reservation.id.to_string()));

    // Verify it's gone.
    let reservations = dhcp.reservations.lock().unwrap();
    assert!(reservations.is_empty());
}

// =========================================================================
// update_config: additional validation edge cases
// =========================================================================

#[tokio::test]
async fn update_config_invalid_pool_end() {
    let svc = build_service();
    let req = UpdateDhcpConfigRequest {
        pool_start: "192.168.1.100".to_owned(),
        pool_end: "not-valid".to_owned(),
        subnet_mask: "255.255.255.0".to_owned(),
        upstream_dns: vec!["1.1.1.1".to_owned()],
        lease_duration_secs: 86400,
        router_ip: None,
    };
    let result = auth_context::with_context(admin_ctx(), svc.update_config(req)).await;
    assert!(matches!(result, Err(AppError::BadRequest(_))));
}

#[tokio::test]
async fn update_config_invalid_subnet_mask() {
    let svc = build_service();
    let req = UpdateDhcpConfigRequest {
        pool_start: "192.168.1.100".to_owned(),
        pool_end: "192.168.1.200".to_owned(),
        subnet_mask: "bad".to_owned(),
        upstream_dns: vec!["1.1.1.1".to_owned()],
        lease_duration_secs: 86400,
        router_ip: None,
    };
    let result = auth_context::with_context(admin_ctx(), svc.update_config(req)).await;
    assert!(matches!(result, Err(AppError::BadRequest(_))));
}

#[tokio::test]
async fn update_config_invalid_upstream_dns() {
    let svc = build_service();
    let req = UpdateDhcpConfigRequest {
        pool_start: "192.168.1.100".to_owned(),
        pool_end: "192.168.1.200".to_owned(),
        subnet_mask: "255.255.255.0".to_owned(),
        upstream_dns: vec!["not-an-ip".to_owned()],
        lease_duration_secs: 86400,
        router_ip: None,
    };
    let result = auth_context::with_context(admin_ctx(), svc.update_config(req)).await;
    assert!(matches!(result, Err(AppError::BadRequest(_))));
}

#[tokio::test]
async fn update_config_invalid_router_ip() {
    let svc = build_service();
    let req = UpdateDhcpConfigRequest {
        pool_start: "192.168.1.100".to_owned(),
        pool_end: "192.168.1.200".to_owned(),
        subnet_mask: "255.255.255.0".to_owned(),
        upstream_dns: vec!["1.1.1.1".to_owned()],
        lease_duration_secs: 86400,
        router_ip: Some("bad-ip".to_owned()),
    };
    let result = auth_context::with_context(admin_ctx(), svc.update_config(req)).await;
    assert!(matches!(result, Err(AppError::BadRequest(_))));
}

#[tokio::test]
async fn update_config_with_valid_router_ip() {
    let svc = build_service();
    let req = UpdateDhcpConfigRequest {
        pool_start: "192.168.1.100".to_owned(),
        pool_end: "192.168.1.200".to_owned(),
        subnet_mask: "255.255.255.0".to_owned(),
        upstream_dns: vec!["1.1.1.1".to_owned()],
        lease_duration_secs: 86400,
        router_ip: Some("192.168.1.1".to_owned()),
    };
    let resp = auth_context::with_context(admin_ctx(), svc.update_config(req))
        .await
        .unwrap();
    assert_eq!(resp.config.router_ip, Some(Ipv4Addr::new(192, 168, 1, 1)));
}

// =========================================================================
// pool_size edge cases
// =========================================================================

#[tokio::test]
async fn pool_size_zero_when_end_before_start() {
    // pool_size is private, but we can test it indirectly via status() with
    // a config where pool_end < pool_start.
    let (svc, _dhcp, system_config) = build_service_with_deps();
    system_config
        .set("dhcp_pool_start", "192.168.1.200")
        .await
        .unwrap();
    system_config
        .set("dhcp_pool_end", "192.168.1.100")
        .await
        .unwrap();

    let resp = auth_context::with_context(admin_ctx(), svc.status())
        .await
        .unwrap();
    assert_eq!(resp.pool_total, 0);
}
