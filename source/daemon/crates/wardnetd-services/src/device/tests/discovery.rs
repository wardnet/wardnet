use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::Utc;
use std::net::Ipv4Addr;
use tokio::sync::broadcast;
use uuid::Uuid;
use wardnet_common::device::{Device, DeviceConnectionMode, DeviceType};
use wardnet_common::dhcp::{DhcpLease, DhcpLeaseLog, DhcpLeaseStatus, DhcpReservation};
use wardnet_common::event::WardnetEvent;
use wardnet_common::network_zone::{
    AllowedTargetKind, NetworkZone, ZoneProvenance, ZoneStance, ZoneSubnet,
};
use wardnet_common::routing::RoutingRule;

use wardnet_common::auth::AuthContext;

use crate::DeviceDiscoveryService;
use crate::auth_context;
use crate::device::discovery::{DeviceDiscoveryServiceImpl, ObservationResult};
use crate::device::hostname_resolver::HostnameResolver;
use crate::device::packet_capture::{ObservedDevice, PacketSource};
use crate::error::AppError;
use crate::event::EventPublisher;
use wardnetd_data::repository::device::DeviceRow;
use wardnetd_data::repository::{
    DeviceRepository, DhcpLeaseLogRow, DhcpLeaseRow, DhcpRepository, DhcpReservationRow,
    NetworkZoneRepository, SystemConfigRepository,
};

/// Helper to create an admin auth context for tests.
fn admin_ctx() -> AuthContext {
    AuthContext::Admin {
        admin_id: Uuid::new_v4(),
    }
}

/// Helper to create a device auth context for tests.
fn device_ctx(mac: &str) -> AuthContext {
    AuthContext::Device {
        mac: mac.to_owned(),
    }
}

// -- Mock DeviceRepository ------------------------------------------------

struct MockDeviceRepo {
    devices: Mutex<Vec<Device>>,
    inserted: Mutex<Vec<DeviceRow>>,
    last_seen_updates: Mutex<Vec<(String, String, String, DeviceConnectionMode)>>,
    batch_updates: Mutex<Vec<(String, String)>>,
    hostname_updates: Mutex<Vec<(String, String)>>,
    name_type_updates: Mutex<Vec<(String, Option<String>, String)>>,
    /// When set, `clear_last_ip` returns an error, exercising the departure
    /// sweep's failure-tolerant path.
    fail_clear_last_ip: std::sync::atomic::AtomicBool,
    /// When set, `delete_unmanaged_before` returns an error, exercising the
    /// retention prune's failure path (#1181).
    fail_delete_unmanaged: std::sync::atomic::AtomicBool,
}

impl MockDeviceRepo {
    fn with_devices(devices: Vec<Device>) -> Self {
        // Mirror the SQLite repo: stored MAC is always canonical lowercase
        // (#312), so seed fixtures get the same treatment.
        let devices = devices
            .into_iter()
            .map(|mut d| {
                d.mac = d.mac.to_lowercase();
                d
            })
            .collect();
        Self {
            devices: Mutex::new(devices),
            inserted: Mutex::new(Vec::new()),
            last_seen_updates: Mutex::new(Vec::new()),
            batch_updates: Mutex::new(Vec::new()),
            hostname_updates: Mutex::new(Vec::new()),
            name_type_updates: Mutex::new(Vec::new()),
            fail_clear_last_ip: std::sync::atomic::AtomicBool::new(false),
            fail_delete_unmanaged: std::sync::atomic::AtomicBool::new(false),
        }
    }
}

#[async_trait]
impl DeviceRepository for MockDeviceRepo {
    async fn delete_rule_for_device(&self, _device_id: &str) -> anyhow::Result<()> {
        Ok(())
    }

    async fn set_managed(&self, id: &str, managed: bool) -> anyhow::Result<()> {
        let mut devices = self.devices.lock().unwrap();
        if let Some(d) = devices.iter_mut().find(|d| d.id.to_string() == id) {
            d.managed = managed;
        }
        Ok(())
    }

    /// Mirrors the real DELETE ... RETURNING: removes the matching rows from
    /// the backing store and returns what was removed, so the eviction tests
    /// exercise the same "row is gone" postcondition production has.
    async fn delete_unmanaged_before(
        &self,
        cutoff: &str,
    ) -> anyhow::Result<Vec<wardnetd_data::repository::PrunedDevice>> {
        if self
            .fail_delete_unmanaged
            .load(std::sync::atomic::Ordering::SeqCst)
        {
            anyhow::bail!("delete_unmanaged_before failed");
        }
        let cutoff: chrono::DateTime<chrono::Utc> = cutoff.parse()?;
        let mut devices = self.devices.lock().unwrap();
        let mut pruned = Vec::new();
        devices.retain(|d| {
            if !d.managed && d.last_seen < cutoff {
                pruned.push(wardnetd_data::repository::PrunedDevice {
                    id: d.id.to_string(),
                    mac: d.mac.clone(),
                    last_seen: d.last_seen.to_rfc3339(),
                });
                false
            } else {
                true
            }
        });
        Ok(pruned)
    }

    async fn find_by_ip(&self, ip: &str) -> anyhow::Result<Option<Device>> {
        let devices = self.devices.lock().unwrap();
        Ok(devices.iter().find(|d| d.last_ip == ip).cloned())
    }

    async fn find_by_id(&self, id: &str) -> anyhow::Result<Option<Device>> {
        let devices = self.devices.lock().unwrap();
        Ok(devices.iter().find(|d| d.id.to_string() == id).cloned())
    }

    async fn find_by_mac(&self, mac: &str) -> anyhow::Result<Option<Device>> {
        // Mirror the SQLite repo: MAC is stored canonical lowercase (#312).
        let mac = mac.to_lowercase();
        let devices = self.devices.lock().unwrap();
        Ok(devices.iter().find(|d| d.mac == mac).cloned())
    }

    async fn find_all(&self) -> anyhow::Result<Vec<Device>> {
        Ok(self.devices.lock().unwrap().clone())
    }

    async fn insert(&self, device: &DeviceRow) -> anyhow::Result<()> {
        // Mirror the SQLite repo: MAC stored canonical lowercase (#312).
        let mac = device.mac.to_lowercase();
        let mut inserted = self.inserted.lock().unwrap();
        inserted.push(DeviceRow {
            id: device.id.clone(),
            mac: mac.clone(),
            hostname: device.hostname.clone(),
            manufacturer: device.manufacturer.clone(),
            manufacturer_source: None,
            is_randomized: false,
            device_type: device.device_type.clone(),
            first_seen: device.first_seen.clone(),
            last_seen: device.last_seen.clone(),
            last_ip: device.last_ip.clone(),
            zone_id: device.zone_id.clone(),
            connection_mode: device.connection_mode,
        });

        // Also add to the devices list so subsequent find_by_id works.
        let mut devices = self.devices.lock().unwrap();
        devices.push(Device {
            id: device.id.parse().unwrap(),
            mac: mac.clone(),
            name: None,
            hostname: device.hostname.clone(),
            manufacturer: device.manufacturer.clone(),
            manufacturer_source: None,
            is_randomized: false,
            device_type: serde_json::from_str(&format!("\"{}\"", device.device_type))
                .unwrap_or(DeviceType::Unknown),
            first_seen: device.first_seen.parse().unwrap(),
            last_seen: device.last_seen.parse().unwrap(),
            last_ip: device.last_ip.clone(),
            admin_locked: false,
            zone_id: device.zone_id.parse().unwrap(),
            dns_capture_enabled: false,
            dns_capture_cap_count: 1000,
            dns_capture_cap_days: 7,
            connection_mode: device.connection_mode,
            // Mirrors the SQLite repo: `insert` omits `managed`, so a freshly
            // discovered device always starts unmanaged (#1181).
            managed: false,
        });
        Ok(())
    }

    async fn update_last_seen_and_ip(
        &self,
        id: &str,
        ip: &str,
        last_seen: &str,
        mode: DeviceConnectionMode,
    ) -> anyhow::Result<()> {
        self.last_seen_updates.lock().unwrap().push((
            id.to_owned(),
            ip.to_owned(),
            last_seen.to_owned(),
            mode,
        ));

        // Mirror the SQLite repo so an observation's IP write is reflected in
        // subsequent lookups (e.g. a reappearance restoring an IP the departure
        // sweep had cleared).
        let mut devices = self.devices.lock().unwrap();
        if let Some(d) = devices.iter_mut().find(|d| d.id.to_string() == id) {
            d.last_ip = ip.to_owned();
            d.connection_mode = mode;
            if let Ok(ts) = last_seen.parse() {
                d.last_seen = ts;
            }
        }
        Ok(())
    }

    async fn clear_last_ip(&self, id: &str) -> anyhow::Result<()> {
        if self
            .fail_clear_last_ip
            .load(std::sync::atomic::Ordering::SeqCst)
        {
            anyhow::bail!("simulated clear_last_ip failure");
        }
        let mut devices = self.devices.lock().unwrap();
        if let Some(d) = devices.iter_mut().find(|d| d.id.to_string() == id) {
            d.last_ip = String::new();
        }
        Ok(())
    }

    async fn update_connection_mode(
        &self,
        _id: &str,
        _mode: DeviceConnectionMode,
    ) -> anyhow::Result<()> {
        Ok(())
    }

    async fn update_last_seen_batch(&self, updates: &[(String, String)]) -> anyhow::Result<()> {
        self.batch_updates
            .lock()
            .unwrap()
            .extend(updates.iter().cloned());
        Ok(())
    }

    async fn update_hostname(&self, id: &str, hostname: &str) -> anyhow::Result<()> {
        self.hostname_updates
            .lock()
            .unwrap()
            .push((id.to_owned(), hostname.to_owned()));
        Ok(())
    }

    async fn update_name_and_type(
        &self,
        id: &str,
        name: Option<&str>,
        device_type: &str,
    ) -> anyhow::Result<()> {
        self.name_type_updates.lock().unwrap().push((
            id.to_owned(),
            name.map(String::from),
            device_type.to_owned(),
        ));

        // Update the in-memory device so subsequent find_by_id returns updated data.
        let mut devices = self.devices.lock().unwrap();
        if let Some(d) = devices.iter_mut().find(|d| d.id.to_string() == id) {
            // Assigned unconditionally, mirroring the real repo's
            // `SET name = ?`: a `None` here means "write NULL" (the caller has
            // already resolved partial-update semantics), not "leave it alone".
            // The mock used to skip `None`, which made clearing a name silently
            // untestable.
            d.name = name.map(str::to_owned);
            if let Ok(dt) = serde_json::from_str(&format!("\"{device_type}\"")) {
                d.device_type = dt;
            }
        }
        Ok(())
    }

    async fn find_stale(&self, _before: &str) -> anyhow::Result<Vec<Device>> {
        Ok(vec![])
    }

    async fn find_rule_for_device(&self, _id: &str) -> anyhow::Result<Option<RoutingRule>> {
        Ok(None)
    }
    async fn find_all_rules(&self) -> anyhow::Result<Vec<RoutingRule>> {
        Ok(vec![])
    }

    async fn upsert_user_rule(&self, _id: &str, _json: &str, _now: &str) -> anyhow::Result<()> {
        Ok(())
    }

    async fn update_admin_locked(&self, id: &str, locked: bool) -> anyhow::Result<()> {
        let mut devices = self.devices.lock().unwrap();
        if let Some(d) = devices.iter_mut().find(|d| d.id.to_string() == id) {
            d.admin_locked = locked;
        }
        Ok(())
    }

    async fn find_devices_for_tunnel(
        &self,
        _tid: &str,
    ) -> anyhow::Result<Vec<wardnet_common::device::Device>> {
        Ok(vec![])
    }

    async fn switch_tunnel_rules_to_direct(
        &self,
        _tid: &str,
        _now: &str,
    ) -> anyhow::Result<Vec<String>> {
        Ok(vec![])
    }

    async fn assign_zone(&self, _device_id: &str, _zone_id: &str) -> anyhow::Result<bool> {
        Ok(true)
    }
    async fn count(&self) -> anyhow::Result<i64> {
        Ok(i64::try_from(self.devices.lock().unwrap().len()).unwrap_or(0))
    }
    async fn update_dns_capture_settings(
        &self,
        _id: &str,
        _enabled: Option<bool>,
        _cap_count: Option<i64>,
        _cap_days: Option<i64>,
    ) -> anyhow::Result<bool> {
        Ok(true)
    }
    async fn find_all_capture_enabled_ids(&self) -> anyhow::Result<Vec<String>> {
        Ok(vec![])
    }
}

// -- Mock HostnameResolver ------------------------------------------------

struct MockHostnameResolver {
    mappings: HashMap<String, String>,
}

impl MockHostnameResolver {
    fn new() -> Self {
        Self {
            mappings: HashMap::new(),
        }
    }

    fn with_mappings(mappings: HashMap<String, String>) -> Self {
        Self { mappings }
    }
}

#[async_trait]
impl HostnameResolver for MockHostnameResolver {
    async fn resolve(&self, ip: &str) -> Option<String> {
        self.mappings.get(ip).cloned()
    }
}

// -- Mock EventPublisher --------------------------------------------------

struct MockEventPublisher {
    events: Mutex<Vec<WardnetEvent>>,
}

impl MockEventPublisher {
    fn new() -> Self {
        Self {
            events: Mutex::new(Vec::new()),
        }
    }

    fn published_events(&self) -> Vec<WardnetEvent> {
        self.events.lock().unwrap().clone()
    }
}

impl EventPublisher for MockEventPublisher {
    fn publish(&self, event: WardnetEvent) {
        self.events.lock().unwrap().push(event);
    }

    fn subscribe(&self) -> broadcast::Receiver<WardnetEvent> {
        let (_, rx) = broadcast::channel(16);
        rx
    }
}

// -- Mock DhcpRepository --------------------------------------------------

/// Minimal `DhcpRepository` mock that returns a configurable hostname per MAC
/// from the active-lease lookup. Other methods are unused by the discovery
/// service and panic if called.
/// Zone repo whose `find_default_for_new` returns the seeded Trusted zone, so
/// discovered devices are inserted with the Trusted zone id.
struct MockNetworkZoneRepo;

fn trusted_zone() -> NetworkZone {
    NetworkZone {
        id: Uuid::parse_str("00000000-0000-0000-0000-000000000201").unwrap(),
        name: "Trusted".to_owned(),
        provenance: ZoneProvenance::System,
        isolation_stance: ZoneStance::SharedSubnet,
        allowed_targets: vec![AllowedTargetKind::Direct, AllowedTargetKind::Tunnel],
        member_isolation: false,
        subnet: None,
        admin_ui_reachable: false,
        is_default: true,
        is_default_for_new: true,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }
}

#[async_trait]
impl NetworkZoneRepository for MockNetworkZoneRepo {
    async fn find_all(&self) -> anyhow::Result<Vec<NetworkZone>> {
        Ok(vec![trusted_zone()])
    }
    async fn find_by_id(&self, _id: &str) -> anyhow::Result<Option<NetworkZone>> {
        Ok(Some(trusted_zone()))
    }
    async fn find_default(&self) -> anyhow::Result<NetworkZone> {
        Ok(trusted_zone())
    }
    async fn find_default_for_new(&self) -> anyhow::Result<NetworkZone> {
        Ok(trusted_zone())
    }
    async fn insert(&self, _zone: &NetworkZone) -> anyhow::Result<()> {
        Ok(())
    }
    async fn update(&self, _zone: &NetworkZone) -> anyhow::Result<()> {
        Ok(())
    }
    async fn delete(&self, _id: &str) -> anyhow::Result<()> {
        Ok(())
    }
    async fn set_default(&self, _id: &str) -> anyhow::Result<()> {
        Ok(())
    }
    async fn set_default_for_new(&self, _id: &str) -> anyhow::Result<()> {
        Ok(())
    }
    async fn count_members(&self, _zone_id: &str) -> anyhow::Result<i64> {
        Ok(0)
    }
    async fn member_counts(&self) -> anyhow::Result<std::collections::HashMap<String, i64>> {
        Ok(std::collections::HashMap::new())
    }
}

/// A zone repo whose zone list can be swapped at runtime, so tests can exercise
/// the trusted-subnet cache picking up a newly-configured zone subnet.
struct MockZoneRepoWithSubnets {
    zones: Mutex<Vec<NetworkZone>>,
}

impl MockZoneRepoWithSubnets {
    fn new(zones: Vec<NetworkZone>) -> Self {
        Self {
            zones: Mutex::new(zones),
        }
    }

    fn set_zones(&self, zones: Vec<NetworkZone>) {
        *self.zones.lock().unwrap() = zones;
    }
}

/// Build a zone carrying a per-zone subnet CIDR (the DHCP-mode subnet devices
/// in the zone are addressed from).
fn zone_with_subnet(cidr: &str) -> NetworkZone {
    NetworkZone {
        subnet: Some(ZoneSubnet {
            cidr: cidr.to_owned(),
        }),
        ..trusted_zone()
    }
}

#[async_trait]
impl NetworkZoneRepository for MockZoneRepoWithSubnets {
    async fn find_all(&self) -> anyhow::Result<Vec<NetworkZone>> {
        Ok(self.zones.lock().unwrap().clone())
    }
    async fn find_by_id(&self, _id: &str) -> anyhow::Result<Option<NetworkZone>> {
        Ok(Some(trusted_zone()))
    }
    async fn find_default(&self) -> anyhow::Result<NetworkZone> {
        Ok(trusted_zone())
    }
    async fn find_default_for_new(&self) -> anyhow::Result<NetworkZone> {
        Ok(trusted_zone())
    }
    async fn insert(&self, _zone: &NetworkZone) -> anyhow::Result<()> {
        Ok(())
    }
    async fn update(&self, _zone: &NetworkZone) -> anyhow::Result<()> {
        Ok(())
    }
    async fn delete(&self, _id: &str) -> anyhow::Result<()> {
        Ok(())
    }
    async fn set_default(&self, _id: &str) -> anyhow::Result<()> {
        Ok(())
    }
    async fn set_default_for_new(&self, _id: &str) -> anyhow::Result<()> {
        Ok(())
    }
    async fn count_members(&self, _zone_id: &str) -> anyhow::Result<i64> {
        Ok(0)
    }
    async fn member_counts(&self) -> anyhow::Result<std::collections::HashMap<String, i64>> {
        Ok(std::collections::HashMap::new())
    }
}

struct MockDhcpRepository {
    leases_by_mac: HashMap<String, Option<String>>,
}

impl MockDhcpRepository {
    fn empty() -> Self {
        Self {
            leases_by_mac: HashMap::new(),
        }
    }

    fn with_lease_hostnames(entries: Vec<(&str, Option<&str>)>) -> Self {
        // Mirror the SQLite repo: keys are stored canonical lowercase
        // (#312) so that a lookup with mixed-case input still hits.
        let mut leases_by_mac = HashMap::new();
        for (mac, hostname) in entries {
            leases_by_mac.insert(mac.to_lowercase(), hostname.map(ToOwned::to_owned));
        }
        Self { leases_by_mac }
    }
}

#[async_trait]
impl DhcpRepository for MockDhcpRepository {
    async fn insert_lease(&self, _row: &DhcpLeaseRow) -> anyhow::Result<()> {
        unreachable!("discovery service does not write leases")
    }
    async fn find_lease_by_id(&self, _id: &str) -> anyhow::Result<Option<DhcpLease>> {
        unreachable!()
    }
    async fn find_active_lease_by_mac(&self, mac: &str) -> anyhow::Result<Option<DhcpLease>> {
        let mac = mac.to_lowercase();
        Ok(self.leases_by_mac.get(&mac).map(|hostname| DhcpLease {
            id: Uuid::nil(),
            mac_address: mac.clone(),
            ip_address: Ipv4Addr::new(192, 168, 1, 10),
            hostname: hostname.clone(),
            lease_start: Utc::now(),
            lease_end: Utc::now(),
            status: DhcpLeaseStatus::Active,
            device_id: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }))
    }
    async fn find_active_lease_by_ip(&self, _ip: &str) -> anyhow::Result<Option<DhcpLease>> {
        unreachable!()
    }
    async fn list_active_leases(&self) -> anyhow::Result<Vec<DhcpLease>> {
        unreachable!()
    }
    async fn update_lease_status(&self, _id: &str, _status: &str) -> anyhow::Result<()> {
        unreachable!()
    }
    async fn renew_lease(&self, _id: &str, _new_end: &str) -> anyhow::Result<()> {
        unreachable!()
    }
    async fn update_lease_hostname(
        &self,
        _id: &str,
        _hostname: Option<&str>,
    ) -> anyhow::Result<()> {
        unreachable!()
    }
    async fn expire_stale_leases(&self) -> anyhow::Result<u64> {
        unreachable!()
    }
    async fn insert_reservation(&self, _row: &DhcpReservationRow) -> anyhow::Result<()> {
        unreachable!()
    }
    async fn delete_reservation(&self, _id: &str) -> anyhow::Result<()> {
        unreachable!()
    }
    async fn list_reservations(&self) -> anyhow::Result<Vec<DhcpReservation>> {
        unreachable!()
    }
    async fn find_reservation_by_mac(&self, _mac: &str) -> anyhow::Result<Option<DhcpReservation>> {
        unreachable!()
    }
    async fn find_reservation_by_ip(&self, _ip: &str) -> anyhow::Result<Option<DhcpReservation>> {
        unreachable!()
    }
    async fn insert_lease_log(&self, _row: &DhcpLeaseLogRow) -> anyhow::Result<()> {
        unreachable!()
    }
    async fn find_lease_logs(&self, _lease_id: &str) -> anyhow::Result<Vec<DhcpLeaseLog>> {
        unreachable!()
    }
    async fn find_lease_logs_by_mac(&self, _mac: &str) -> anyhow::Result<Vec<DhcpLeaseLog>> {
        unreachable!()
    }
}

// -- Helpers --------------------------------------------------------------

fn sample_device(id: &str, mac: &str, ip: &str) -> Device {
    Device {
        id: Uuid::parse_str(id).unwrap(),
        mac: mac.to_owned(),
        name: None,
        hostname: None,
        manufacturer: Some("Apple".to_owned()),
        manufacturer_source: None,
        is_randomized: false,
        device_type: DeviceType::Phone,
        first_seen: "2026-03-07T00:00:00Z".parse().unwrap(),
        last_seen: "2026-03-07T00:00:00Z".parse().unwrap(),
        last_ip: ip.to_owned(),
        admin_locked: false,
        zone_id: "00000000-0000-0000-0000-000000000201".parse().unwrap(),
        dns_capture_enabled: false,
        dns_capture_cap_count: 1000,
        dns_capture_cap_days: 7,
        connection_mode: DeviceConnectionMode::Lan,
        managed: false,
    }
}

fn sample_observation(mac: &str, ip: &str) -> ObservedDevice {
    ObservedDevice {
        mac: mac.to_owned(),
        ip: ip.to_owned(),
        source: PacketSource::Arp,
    }
}

/// In-memory `SystemConfigRepository` for discovery tests. Only the generic
/// key-value surface is backed; the typed helpers (incl. the #738 quarantine
/// toggle) come from the trait's default impls over `get`/`set`.
#[derive(Default)]
struct MockSystemConfig {
    values: Mutex<HashMap<String, String>>,
}

impl MockSystemConfig {
    /// Flip the `quarantine_new_devices` toggle for a test.
    fn set_quarantine(&self, enabled: bool) {
        self.values.lock().unwrap().insert(
            "quarantine_new_devices".to_owned(),
            if enabled { "true" } else { "false" }.to_owned(),
        );
    }
}

#[async_trait]
impl SystemConfigRepository for MockSystemConfig {
    async fn get(&self, key: &str) -> anyhow::Result<Option<String>> {
        Ok(self.values.lock().unwrap().get(key).cloned())
    }
    async fn set(&self, key: &str, value: &str) -> anyhow::Result<()> {
        self.values
            .lock()
            .unwrap()
            .insert(key.to_owned(), value.to_owned());
        Ok(())
    }
    async fn delete(&self, key: &str) -> anyhow::Result<()> {
        self.values.lock().unwrap().remove(key);
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

struct TestHarness {
    svc: DeviceDiscoveryServiceImpl,
    repo: Arc<MockDeviceRepo>,
    events: Arc<MockEventPublisher>,
    system_config: Arc<MockSystemConfig>,
}

/// Thin wrappers that run the admin-guarded discovery methods under an admin
/// [`AuthContext`], mirroring how the background `device_detector` tasks wrap
/// every call. Tests that specifically exercise the auth guard call `svc`
/// directly with the context they want.
impl TestHarness {
    async fn restore_devices(&self) -> Result<(), AppError> {
        auth_context::with_context(admin_ctx(), self.svc.restore_devices()).await
    }

    async fn process_observation(
        &self,
        obs: &ObservedDevice,
    ) -> Result<ObservationResult, AppError> {
        auth_context::with_context(admin_ctx(), self.svc.process_observation(obs)).await
    }

    async fn flush_last_seen(&self) -> Result<u64, AppError> {
        auth_context::with_context(admin_ctx(), self.svc.flush_last_seen()).await
    }

    async fn scan_departures(&self, timeout_secs: u64) -> Result<Vec<Uuid>, AppError> {
        auth_context::with_context(admin_ctx(), self.svc.scan_departures(timeout_secs)).await
    }

    async fn resolve_hostname(&self, mac: &str, ip: &str) -> Result<(), AppError> {
        auth_context::with_context(admin_ctx(), self.svc.resolve_hostname(mac, ip)).await
    }

    async fn prune_unmanaged_devices(&self) -> Result<u64, AppError> {
        auth_context::with_context(admin_ctx(), self.svc.prune_unmanaged_devices()).await
    }
}

fn build_harness() -> TestHarness {
    build_harness_with_devices(Vec::new())
}

fn build_harness_with_devices(devices: Vec<Device>) -> TestHarness {
    let repo = Arc::new(MockDeviceRepo::with_devices(devices));
    let dhcp = Arc::new(MockDhcpRepository::empty());
    let events = Arc::new(MockEventPublisher::new());
    let resolver = Arc::new(MockHostnameResolver::new());
    let system_config = Arc::new(MockSystemConfig::default());
    let svc = DeviceDiscoveryServiceImpl::new(
        repo.clone(),
        Arc::new(MockNetworkZoneRepo),
        dhcp,
        system_config.clone(),
        events.clone(),
        resolver,
        "192.168.1.0/24".parse().unwrap(),
        std::net::Ipv4Addr::new(192, 168, 1, 1),
    );

    TestHarness {
        svc,
        repo,
        events,
        system_config,
    }
}

fn build_harness_with_resolver(
    devices: Vec<Device>,
    resolver_mappings: HashMap<String, String>,
) -> TestHarness {
    build_harness_with_resolver_and_dhcp(devices, resolver_mappings, MockDhcpRepository::empty())
}

fn build_harness_with_resolver_and_dhcp(
    devices: Vec<Device>,
    resolver_mappings: HashMap<String, String>,
    dhcp: MockDhcpRepository,
) -> TestHarness {
    let repo = Arc::new(MockDeviceRepo::with_devices(devices));
    let events = Arc::new(MockEventPublisher::new());
    let resolver = Arc::new(MockHostnameResolver::with_mappings(resolver_mappings));
    let system_config = Arc::new(MockSystemConfig::default());
    let svc = DeviceDiscoveryServiceImpl::new(
        repo.clone(),
        Arc::new(MockNetworkZoneRepo),
        Arc::new(dhcp),
        system_config.clone(),
        events.clone(),
        resolver,
        "192.168.1.0/24".parse().unwrap(),
        std::net::Ipv4Addr::new(192, 168, 1, 1),
    );

    TestHarness {
        svc,
        repo,
        events,
        system_config,
    }
}

fn build_harness_with_zone_repo(zones: Arc<dyn NetworkZoneRepository>) -> TestHarness {
    let repo = Arc::new(MockDeviceRepo::with_devices(Vec::new()));
    let dhcp = Arc::new(MockDhcpRepository::empty());
    let events = Arc::new(MockEventPublisher::new());
    let resolver = Arc::new(MockHostnameResolver::new());
    let system_config = Arc::new(MockSystemConfig::default());
    let svc = DeviceDiscoveryServiceImpl::new(
        repo.clone(),
        zones,
        dhcp,
        system_config.clone(),
        events.clone(),
        resolver,
        "192.168.1.0/24".parse().unwrap(),
        std::net::Ipv4Addr::new(192, 168, 1, 1),
    );

    TestHarness {
        svc,
        repo,
        events,
        system_config,
    }
}

// -- Tests ----------------------------------------------------------------

/// An observation from an IP inside a configured zone subnet (outside the base
/// LAN `/24`) is processed once the trusted-subnet cache has been rebuilt from
/// the zone list — not dropped as off-LAN.
#[tokio::test]
async fn process_observation_accepts_zone_subnet_ip() {
    let zones = Arc::new(MockZoneRepoWithSubnets::new(vec![zone_with_subnet(
        "10.0.100.0/24",
    )]));
    let h = build_harness_with_zone_repo(zones);

    // Fold the zone subnet into the trusted set (as startup / a zone event does).
    h.svc.rebuild_trusted_subnets().await.unwrap();

    let obs = sample_observation("aa:bb:cc:dd:ee:10", "10.0.100.50");
    let result = h.process_observation(&obs).await.unwrap();

    assert!(
        !matches!(result, ObservationResult::Ignored),
        "expected the zone-subnet observation to be processed, got {result:?}"
    );
    assert!(
        matches!(result, ObservationResult::NewDevice { .. }),
        "expected a new device to be registered, got {result:?}"
    );
}

/// An observation from a WAN-like IP outside the LAN and every zone subnet is
/// still ignored (return traffic arriving with the router's MAC).
#[tokio::test]
async fn process_observation_ignores_wan_ip_outside_all_subnets() {
    let zones = Arc::new(MockZoneRepoWithSubnets::new(vec![zone_with_subnet(
        "10.0.100.0/24",
    )]));
    let h = build_harness_with_zone_repo(zones);
    h.svc.rebuild_trusted_subnets().await.unwrap();

    let obs = sample_observation("aa:bb:cc:dd:ee:11", "8.8.8.8");
    let result = h.process_observation(&obs).await.unwrap();

    assert!(
        matches!(result, ObservationResult::Ignored),
        "expected a WAN IP to be ignored, got {result:?}"
    );
    assert!(
        h.repo.inserted.lock().unwrap().is_empty(),
        "no device should have been inserted for a WAN observation"
    );
}

/// Rebuilding the trusted-subnet cache picks up a zone subnet added after the
/// service was constructed: the same observation flips from Ignored to processed.
#[tokio::test]
async fn rebuild_trusted_subnets_picks_up_new_zone_subnet() {
    // Start with a zone that has no subnet, so only the base LAN /24 is trusted.
    let zones = Arc::new(MockZoneRepoWithSubnets::new(vec![trusted_zone()]));
    let h = build_harness_with_zone_repo(zones.clone());
    h.svc.rebuild_trusted_subnets().await.unwrap();

    let obs = sample_observation("aa:bb:cc:dd:ee:12", "10.0.100.50");
    let before = h.process_observation(&obs).await.unwrap();
    assert!(
        matches!(before, ObservationResult::Ignored),
        "expected the zone-subnet IP to be ignored before the subnet is configured, got {before:?}"
    );

    // Configure the zone subnet and rebuild — the same IP is now trusted.
    zones.set_zones(vec![zone_with_subnet("10.0.100.0/24")]);
    h.svc.rebuild_trusted_subnets().await.unwrap();

    let after = h.process_observation(&obs).await.unwrap();
    assert!(
        !matches!(after, ObservationResult::Ignored),
        "expected the observation to be processed after the zone subnet is added, got {after:?}"
    );
}

#[tokio::test]
async fn process_observation_new_device() {
    let h = build_harness();
    let obs = sample_observation("aa:bb:cc:dd:ee:01", "192.168.1.10");

    let result = h.process_observation(&obs).await.unwrap();
    assert!(
        matches!(result, ObservationResult::NewDevice { .. }),
        "expected NewDevice, got {result:?}"
    );

    // Verify device was inserted.
    let inserted = h.repo.inserted.lock().unwrap();
    assert_eq!(inserted.len(), 1);
    assert_eq!(inserted[0].mac, "aa:bb:cc:dd:ee:01");
    assert_eq!(inserted[0].last_ip, "192.168.1.10");

    // Verify DeviceDiscovered event was emitted.
    let events = h.events.published_events();
    assert_eq!(events.len(), 1);
    assert!(matches!(
        &events[0],
        WardnetEvent::DeviceDiscovered {
            mac, ip, ..
        } if mac == "aa:bb:cc:dd:ee:01" && ip == "192.168.1.10"
    ));
}

#[tokio::test]
async fn process_observation_known_device_same_ip() {
    let h = build_harness();
    let obs = sample_observation("aa:bb:cc:dd:ee:01", "192.168.1.10");

    // First observation registers the device.
    h.process_observation(&obs).await.unwrap();

    // Clear events from the first observation.
    h.events.events.lock().unwrap().clear();

    // Second observation with same MAC and IP.
    let result = h.process_observation(&obs).await.unwrap();
    assert!(
        matches!(result, ObservationResult::Seen(_)),
        "expected Seen, got {result:?}"
    );

    // No events emitted for a simple "seen".
    let events = h.events.published_events();
    assert!(events.is_empty(), "expected no events, got {events:?}");
}

#[tokio::test]
async fn process_observation_known_device_different_ip() {
    let h = build_harness();
    let obs1 = sample_observation("aa:bb:cc:dd:ee:01", "192.168.1.10");
    h.process_observation(&obs1).await.unwrap();

    // Clear events.
    h.events.events.lock().unwrap().clear();

    // Second observation with same MAC but different IP.
    let obs2 = sample_observation("aa:bb:cc:dd:ee:01", "192.168.1.20");
    let result = h.process_observation(&obs2).await.unwrap();
    assert!(
        matches!(
            &result,
            ObservationResult::IpChanged {
                old_ip,
                ..
            } if old_ip == "192.168.1.10"
        ),
        "expected IpChanged, got {result:?}"
    );

    // Verify DeviceIpChanged event was emitted.
    let events = h.events.published_events();
    assert_eq!(events.len(), 1);
    assert!(matches!(
        &events[0],
        WardnetEvent::DeviceIpChanged {
            old_ip,
            new_ip,
            ..
        } if old_ip == "192.168.1.10" && new_ip == "192.168.1.20"
    ));
}

#[tokio::test]
async fn process_observation_gone_device_returns() {
    let h = build_harness();
    let obs = sample_observation("aa:bb:cc:dd:ee:01", "192.168.1.10");

    // Register device.
    h.process_observation(&obs).await.unwrap();

    // Mark it as departed (use a tiny timeout so it departs immediately).
    // We need to wait a small amount of time for the Instant to advance.
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    let departed = h.scan_departures(0).await.unwrap();
    assert_eq!(departed.len(), 1);

    // Clear events.
    h.events.events.lock().unwrap().clear();

    // Device reappears.
    let result = h.process_observation(&obs).await.unwrap();
    assert!(
        matches!(result, ObservationResult::Reappeared(_)),
        "expected Reappeared, got {result:?}"
    );

    // Verify DeviceDiscovered event was emitted (reappearance).
    let events = h.events.published_events();
    assert_eq!(events.len(), 1);
    assert!(matches!(&events[0], WardnetEvent::DeviceDiscovered { .. }));
}

#[tokio::test]
async fn flush_last_seen_updates_batch() {
    let h = build_harness();
    let obs = sample_observation("aa:bb:cc:dd:ee:01", "192.168.1.10");
    h.process_observation(&obs).await.unwrap();

    let count = h.flush_last_seen().await.unwrap();
    assert_eq!(count, 1);

    let batch = h.repo.batch_updates.lock().unwrap();
    assert_eq!(batch.len(), 1);
}

#[tokio::test]
async fn scan_departures_emits_device_gone() {
    let h = build_harness();
    let obs = sample_observation("aa:bb:cc:dd:ee:01", "192.168.1.10");
    h.process_observation(&obs).await.unwrap();

    // Clear events from registration.
    h.events.events.lock().unwrap().clear();

    // Wait briefly and scan with a zero timeout to force departure.
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    let departed = h.scan_departures(0).await.unwrap();
    assert_eq!(departed.len(), 1);

    // Verify DeviceGone event was emitted.
    let events = h.events.published_events();
    assert_eq!(events.len(), 1);
    assert!(matches!(
        &events[0],
        WardnetEvent::DeviceGone {
            mac, last_ip, ..
        } if mac == "aa:bb:cc:dd:ee:01" && last_ip == "192.168.1.10"
    ));
}

// Regression for issue #831: when the sweep marks a device gone it must clear
// the device's stored `last_ip`, so a departed device's row can no longer be
// resolved by a live device's source IP in `find_by_ip` once DHCP recycles the
// address.
#[tokio::test]
async fn scan_departures_clears_departed_last_ip() {
    let h = build_harness();
    let obs = sample_observation("aa:bb:cc:dd:ee:01", "192.168.1.10");
    h.process_observation(&obs).await.unwrap();

    // Precondition: the freshly discovered device resolves by its IP.
    assert!(h.repo.find_by_ip("192.168.1.10").await.unwrap().is_some());

    // Force the departure sweep.
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    let departed = h.scan_departures(0).await.unwrap();
    assert_eq!(departed.len(), 1);

    // The departed device's IP is cleared: its row no longer resolves by that
    // address, closing the collision window against a device DHCP later hands
    // the same IP.
    assert!(
        h.repo.find_by_ip("192.168.1.10").await.unwrap().is_none(),
        "departed device must no longer be resolvable by its old IP"
    );
    let device = h
        .repo
        .find_by_id(&departed[0].to_string())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(device.last_ip, "");
}

// A failure clearing the departed device's IP must not abort the sweep: the
// device is still reported departed (so downstream listeners run) and the
// failure is logged rather than propagated.
#[tokio::test]
async fn scan_departures_tolerates_clear_last_ip_failure() {
    let h = build_harness();
    let obs = sample_observation("aa:bb:cc:dd:ee:01", "192.168.1.10");
    h.process_observation(&obs).await.unwrap();
    h.repo
        .fail_clear_last_ip
        .store(true, std::sync::atomic::Ordering::SeqCst);

    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    let departed = h.scan_departures(0).await.unwrap();
    assert_eq!(departed.len(), 1);

    // The clear failed, so the IP is left in place — but the sweep still
    // completed and emitted the departure event.
    let events = h.events.published_events();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, WardnetEvent::DeviceGone { .. }))
    );
}

// Regression for issue #831: a device that reconnects in the same instant as
// the departure sweep must keep the live IP its observation just wrote — the
// sweep's IP clear must not clobber it back to empty.
#[tokio::test]
async fn scan_departures_does_not_clear_ip_of_reconnected_device() {
    let h = build_harness();
    let obs = sample_observation("aa:bb:cc:dd:ee:01", "192.168.1.10");
    h.process_observation(&obs).await.unwrap();

    // Depart, then immediately reconnect at the same address before asserting.
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    h.scan_departures(0).await.unwrap();
    let result = h.process_observation(&obs).await.unwrap();
    assert!(
        matches!(result, ObservationResult::Reappeared(_)),
        "expected Reappeared, got {result:?}"
    );

    // The reconnect re-established the address; it must still be resolvable.
    assert!(
        h.repo.find_by_ip("192.168.1.10").await.unwrap().is_some(),
        "reconnected device must remain resolvable by its IP"
    );
}

// Regression for issue #831: the WireGuard handshake-staleness departure path
// (`mark_peer_gone`) must clear `last_ip` too, mirroring the timeout sweep, so a
// stale remote peer's row can't collide with a live device's source IP.
#[tokio::test]
async fn mark_peer_gone_clears_departed_last_ip() {
    let h = build_harness();
    let obs = sample_observation("aa:bb:cc:dd:ee:01", "192.168.1.10");
    h.process_observation(&obs).await.unwrap();
    let device_id = h.repo.find_by_ip("192.168.1.10").await.unwrap().unwrap().id;

    // A zero timeout marks the peer gone once its last_seen has elapsed.
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    let gone = h
        .svc
        .mark_peer_gone(device_id, std::time::Duration::from_secs(0))
        .await
        .unwrap();
    assert_eq!(gone, Some(device_id));

    // The departed peer's IP is cleared, closing the same collision window the
    // timeout sweep does.
    assert!(
        h.repo.find_by_ip("192.168.1.10").await.unwrap().is_none(),
        "gone peer must no longer be resolvable by its old IP"
    );
}

#[tokio::test]
async fn scan_departures_ignores_recent() {
    let h = build_harness();
    let obs = sample_observation("aa:bb:cc:dd:ee:01", "192.168.1.10");
    h.process_observation(&obs).await.unwrap();

    // Clear events.
    h.events.events.lock().unwrap().clear();

    // Scan with a very large timeout -- device should not be departed.
    let departed = h.scan_departures(3600).await.unwrap();
    assert!(departed.is_empty());

    // No events emitted.
    let events = h.events.published_events();
    assert!(events.is_empty());
}

#[tokio::test]
async fn get_all_devices_as_admin() {
    let device = sample_device(
        "00000000-0000-0000-0000-000000000001",
        "aa:bb:cc:dd:ee:01",
        "192.168.1.10",
    );
    let h = build_harness_with_devices(vec![device]);

    let devices = auth_context::with_context(admin_ctx(), h.svc.get_all_devices())
        .await
        .unwrap();
    assert_eq!(devices.len(), 1);
    assert_eq!(devices[0].mac, "aa:bb:cc:dd:ee:01");
}

#[tokio::test]
async fn get_all_devices_as_device_returns_own_only() {
    let dev_a = sample_device(
        "00000000-0000-0000-0000-000000000001",
        "aa:bb:cc:dd:ee:01",
        "192.168.1.10",
    );
    let dev_b = sample_device(
        "00000000-0000-0000-0000-000000000002",
        "aa:bb:cc:dd:ee:02",
        "192.168.1.11",
    );
    let h = build_harness_with_devices(vec![dev_a, dev_b]);

    let found =
        auth_context::with_context(device_ctx("aa:bb:cc:dd:ee:01"), h.svc.get_all_devices())
            .await
            .unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].mac, "aa:bb:cc:dd:ee:01");
}

#[tokio::test]
async fn get_all_devices_anonymous_forbidden() {
    let h = build_harness();
    let result = auth_context::with_context(AuthContext::Anonymous, h.svc.get_all_devices()).await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}

#[tokio::test]
async fn get_device_by_id_not_found() {
    let h = build_harness();
    let result =
        auth_context::with_context(admin_ctx(), h.svc.get_device_by_id(Uuid::new_v4())).await;
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), AppError::NotFound(_)));
}

#[tokio::test]
async fn get_device_by_id_anonymous_forbidden() {
    let h = build_harness();
    let result = auth_context::with_context(
        AuthContext::Anonymous,
        h.svc.get_device_by_id(Uuid::new_v4()),
    )
    .await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}

#[tokio::test]
async fn get_device_by_id_device_can_view_own() {
    let device = sample_device(
        "00000000-0000-0000-0000-000000000001",
        "aa:bb:cc:dd:ee:01",
        "192.168.1.10",
    );
    let id = device.id;
    let h = build_harness_with_devices(vec![device]);

    let result =
        auth_context::with_context(device_ctx("aa:bb:cc:dd:ee:01"), h.svc.get_device_by_id(id))
            .await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap().mac, "aa:bb:cc:dd:ee:01");
}

#[tokio::test]
async fn get_device_by_id_device_cannot_view_other() {
    let device = sample_device(
        "00000000-0000-0000-0000-000000000001",
        "aa:bb:cc:dd:ee:01",
        "192.168.1.10",
    );
    let id = device.id;
    let h = build_harness_with_devices(vec![device]);

    let result =
        auth_context::with_context(device_ctx("aa:bb:cc:dd:ee:99"), h.svc.get_device_by_id(id))
            .await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}

#[tokio::test]
async fn update_device_sets_name_and_type_as_admin() {
    let device = sample_device(
        "00000000-0000-0000-0000-000000000001",
        "aa:bb:cc:dd:ee:01",
        "192.168.1.10",
    );
    let id = device.id;
    let h = build_harness_with_devices(vec![device]);

    let result = auth_context::with_context(
        admin_ctx(),
        h.svc
            .update_device(id, Some("Living Room TV"), Some(DeviceType::Tv)),
    )
    .await
    .unwrap();

    assert_eq!(result.name, Some("Living Room TV".to_owned()));
    assert_eq!(result.device_type, DeviceType::Tv);

    // Verify the update was recorded in the repository.
    let recorded = h.repo.name_type_updates.lock().unwrap();
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0].1, Some("Living Room TV".to_owned()));
}

#[tokio::test]
async fn update_device_preserves_name_when_omitted() {
    // A partial update that changes only the device type (e.g. what a
    // set-rule or admin-lock call sends) must not null out the display name.
    let mut device = sample_device(
        "00000000-0000-0000-0000-000000000001",
        "aa:bb:cc:dd:ee:01",
        "192.168.1.10",
    );
    device.name = Some("Alice's phone".to_owned());
    let id = device.id;
    let h = build_harness_with_devices(vec![device]);

    auth_context::with_context(
        admin_ctx(),
        h.svc.update_device(id, None, Some(DeviceType::Laptop)),
    )
    .await
    .unwrap();

    // The persisted write carries the existing name, not None (which the
    // repository would translate to SQL NULL, clearing the name).
    let recorded = h.repo.name_type_updates.lock().unwrap();
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0].1, Some("Alice's phone".to_owned()));
}

#[tokio::test]
async fn update_device_anonymous_forbidden() {
    let device = sample_device(
        "00000000-0000-0000-0000-000000000001",
        "aa:bb:cc:dd:ee:01",
        "192.168.1.10",
    );
    let id = device.id;
    let h = build_harness_with_devices(vec![device]);

    let result = auth_context::with_context(
        AuthContext::Anonymous,
        h.svc.update_device(id, Some("name"), None),
    )
    .await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}

#[tokio::test]
async fn update_device_as_device_own_allowed() {
    let device = sample_device(
        "00000000-0000-0000-0000-000000000001",
        "aa:bb:cc:dd:ee:01",
        "192.168.1.10",
    );
    let id = device.id;
    let h = build_harness_with_devices(vec![device]);

    let result = auth_context::with_context(
        device_ctx("aa:bb:cc:dd:ee:01"),
        h.svc.update_device(id, Some("My Phone"), None),
    )
    .await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap().name, Some("My Phone".to_owned()));
}

#[tokio::test]
async fn update_device_as_device_other_forbidden() {
    let device = sample_device(
        "00000000-0000-0000-0000-000000000001",
        "aa:bb:cc:dd:ee:01",
        "192.168.1.10",
    );
    let id = device.id;
    let h = build_harness_with_devices(vec![device]);

    let result = auth_context::with_context(
        device_ctx("aa:bb:cc:dd:ee:99"),
        h.svc.update_device(id, Some("name"), None),
    )
    .await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}

#[tokio::test]
async fn update_device_as_device_admin_locked_forbidden() {
    let mut device = sample_device(
        "00000000-0000-0000-0000-000000000001",
        "aa:bb:cc:dd:ee:01",
        "192.168.1.10",
    );
    device.admin_locked = true;
    let id = device.id;
    let h = build_harness_with_devices(vec![device]);

    let result = auth_context::with_context(
        device_ctx("aa:bb:cc:dd:ee:01"),
        h.svc.update_device(id, Some("name"), None),
    )
    .await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}

#[tokio::test]
async fn resolve_hostname_falls_back_to_reverse_dns() {
    let device = sample_device(
        "00000000-0000-0000-0000-000000000001",
        "aa:bb:cc:dd:ee:01",
        "192.168.1.10",
    );
    let id = device.id;
    let mut mappings = HashMap::new();
    mappings.insert("192.168.1.10".to_owned(), "myphone.local".to_owned());
    let h = build_harness_with_resolver(vec![device], mappings);

    h.resolve_hostname("aa:bb:cc:dd:ee:01", "192.168.1.10")
        .await
        .unwrap();

    let updates = h.repo.hostname_updates.lock().unwrap();
    assert_eq!(updates.len(), 1);
    assert_eq!(updates[0].0, id.to_string());
    assert_eq!(updates[0].1, "myphone.local");
}

#[tokio::test]
async fn resolve_hostname_no_result_does_not_update_db() {
    let device = sample_device(
        "00000000-0000-0000-0000-000000000001",
        "aa:bb:cc:dd:ee:01",
        "192.168.1.10",
    );
    // Empty resolver and no DHCP lease: nothing to write.
    let h = build_harness_with_resolver(vec![device], HashMap::new());

    h.resolve_hostname("aa:bb:cc:dd:ee:01", "192.168.1.10")
        .await
        .unwrap();

    let updates = h.repo.hostname_updates.lock().unwrap();
    assert!(
        updates.is_empty(),
        "no hostname update when neither DHCP nor reverse DNS produce a value"
    );
}

#[tokio::test]
async fn resolve_hostname_prefers_dhcp_over_reverse_dns() {
    let device = sample_device(
        "00000000-0000-0000-0000-000000000001",
        "aa:bb:cc:dd:ee:01",
        "192.168.1.10",
    );
    let id = device.id;

    // Reverse DNS would return "generic.local", but the active lease has
    // an option-12 hostname — that should win per FR-022.
    let mut mappings = HashMap::new();
    mappings.insert("192.168.1.10".to_owned(), "generic.local".to_owned());
    // The real DHCP service writes MACs lowercase before insert, so the
    // mock follows the same convention.
    let dhcp = MockDhcpRepository::with_lease_hostnames(vec![(
        "aa:bb:cc:dd:ee:01",
        Some("living-room-tv"),
    )]);
    let h = build_harness_with_resolver_and_dhcp(vec![device], mappings, dhcp);

    h.resolve_hostname("aa:bb:cc:dd:ee:01", "192.168.1.10")
        .await
        .unwrap();

    let updates = h.repo.hostname_updates.lock().unwrap();
    assert_eq!(updates.len(), 1);
    assert_eq!(updates[0].0, id.to_string());
    assert_eq!(updates[0].1, "living-room-tv");
}

#[tokio::test]
async fn resolve_hostname_falls_back_when_dhcp_value_empty() {
    let device = sample_device(
        "00000000-0000-0000-0000-000000000001",
        "aa:bb:cc:dd:ee:01",
        "192.168.1.10",
    );

    // Empty / whitespace-only option-12 counts as absent — fall through.
    let mut mappings = HashMap::new();
    mappings.insert("192.168.1.10".to_owned(), "myphone.local".to_owned());
    let dhcp = MockDhcpRepository::with_lease_hostnames(vec![("aa:bb:cc:dd:ee:01", Some("   "))]);
    let h = build_harness_with_resolver_and_dhcp(vec![device], mappings, dhcp);

    h.resolve_hostname("aa:bb:cc:dd:ee:01", "192.168.1.10")
        .await
        .unwrap();

    let updates = h.repo.hostname_updates.lock().unwrap();
    assert_eq!(updates.len(), 1);
    assert_eq!(updates[0].1, "myphone.local");
}

#[tokio::test]
async fn resolve_hostname_falls_back_when_no_active_lease() {
    let device = sample_device(
        "00000000-0000-0000-0000-000000000001",
        "aa:bb:cc:dd:ee:01",
        "192.168.1.10",
    );

    let mut mappings = HashMap::new();
    mappings.insert("192.168.1.10".to_owned(), "myphone.local".to_owned());
    // No DHCP lease for this MAC — should fall back to reverse DNS.
    let h =
        build_harness_with_resolver_and_dhcp(vec![device], mappings, MockDhcpRepository::empty());

    h.resolve_hostname("aa:bb:cc:dd:ee:01", "192.168.1.10")
        .await
        .unwrap();

    let updates = h.repo.hostname_updates.lock().unwrap();
    assert_eq!(updates.len(), 1);
    assert_eq!(updates[0].1, "myphone.local");
}

#[tokio::test]
async fn resolve_hostname_no_op_when_device_not_registered() {
    // Lease arrives before packet capture has registered the device — must
    // not error, must not write.
    let dhcp = MockDhcpRepository::with_lease_hostnames(vec![("aa:bb:cc:dd:ee:99", Some("ghost"))]);
    let h = build_harness_with_resolver_and_dhcp(vec![], HashMap::new(), dhcp);

    h.resolve_hostname("aa:bb:cc:dd:ee:99", "192.168.1.99")
        .await
        .unwrap();

    let updates = h.repo.hostname_updates.lock().unwrap();
    assert!(updates.is_empty());
}

#[tokio::test]
async fn resolve_hostname_tolerates_mixed_case_caller() {
    // Even though MAC is canonicalised to lowercase across the data
    // layer post-#312, callers (and historical event payloads) may
    // arrive with mixed case. The repository layer normalises at the
    // boundary, so a single uppercase MAC fed to `resolve_hostname`
    // must still match the lowercase rows in both the device registry
    // and the DHCP repository.
    let device = sample_device(
        "00000000-0000-0000-0000-000000000001",
        "aa:bb:cc:dd:ee:01",
        "192.168.1.10",
    );
    let id = device.id;

    let dhcp = MockDhcpRepository::with_lease_hostnames(vec![(
        "aa:bb:cc:dd:ee:01",
        Some("kitchen-tablet"),
    )]);
    let h = build_harness_with_resolver_and_dhcp(vec![device], HashMap::new(), dhcp);

    // Caller uses uppercase; both lookups must still hit.
    h.resolve_hostname("AA:BB:CC:DD:EE:01", "192.168.1.10")
        .await
        .unwrap();

    let updates = h.repo.hostname_updates.lock().unwrap();
    assert_eq!(updates.len(), 1);
    assert_eq!(updates[0].0, id.to_string());
    assert_eq!(updates[0].1, "kitchen-tablet");
}

#[tokio::test]
async fn restore_devices_populates_in_memory_state() {
    let device = sample_device(
        "00000000-0000-0000-0000-000000000001",
        "aa:bb:cc:dd:ee:01",
        "192.168.1.10",
    );
    let h = build_harness_with_devices(vec![device]);

    h.restore_devices().await.unwrap();

    // After restore, processing the same MAC should not create a new device
    // because restore marks devices as gone; re-observing should reappear them.
    let obs = sample_observation("aa:bb:cc:dd:ee:01", "192.168.1.10");
    let result = h.process_observation(&obs).await.unwrap();
    assert!(
        matches!(result, ObservationResult::Reappeared(_)),
        "expected Reappeared after restore, got {result:?}"
    );

    // Verify no insert was done (reappearance, not new).
    let inserted = h.repo.inserted.lock().unwrap();
    assert!(inserted.is_empty(), "device should not be re-inserted");
}

#[tokio::test]
async fn restore_devices_empty_repo() {
    let h = build_harness();

    // Restore with no devices should succeed.
    h.restore_devices().await.unwrap();
}

#[tokio::test]
async fn update_device_without_device_type_preserves_existing() {
    let device = sample_device(
        "00000000-0000-0000-0000-000000000001",
        "aa:bb:cc:dd:ee:01",
        "192.168.1.10",
    );
    let id = device.id;
    let h = build_harness_with_devices(vec![device]);

    // Update only name, leaving device_type as None.
    let result = auth_context::with_context(
        admin_ctx(),
        h.svc.update_device(id, Some("Kitchen Tablet"), None),
    )
    .await
    .unwrap();

    assert_eq!(result.name, Some("Kitchen Tablet".to_owned()));
    // Device type should be preserved from the original.
    assert_eq!(result.device_type, DeviceType::Phone);
}

#[tokio::test]
async fn update_device_not_found() {
    let h = build_harness();
    let result = auth_context::with_context(
        admin_ctx(),
        h.svc.update_device(Uuid::new_v4(), Some("name"), None),
    )
    .await;
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), AppError::NotFound(_)));
}

#[tokio::test]
async fn get_device_by_id_success() {
    let device = sample_device(
        "00000000-0000-0000-0000-000000000001",
        "aa:bb:cc:dd:ee:01",
        "192.168.1.10",
    );
    let id = device.id;
    let h = build_harness_with_devices(vec![device]);

    let result = auth_context::with_context(admin_ctx(), h.svc.get_device_by_id(id))
        .await
        .unwrap();
    assert_eq!(result.id, id);
    assert_eq!(result.mac, "aa:bb:cc:dd:ee:01");
}

#[tokio::test]
async fn flush_last_seen_with_gone_devices_skips_them() {
    let h = build_harness();
    let obs = sample_observation("aa:bb:cc:dd:ee:01", "192.168.1.10");
    h.process_observation(&obs).await.unwrap();

    // Mark device as departed.
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    h.scan_departures(0).await.unwrap();

    // Flush should skip gone devices.
    let count = h.flush_last_seen().await.unwrap();
    assert_eq!(count, 0, "gone devices should not be flushed");
}

#[tokio::test]
async fn process_observation_reappear_from_db_not_memory() {
    // Device exists in DB (from a previous daemon run) but not in memory.
    let device = sample_device(
        "00000000-0000-0000-0000-000000000001",
        "aa:bb:cc:dd:ee:01",
        "192.168.1.10",
    );
    let h = build_harness_with_devices(vec![device]);

    // Do NOT call restore_devices, so the in-memory map is empty.
    // Observing should find the device in the DB and reappear it.
    let obs = sample_observation("aa:bb:cc:dd:ee:01", "192.168.1.20");
    let result = h.process_observation(&obs).await.unwrap();
    assert!(
        matches!(result, ObservationResult::Reappeared(_)),
        "expected Reappeared from DB, got {result:?}"
    );

    // Verify a DeviceDiscovered event was emitted.
    let events = h.events.published_events();
    assert_eq!(events.len(), 1);
    assert!(matches!(&events[0], WardnetEvent::DeviceDiscovered { .. }));
}

// -- New-device quarantine (#738) -----------------------------------------

/// Toggle ON + a brand-new MAC ⇒ a `NewDeviceQuarantined` event is published
/// (alongside the usual `DeviceDiscovered`), carrying the default-for-new zone
/// name so the admin push can say where the device landed.
#[tokio::test]
async fn quarantine_on_new_device_publishes_event() {
    let h = build_harness();
    h.system_config.set_quarantine(true);

    let obs = sample_observation("aa:bb:cc:dd:ee:01", "192.168.1.10");
    let result = h.process_observation(&obs).await.unwrap();
    assert!(matches!(result, ObservationResult::NewDevice { .. }));

    let events = h.events.published_events();
    // DeviceDiscovered (enforcement) is emitted before the quarantine nudge.
    assert!(matches!(&events[0], WardnetEvent::DeviceDiscovered { .. }));
    let quarantined = events
        .iter()
        .find(|e| matches!(e, WardnetEvent::NewDeviceQuarantined { .. }))
        .expect("expected a NewDeviceQuarantined event");
    assert!(matches!(
        quarantined,
        WardnetEvent::NewDeviceQuarantined { mac, zone_name, .. }
            if mac == "aa:bb:cc:dd:ee:01" && zone_name == "Trusted"
    ));
}

/// Toggle OFF (the default) ⇒ a brand-new MAC behaves exactly as today: no
/// `NewDeviceQuarantined` event.
#[tokio::test]
async fn quarantine_off_new_device_publishes_no_quarantine_event() {
    let h = build_harness();
    // Toggle left at its default (off).

    let obs = sample_observation("aa:bb:cc:dd:ee:01", "192.168.1.10");
    h.process_observation(&obs).await.unwrap();

    let events = h.events.published_events();
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, WardnetEvent::NewDeviceQuarantined { .. })),
        "expected no NewDeviceQuarantined event, got {events:?}"
    );
}

/// Toggle ON but the MAC already has a DB row (a previously-approved device
/// reconnecting) ⇒ it takes the reappear path, never `insert_new_device`, so it
/// is never re-quarantined.
#[tokio::test]
async fn quarantine_on_reappearing_device_is_not_requarantined() {
    let existing = sample_device(
        "00000000-0000-0000-0000-0000000000aa",
        "aa:bb:cc:dd:ee:01",
        "192.168.1.10",
    );
    let h = build_harness_with_devices(vec![existing]);
    h.system_config.set_quarantine(true);

    let obs = sample_observation("aa:bb:cc:dd:ee:01", "192.168.1.10");
    let result = h.process_observation(&obs).await.unwrap();
    assert!(
        matches!(result, ObservationResult::Reappeared(_)),
        "expected Reappeared, got {result:?}"
    );

    let events = h.events.published_events();
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, WardnetEvent::NewDeviceQuarantined { .. })),
        "a reconnecting device must not be re-quarantined, got {events:?}"
    );
}

// -- Inbound-WireGuard peer observations (#810) ---------------------------

/// A `device_id`-keyed peer observation on a device the service tracks as gone
/// (here: after `restore_devices` marks every device gone) takes the reappear
/// path: it publishes `DeviceDiscovered` and persists with
/// `connection_mode = Remote` (not `Lan`, unlike the LAN reappear path).
#[tokio::test]
async fn process_peer_observation_reappears_gone_device_as_remote() {
    let device = sample_device(
        "00000000-0000-0000-0000-000000000001",
        "aa:bb:cc:dd:ee:01",
        "192.168.1.10",
    );
    let device_id = device.id;
    let h = build_harness_with_devices(vec![device]);

    // Restore marks every device gone in the in-memory map.
    h.restore_devices().await.unwrap();
    h.events.events.lock().unwrap().clear();

    // Peer IPs live in the inbound-WG subnet, outside the LAN subnet — the
    // peer path skips the lan_subnet filter the LAN path applies.
    let result = h
        .svc
        .process_peer_observation(device_id, "10.100.64.2")
        .await
        .unwrap();
    assert!(
        matches!(result, ObservationResult::Reappeared(id) if id == device_id),
        "expected Reappeared, got {result:?}"
    );

    let events = h.events.published_events();
    assert_eq!(events.len(), 1);
    assert!(matches!(
        &events[0],
        WardnetEvent::DeviceDiscovered { device_id: d, ip, .. }
            if *d == device_id && ip == "10.100.64.2"
    ));

    // The write carries connection_mode = Remote.
    let updates = h.repo.last_seen_updates.lock().unwrap();
    let last = updates.last().expect("an update was recorded");
    assert_eq!(last.0, device_id.to_string());
    assert_eq!(last.1, "10.100.64.2");
    assert_eq!(last.3, DeviceConnectionMode::Remote);
}

/// A second peer observation with a different IP for an already-present device
/// fires the IP-changed path: `DeviceIpChanged` is published and the write
/// still carries `connection_mode = Remote`.
#[tokio::test]
async fn process_peer_observation_ip_changed_stays_remote() {
    let device = sample_device(
        "00000000-0000-0000-0000-000000000001",
        "aa:bb:cc:dd:ee:01",
        "192.168.1.10",
    );
    let device_id = device.id;
    let h = build_harness_with_devices(vec![device]);

    // First observation registers the peer as present in memory at .2.
    h.svc
        .process_peer_observation(device_id, "10.100.64.2")
        .await
        .unwrap();
    h.events.events.lock().unwrap().clear();

    // Second observation at a different peer IP.
    let result = h
        .svc
        .process_peer_observation(device_id, "10.100.64.3")
        .await
        .unwrap();
    assert!(
        matches!(
            &result,
            ObservationResult::IpChanged { device_id: d, old_ip }
                if *d == device_id && old_ip == "10.100.64.2"
        ),
        "expected IpChanged, got {result:?}"
    );

    let events = h.events.published_events();
    assert_eq!(events.len(), 1);
    assert!(matches!(
        &events[0],
        WardnetEvent::DeviceIpChanged { old_ip, new_ip, .. }
            if old_ip == "10.100.64.2" && new_ip == "10.100.64.3"
    ));

    let updates = h.repo.last_seen_updates.lock().unwrap();
    let last = updates.last().expect("an update was recorded");
    assert_eq!(last.3, DeviceConnectionMode::Remote);
}

/// A peer observation for a `device_id` with no matching device row is an
/// internal-invariant violation (the FK should make it impossible), so the
/// method fails with `AppError::Internal` rather than taking a new-device path.
#[tokio::test]
async fn process_peer_observation_unknown_device_errors() {
    let h = build_harness();

    let result = h
        .svc
        .process_peer_observation(Uuid::new_v4(), "10.100.64.2")
        .await;
    assert!(
        matches!(result, Err(AppError::Internal(_))),
        "expected Internal error for unknown device_id, got {result:?}"
    );
}

/// `mark_peer_gone` on a device currently tracked as present flips it gone,
/// publishes `DeviceGone`, and returns `Ok(Some(device_id))`.
#[tokio::test]
async fn mark_peer_gone_flips_present_device() {
    let device = sample_device(
        "00000000-0000-0000-0000-000000000001",
        "aa:bb:cc:dd:ee:01",
        "192.168.1.10",
    );
    let device_id = device.id;
    let h = build_harness_with_devices(vec![device]);

    // Make the device present in memory via a peer observation at .2.
    h.svc
        .process_peer_observation(device_id, "10.100.64.2")
        .await
        .unwrap();
    h.events.events.lock().unwrap().clear();

    // Zero timeout: the entry's shared `last_seen` is stale by any positive
    // window, so the WG-staleness departure flips it gone.
    let result = h
        .svc
        .mark_peer_gone(device_id, std::time::Duration::ZERO)
        .await
        .unwrap();
    assert_eq!(result, Some(device_id));

    let events = h.events.published_events();
    assert_eq!(events.len(), 1);
    assert!(matches!(
        &events[0],
        WardnetEvent::DeviceGone { device_id: d, mac, last_ip, .. }
            if *d == device_id && mac == "aa:bb:cc:dd:ee:01" && last_ip == "10.100.64.2"
    ));
}

/// `mark_peer_gone` on a device that exists in the repo but is not tracked as
/// present in memory (never observed by this instance) is a no-op: no event is
/// published and it returns `Ok(None)`.
#[tokio::test]
async fn mark_peer_gone_untracked_is_noop() {
    let device = sample_device(
        "00000000-0000-0000-0000-000000000001",
        "aa:bb:cc:dd:ee:01",
        "192.168.1.10",
    );
    let device_id = device.id;
    let h = build_harness_with_devices(vec![device]);

    // Never observed, so no in-memory tracking entry exists.
    let result = h
        .svc
        .mark_peer_gone(device_id, std::time::Duration::ZERO)
        .await
        .unwrap();
    assert_eq!(result, None);
    assert!(
        h.events.published_events().is_empty(),
        "no event when the device was not tracked as present"
    );
}

// -- Untrustworthy observations (issue #886) ---------------------------------

/// The daemon's own LAN IP, as configured in the test harness.
const OWN_LAN_IP: &str = "192.168.1.1";

/// Regression test for #886.
///
/// A powerline bridge proxy-ARP'd for the Pi's own address, so discovery
/// recorded `last_ip = <the Pi's IP>` against that MAC. Zone enforcement then
/// keyed on that address and blackholed the box to its own network. Discovery
/// is the first of the two guards: our own address is never a device.
#[tokio::test]
async fn observation_claiming_our_own_lan_ip_is_ignored() {
    let h = build_harness();
    let obs = sample_observation("aa:bb:cc:dd:ee:02", OWN_LAN_IP);

    let result = h.process_observation(&obs).await.unwrap();

    assert!(
        matches!(result, ObservationResult::Ignored),
        "an observation claiming the daemon's own LAN IP must be ignored (#886), \
         got {result:?}"
    );
    assert!(
        h.repo.inserted.lock().unwrap().is_empty(),
        "no device row may be created for the daemon's own IP (#886)"
    );
    assert!(
        h.events.published_events().is_empty(),
        "no discovery event may be published for the daemon's own IP (#886)"
    );
}

/// A device that already exists must not be *moved* onto our own address either
/// — the incident's device flapped onto the Pi's IP from a legitimate one.
#[tokio::test]
async fn known_device_flapping_onto_our_own_lan_ip_is_ignored() {
    let h = build_harness();
    let mac = "aa:bb:cc:dd:ee:03";
    h.process_observation(&sample_observation(mac, "192.168.1.40"))
        .await
        .unwrap();

    let result = h
        .process_observation(&sample_observation(mac, OWN_LAN_IP))
        .await
        .unwrap();

    assert!(
        matches!(result, ObservationResult::Ignored),
        "a known device claiming the daemon's own IP must be ignored (#886), got {result:?}"
    );
    let updates = h.repo.last_seen_updates.lock().unwrap();
    assert!(
        updates.iter().all(|(_, ip, _, _)| ip != OWN_LAN_IP),
        "last_ip must never be written as the daemon's own IP (#886): {updates:?}"
    );
}

/// Regression test for #886.
///
/// The MAC behind the incident claimed four different addresses within a single
/// second (`.4`, `.22`, `.2`, `.59`) — a powerline bridge answering ARP for
/// hosts across its segment, not a device changing address. A MAC cycling
/// through addresses that fast is not telling the truth about any of them, so
/// its observations stop being trusted until it settles.
#[tokio::test]
async fn mac_flapping_across_many_ips_stops_being_trusted() {
    let h = build_harness();
    let mac = "aa:bb:cc:dd:ee:04";

    // Establish the device, then flap it across distinct addresses.
    for ip in ["192.168.1.4", "192.168.1.22", "192.168.1.59"] {
        h.process_observation(&sample_observation(mac, ip))
            .await
            .unwrap();
    }

    // One address too many, inside the window: the MAC is now untrustworthy.
    let result = h
        .process_observation(&sample_observation(mac, "192.168.1.77"))
        .await
        .unwrap();

    assert!(
        matches!(result, ObservationResult::Ignored),
        "a MAC claiming more distinct IPs than the flap threshold inside the \
         window must stop being trusted (#886), got {result:?}"
    );
    let updates = h.repo.last_seen_updates.lock().unwrap();
    assert!(
        updates.iter().all(|(_, ip, _, _)| ip != "192.168.1.77"),
        "an untrusted flapping MAC must not rewrite last_ip (#886): {updates:?}"
    );
}

/// The guard must not punish a device that simply changed address once (a
/// normal DHCP move) — only one cycling through many.
#[tokio::test]
async fn a_device_changing_ip_normally_is_still_trusted() {
    let h = build_harness();
    let mac = "aa:bb:cc:dd:ee:05";
    h.process_observation(&sample_observation(mac, "192.168.1.50"))
        .await
        .unwrap();

    let result = h
        .process_observation(&sample_observation(mac, "192.168.1.51"))
        .await
        .unwrap();

    assert!(
        matches!(result, ObservationResult::IpChanged { .. }),
        "a single, ordinary IP change must still be honoured, got {result:?}"
    );
}

/// Code-review follow-up on #886: a claim of the Pi's own IP must still COUNT
/// toward the flap threshold. The incident MAC claimed three real addresses
/// plus the Pi's own inside one second; if the own-IP claim is dropped before
/// the flap history records it, the MAC stays forever one short of the
/// threshold and its lies about the three victim IPs keep being trusted.
#[tokio::test]
async fn own_ip_claims_count_toward_the_flap_threshold() {
    let h = build_harness();
    let mac = "aa:bb:cc:dd:ee:06";

    // The recorded #886 trace: .4, .22, <own IP>, .59 from one MAC.
    for ip in ["192.168.1.4", "192.168.1.22", OWN_LAN_IP] {
        h.process_observation(&sample_observation(mac, ip))
            .await
            .unwrap();
    }
    let result = h
        .process_observation(&sample_observation(mac, "192.168.1.59"))
        .await
        .unwrap();

    assert!(
        matches!(result, ObservationResult::Ignored),
        "the fourth distinct claim (own IP counted) must trip the flap \
         detector (#886), got {result:?}"
    );
    let updates = h.repo.last_seen_updates.lock().unwrap();
    assert!(
        updates.iter().all(|(_, ip, _, _)| ip != "192.168.1.59"),
        "the flapping MAC's claim on .59 must not be recorded (#886): {updates:?}"
    );
}

/// Code-review follow-up on #886: ignoring a flapping MAC's observations must
/// not let the departure sweep mark it gone — the claims lie about the address,
/// but they still prove the MAC is on the wire, and a gone-marking tears down
/// its zone enforcement fail-open via `DeviceGone` → `remove_device`.
#[tokio::test]
async fn flap_ignored_observations_still_count_as_presence() {
    let h = build_harness();
    let mac = "aa:bb:cc:dd:ee:07";

    // Register, then trip the flap threshold (4 distinct IPs).
    for ip in [
        "192.168.1.61",
        "192.168.1.62",
        "192.168.1.63",
        "192.168.1.64",
    ] {
        h.process_observation(&sample_observation(mac, ip))
            .await
            .unwrap();
    }

    // Let more than the departure timeout pass, then observe the (still
    // flapping) MAC again: the observation is Ignored but must refresh
    // presence.
    tokio::time::sleep(std::time::Duration::from_millis(1200)).await;
    let result = h
        .process_observation(&sample_observation(mac, "192.168.1.64"))
        .await
        .unwrap();
    assert!(matches!(result, ObservationResult::Ignored));

    let departed = h.scan_departures(1).await.unwrap();
    assert!(
        departed.is_empty(),
        "a flapping-but-present MAC must not be marked gone (#886): {departed:?}"
    );
}

/// Code-review follow-up on #886: a device row claiming the Pi's own IP that
/// survived in an older database is repaired at startup — its address is
/// cleared (loudly) rather than left to make every zone action on it a silent
/// no-op forever.
#[tokio::test]
async fn restore_devices_repairs_a_row_claiming_our_own_ip() {
    let poisoned = sample_device(
        "0a0a0a0a-0000-0000-0000-000000000001",
        "aa:bb:cc:dd:ee:08",
        OWN_LAN_IP,
    );
    let h = build_harness_with_devices(vec![poisoned]);

    h.restore_devices().await.unwrap();

    let updates = h.repo.last_seen_updates.lock().unwrap();
    assert!(
        updates
            .iter()
            .any(|(id, ip, _, _)| id == "0a0a0a0a-0000-0000-0000-000000000001" && ip.is_empty()),
        "the poisoned row's last_ip must be cleared at startup (#886): {updates:?}"
    );
}

// -- Auth guards ----------------------------------------------------------

/// Every admin-guarded background method rejects an anonymous caller with
/// `Forbidden` rather than silently doing its work. These run outside the HTTP
/// middleware, so a bug that let them execute unguarded would have no other
/// backstop.
#[tokio::test]
async fn guarded_methods_reject_anonymous_caller() {
    let h = build_harness();
    let obs = sample_observation("aa:bb:cc:dd:ee:01", "192.168.1.10");

    let anon = AuthContext::Anonymous;
    assert!(matches!(
        auth_context::with_context(anon.clone(), h.svc.restore_devices()).await,
        Err(AppError::Forbidden(_))
    ));
    assert!(matches!(
        auth_context::with_context(anon.clone(), h.svc.process_observation(&obs)).await,
        Err(AppError::Forbidden(_))
    ));
    assert!(matches!(
        auth_context::with_context(anon.clone(), h.svc.flush_last_seen()).await,
        Err(AppError::Forbidden(_))
    ));
    assert!(matches!(
        auth_context::with_context(anon.clone(), h.svc.scan_departures(0)).await,
        Err(AppError::Forbidden(_))
    ));
    assert!(matches!(
        auth_context::with_context(
            anon,
            h.svc.resolve_hostname("aa:bb:cc:dd:ee:01", "192.168.1.10")
        )
        .await,
        Err(AppError::Forbidden(_))
    ));
}

/// The nil-admin context the `device_detector` background tasks establish
/// reaches the service and is accepted, so the guard does not break the runner.
#[tokio::test]
async fn process_observation_accepts_nil_admin_context() {
    let h = build_harness();
    let obs = sample_observation("aa:bb:cc:dd:ee:01", "192.168.1.10");

    let nil_admin = AuthContext::Admin {
        admin_id: Uuid::nil(),
    };
    let result = auth_context::with_context(nil_admin, h.svc.process_observation(&obs)).await;
    assert!(
        matches!(result, Ok(ObservationResult::NewDevice { .. })),
        "nil-admin observation should register the device: {result:?}"
    );
}

/// A device whose IEEE listing is the placeholder `Private` is named from the
/// curated vendor catalog instead — the Govee lamp that motivated issue #1099.
#[tokio::test]
async fn new_device_falls_back_to_the_vendor_catalog_for_a_private_oui() {
    let h = build_harness();

    let obs = sample_observation("5c:e7:53:4e:ec:d9", "192.168.1.60");
    let result = h.process_observation(&obs).await.unwrap();

    match result {
        ObservationResult::NewDevice { manufacturer, .. } => {
            assert_eq!(
                manufacturer.as_deref(),
                Some("Govee"),
                "the IEEE table cannot name a `Private` block; the catalog must"
            );
        }
        other => panic!("expected NewDevice, got {other:?}"),
    }
}

/// A locally-administered (privacy) MAC has no OUI to look up, so it gets no
/// manufacturer — but it is still overwhelmingly a phone, an inference that
/// used to ride on the removed "Randomized MAC" sentinel (issue #1099).
#[tokio::test]
async fn new_device_with_a_randomized_mac_has_no_manufacturer_but_guesses_phone() {
    let h = build_harness();

    let obs = sample_observation("02:1a:2b:3c:4d:5e", "192.168.1.61");
    let result = h.process_observation(&obs).await.unwrap();

    match result {
        ObservationResult::NewDevice {
            manufacturer,
            device_type,
            ..
        } => {
            assert_eq!(manufacturer, None, "a privacy MAC must not name a vendor");
            assert_eq!(device_type, DeviceType::Phone);
        }
        other => panic!("expected NewDevice, got {other:?}"),
    }
}

/// An unregistered, universally-administered OUI stays genuinely unknown —
/// neither the IEEE table nor the catalog invents a name for it.
#[tokio::test]
async fn new_device_with_an_unknown_oui_stays_unnamed() {
    let h = build_harness();

    // B8-4C-87 is an MA-M/MA-S parent block listed to "IEEE Registration
    // Authority", which `build.rs` drops as a placeholder.
    let obs = sample_observation("b8:4c:87:00:11:22", "192.168.1.62");
    let result = h.process_observation(&obs).await.unwrap();

    match result {
        ObservationResult::NewDevice {
            manufacturer,
            device_type,
            ..
        } => {
            assert_eq!(manufacturer, None);
            assert_eq!(device_type, DeviceType::Unknown);
        }
        other => panic!("expected NewDevice, got {other:?}"),
    }
}

/// A publicly-registered OUI is named from the IEEE table and marked as the
/// authoritative source — the arm the catalog fallback defers to.
#[tokio::test]
async fn new_device_with_a_public_oui_is_named_from_the_ieee_table() {
    let h = build_harness();

    // F0-EE-7A is registered to Apple and has the locally-administered bit
    // clear, so it resolves through the IEEE table rather than the catalog.
    let obs = sample_observation("f0:ee:7a:00:11:22", "192.168.1.63");
    let result = h.process_observation(&obs).await.unwrap();

    match result {
        ObservationResult::NewDevice {
            manufacturer,
            device_type,
            ..
        } => {
            assert_eq!(manufacturer.as_deref(), Some("Apple, Inc."));
            assert_eq!(device_type, DeviceType::Phone);
        }
        other => panic!("expected NewDevice, got {other:?}"),
    }
}

// ── Retention prune (issue #1181) ────────────────────────────────────────────

const DEVICE_ID_1: &str = "00000000-0000-0000-0000-0000000f0001";
const DEVICE_ID_2: &str = "00000000-0000-0000-0000-0000000f0002";
const DEVICE_ID_3: &str = "00000000-0000-0000-0000-0000000f0003";
const MAC_1: &str = "aa:bb:cc:dd:ef:01";
const MAC_2: &str = "aa:bb:cc:dd:ef:02";
const MAC_3: &str = "aa:bb:cc:dd:ef:03";

/// A device last seen well outside the 30-day retention window.
fn stale_device(id: &str, mac: &str, ip: &str) -> Device {
    let mut device = sample_device(id, mac, ip);
    device.last_seen = chrono::Utc::now() - chrono::Duration::days(90);
    device
}

/// Push a device's `last_seen` back outside the retention window, undoing the
/// refresh an observation performs.
fn age_out(harness: &TestHarness, id: &str) {
    let mut devices = harness.repo.devices.lock().unwrap();
    let device = devices
        .iter_mut()
        .find(|d| d.id.to_string() == id)
        .expect("device present");
    device.last_seen = chrono::Utc::now() - chrono::Duration::days(90);
}

#[tokio::test]
async fn prune_requires_admin() {
    let harness =
        build_harness_with_devices(vec![stale_device(DEVICE_ID_1, MAC_1, "192.168.1.10")]);

    // Anonymous, and as the device itself: neither may trigger a prune.
    let result = harness.svc.prune_unmanaged_devices().await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));

    let result =
        auth_context::with_context(device_ctx(MAC_1), harness.svc.prune_unmanaged_devices()).await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));

    // And nothing was deleted while we were checking.
    assert_eq!(harness.repo.find_all().await.unwrap().len(), 1);
}

#[tokio::test]
async fn prune_deletes_stale_unmanaged_and_keeps_the_rest() {
    let mut managed_stale = stale_device(DEVICE_ID_2, MAC_2, "192.168.1.11");
    managed_stale.managed = true;

    let harness = build_harness_with_devices(vec![
        stale_device(DEVICE_ID_1, MAC_1, "192.168.1.10"),
        managed_stale,
        // Unmanaged but seen recently — `sample_device`'s own timestamp is
        // inside the window relative to nothing, so stamp it explicitly.
        {
            let mut d = sample_device(DEVICE_ID_3, MAC_3, "192.168.1.12");
            d.last_seen = chrono::Utc::now();
            d
        },
    ]);

    assert_eq!(harness.prune_unmanaged_devices().await.unwrap(), 1);

    let remaining: Vec<String> = harness
        .repo
        .find_all()
        .await
        .unwrap()
        .into_iter()
        .map(|d| d.id.to_string())
        .collect();
    assert!(!remaining.contains(&DEVICE_ID_1.to_owned()));
    assert!(
        remaining.contains(&DEVICE_ID_2.to_owned()),
        "managed is exempt at any age"
    );
    assert!(
        remaining.contains(&DEVICE_ID_3.to_owned()),
        "recent is exempt"
    );
}

/// The load-bearing test.
///
/// Deleting the row without evicting the MAC from `state` leaves the entry
/// behind with `gone = true`. The next observation then takes the `Reappear`
/// arm carrying a dangling `device_id`, `update_last_seen_and_ip` matches zero
/// rows and returns `Ok(())` **silently**, and `handle_unknown_mac` is never
/// reached — so the device is never re-inserted. It becomes invisible in the UI
/// while its traffic flows unattributed, until the daemon restarts.
///
/// Observing the MAC after a prune must therefore yield `NewDevice`, not
/// `Reappeared`.
#[tokio::test]
async fn prune_evicts_pruned_mac_from_memory_so_it_is_rediscovered_not_resurrected() {
    let harness =
        build_harness_with_devices(vec![stale_device(DEVICE_ID_1, MAC_1, "192.168.1.10")]);

    // Populate the in-memory maps the way a running daemon would: restore
    // marks every device gone, and an observation registers live tracking.
    harness.restore_devices().await.unwrap();
    harness
        .process_observation(&sample_observation(MAC_1, "192.168.1.10"))
        .await
        .unwrap();
    // The observation legitimately refreshed `last_seen`, so age the row again
    // — the point of this test is the eviction, not the predicate.
    age_out(&harness, DEVICE_ID_1);

    assert_eq!(harness.prune_unmanaged_devices().await.unwrap(), 1);

    let result = harness
        .process_observation(&sample_observation(MAC_1, "192.168.1.10"))
        .await
        .unwrap();

    assert!(
        matches!(result, ObservationResult::NewDevice { .. }),
        "a pruned MAC must be rediscovered as new, not resurrected as a ghost \
         pointing at a deleted row; got {result:?}"
    );

    // And the re-insert really happened, rather than the silent zero-row update.
    assert!(harness.repo.find_by_mac(MAC_1).await.unwrap().is_some());
}

#[tokio::test]
async fn prune_repo_error_propagates_as_internal_and_evicts_nothing() {
    let harness =
        build_harness_with_devices(vec![stale_device(DEVICE_ID_1, MAC_1, "192.168.1.10")]);
    harness.restore_devices().await.unwrap();
    harness
        .process_observation(&sample_observation(MAC_1, "192.168.1.10"))
        .await
        .unwrap();
    age_out(&harness, DEVICE_ID_1);

    harness
        .repo
        .fail_delete_unmanaged
        .store(true, std::sync::atomic::Ordering::SeqCst);

    let result = harness.prune_unmanaged_devices().await;
    assert!(matches!(result, Err(AppError::Internal(_))));

    // The row survived, so the in-memory entry must survive with it — evicting
    // on a failed delete would produce the mirror-image bug: a live device
    // re-inserted as a duplicate row under a MAC that already has one.
    harness
        .repo
        .fail_delete_unmanaged
        .store(false, std::sync::atomic::Ordering::SeqCst);
    let result = harness
        .process_observation(&sample_observation(MAC_1, "192.168.1.10"))
        .await
        .unwrap();
    assert!(
        matches!(
            result,
            ObservationResult::Seen(_) | ObservationResult::Reappeared(_)
        ),
        "the device is still tracked; got {result:?}"
    );
}

#[tokio::test]
async fn prune_with_nothing_to_delete_is_a_no_op() {
    let harness =
        build_harness_with_devices(vec![sample_device(DEVICE_ID_1, MAC_1, "192.168.1.10")]);
    // `sample_device`'s last_seen is a fixed past date, so pin it inside the
    // window explicitly rather than depending on when the suite runs.
    harness.repo.devices.lock().unwrap()[0].last_seen = chrono::Utc::now();

    assert_eq!(harness.prune_unmanaged_devices().await.unwrap(), 0);
    assert_eq!(harness.repo.find_all().await.unwrap().len(), 1);
}

// ── Managed promotion (issue #1181) ──────────────────────────────────────────

#[tokio::test]
async fn admin_rename_promotes_the_device_to_managed() {
    let device = sample_device(DEVICE_ID_1, MAC_1, "192.168.1.10");
    let id = device.id;
    let h = build_harness_with_devices(vec![device]);
    assert!(
        !h.repo
            .find_by_id(&id.to_string())
            .await
            .unwrap()
            .unwrap()
            .managed
    );

    auth_context::with_context(
        admin_ctx(),
        h.svc.update_device(id, Some("Living Room TV"), None),
    )
    .await
    .unwrap();

    assert!(
        h.repo
            .find_by_id(&id.to_string())
            .await
            .unwrap()
            .unwrap()
            .managed
    );
}

#[tokio::test]
async fn a_device_renaming_itself_does_not_promote_it() {
    // The exclusion the whole feature depends on. `update_device` is
    // `require_authenticated`, so a device may rename itself — and a guest
    // doing that is the device asking, not the admin deciding. Promoting here
    // would make every guest phone permanently exempt from retention.
    let device = sample_device(DEVICE_ID_1, MAC_1, "192.168.1.10");
    let id = device.id;
    let h = build_harness_with_devices(vec![device]);

    auth_context::with_context(
        device_ctx(MAC_1),
        h.svc.update_device(id, Some("My Phone"), None),
    )
    .await
    .unwrap();

    let stored = h.repo.find_by_id(&id.to_string()).await.unwrap().unwrap();
    assert_eq!(
        stored.name,
        Some("My Phone".to_owned()),
        "the rename lands..."
    );
    assert!(!stored.managed, "...but it does not promote");
}

#[tokio::test]
async fn a_blank_name_clears_the_name_to_null() {
    // An empty string would persist as a name that renders as no label while
    // still counting as named — the ambiguity `managed` exists to remove — and
    // the release handler needs a way to genuinely unname a device.
    let mut device = sample_device(DEVICE_ID_1, MAC_1, "192.168.1.10");
    device.name = Some("Living Room TV".to_owned());
    let id = device.id;
    let h = build_harness_with_devices(vec![device]);

    auth_context::with_context(admin_ctx(), h.svc.update_device(id, Some("   "), None))
        .await
        .unwrap();

    let stored = h.repo.find_by_id(&id.to_string()).await.unwrap().unwrap();
    assert_eq!(stored.name, None);
}

#[tokio::test]
async fn omitting_the_name_still_preserves_it() {
    // Guard against the blank-clears-it rule above swallowing the existing
    // partial-update contract: `None` means "leave the name alone", and is
    // distinct from `Some("")`.
    let mut device = sample_device(DEVICE_ID_1, MAC_1, "192.168.1.10");
    device.name = Some("Living Room TV".to_owned());
    let id = device.id;
    let h = build_harness_with_devices(vec![device]);

    auth_context::with_context(
        admin_ctx(),
        h.svc.update_device(id, None, Some(DeviceType::Tv)),
    )
    .await
    .unwrap();

    let stored = h.repo.find_by_id(&id.to_string()).await.unwrap().unwrap();
    assert_eq!(stored.name, Some("Living Room TV".to_owned()));
}
