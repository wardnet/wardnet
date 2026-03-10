use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use tokio::sync::broadcast;
use uuid::Uuid;
use wardnet_types::device::{Device, DeviceType};
use wardnet_types::event::WardnetEvent;
use wardnet_types::routing::RoutingRule;

use wardnet_types::auth::AuthContext;

use crate::auth_context;
use crate::error::AppError;
use crate::event::EventPublisher;
use crate::hostname_resolver::HostnameResolver;
use crate::packet_capture::{ObservedDevice, PacketSource};
use crate::repository::DeviceRepository;
use crate::repository::device::DeviceRow;
use crate::service::DeviceDiscoveryService;
use crate::service::discovery::{DeviceDiscoveryServiceImpl, ObservationResult};

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
    last_seen_updates: Mutex<Vec<(String, String, String)>>,
    batch_updates: Mutex<Vec<(String, String)>>,
    hostname_updates: Mutex<Vec<(String, String)>>,
    name_type_updates: Mutex<Vec<(String, Option<String>, String)>>,
}

impl MockDeviceRepo {
    fn with_devices(devices: Vec<Device>) -> Self {
        Self {
            devices: Mutex::new(devices),
            inserted: Mutex::new(Vec::new()),
            last_seen_updates: Mutex::new(Vec::new()),
            batch_updates: Mutex::new(Vec::new()),
            hostname_updates: Mutex::new(Vec::new()),
            name_type_updates: Mutex::new(Vec::new()),
        }
    }
}

#[async_trait]
impl DeviceRepository for MockDeviceRepo {
    async fn find_by_ip(&self, ip: &str) -> anyhow::Result<Option<Device>> {
        let devices = self.devices.lock().unwrap();
        Ok(devices.iter().find(|d| d.last_ip == ip).cloned())
    }

    async fn find_by_id(&self, id: &str) -> anyhow::Result<Option<Device>> {
        let devices = self.devices.lock().unwrap();
        Ok(devices.iter().find(|d| d.id.to_string() == id).cloned())
    }

    async fn find_by_mac(&self, mac: &str) -> anyhow::Result<Option<Device>> {
        let devices = self.devices.lock().unwrap();
        Ok(devices.iter().find(|d| d.mac == mac).cloned())
    }

    async fn find_all(&self) -> anyhow::Result<Vec<Device>> {
        Ok(self.devices.lock().unwrap().clone())
    }

    async fn insert(&self, device: &DeviceRow) -> anyhow::Result<()> {
        let mut inserted = self.inserted.lock().unwrap();
        inserted.push(DeviceRow {
            id: device.id.clone(),
            mac: device.mac.clone(),
            hostname: device.hostname.clone(),
            manufacturer: device.manufacturer.clone(),
            device_type: device.device_type.clone(),
            first_seen: device.first_seen.clone(),
            last_seen: device.last_seen.clone(),
            last_ip: device.last_ip.clone(),
        });

        // Also add to the devices list so subsequent find_by_id works.
        let mut devices = self.devices.lock().unwrap();
        devices.push(Device {
            id: device.id.parse().unwrap(),
            mac: device.mac.clone(),
            name: None,
            hostname: device.hostname.clone(),
            manufacturer: device.manufacturer.clone(),
            device_type: serde_json::from_str(&format!("\"{}\"", device.device_type))
                .unwrap_or(DeviceType::Unknown),
            first_seen: device.first_seen.parse().unwrap(),
            last_seen: device.last_seen.parse().unwrap(),
            last_ip: device.last_ip.clone(),
            admin_locked: false,
        });
        Ok(())
    }

    async fn update_last_seen_and_ip(
        &self,
        id: &str,
        ip: &str,
        last_seen: &str,
    ) -> anyhow::Result<()> {
        self.last_seen_updates.lock().unwrap().push((
            id.to_owned(),
            ip.to_owned(),
            last_seen.to_owned(),
        ));
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
            if let Some(n) = name {
                d.name = Some(n.to_owned());
            }
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

    async fn count(&self) -> anyhow::Result<i64> {
        Ok(i64::try_from(self.devices.lock().unwrap().len()).unwrap_or(0))
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

// -- Helpers --------------------------------------------------------------

fn sample_device(id: &str, mac: &str, ip: &str) -> Device {
    Device {
        id: Uuid::parse_str(id).unwrap(),
        mac: mac.to_owned(),
        name: None,
        hostname: None,
        manufacturer: Some("Apple".to_owned()),
        device_type: DeviceType::Phone,
        first_seen: "2026-03-07T00:00:00Z".parse().unwrap(),
        last_seen: "2026-03-07T00:00:00Z".parse().unwrap(),
        last_ip: ip.to_owned(),
        admin_locked: false,
    }
}

fn sample_observation(mac: &str, ip: &str) -> ObservedDevice {
    ObservedDevice {
        mac: mac.to_owned(),
        ip: ip.to_owned(),
        source: PacketSource::Arp,
    }
}

struct TestHarness {
    svc: DeviceDiscoveryServiceImpl,
    repo: Arc<MockDeviceRepo>,
    events: Arc<MockEventPublisher>,
}

fn build_harness() -> TestHarness {
    build_harness_with_devices(Vec::new())
}

fn build_harness_with_devices(devices: Vec<Device>) -> TestHarness {
    let repo = Arc::new(MockDeviceRepo::with_devices(devices));
    let events = Arc::new(MockEventPublisher::new());
    let resolver = Arc::new(MockHostnameResolver::new());
    let svc = DeviceDiscoveryServiceImpl::new(repo.clone(), events.clone(), resolver);

    TestHarness { svc, repo, events }
}

fn build_harness_with_resolver(
    devices: Vec<Device>,
    resolver_mappings: HashMap<String, String>,
) -> TestHarness {
    let repo = Arc::new(MockDeviceRepo::with_devices(devices));
    let events = Arc::new(MockEventPublisher::new());
    let resolver = Arc::new(MockHostnameResolver::with_mappings(resolver_mappings));
    let svc = DeviceDiscoveryServiceImpl::new(repo.clone(), events.clone(), resolver);

    TestHarness { svc, repo, events }
}

// -- Tests ----------------------------------------------------------------

#[tokio::test]
async fn process_observation_new_device() {
    let h = build_harness();
    let obs = sample_observation("AA:BB:CC:DD:EE:01", "192.168.1.10");

    let result = h.svc.process_observation(&obs).await.unwrap();
    assert!(
        matches!(result, ObservationResult::NewDevice { .. }),
        "expected NewDevice, got {result:?}"
    );

    // Verify device was inserted.
    let inserted = h.repo.inserted.lock().unwrap();
    assert_eq!(inserted.len(), 1);
    assert_eq!(inserted[0].mac, "AA:BB:CC:DD:EE:01");
    assert_eq!(inserted[0].last_ip, "192.168.1.10");

    // Verify DeviceDiscovered event was emitted.
    let events = h.events.published_events();
    assert_eq!(events.len(), 1);
    assert!(matches!(
        &events[0],
        WardnetEvent::DeviceDiscovered {
            mac, ip, ..
        } if mac == "AA:BB:CC:DD:EE:01" && ip == "192.168.1.10"
    ));
}

#[tokio::test]
async fn process_observation_known_device_same_ip() {
    let h = build_harness();
    let obs = sample_observation("AA:BB:CC:DD:EE:01", "192.168.1.10");

    // First observation registers the device.
    h.svc.process_observation(&obs).await.unwrap();

    // Clear events from the first observation.
    h.events.events.lock().unwrap().clear();

    // Second observation with same MAC and IP.
    let result = h.svc.process_observation(&obs).await.unwrap();
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
    let obs1 = sample_observation("AA:BB:CC:DD:EE:01", "192.168.1.10");
    h.svc.process_observation(&obs1).await.unwrap();

    // Clear events.
    h.events.events.lock().unwrap().clear();

    // Second observation with same MAC but different IP.
    let obs2 = sample_observation("AA:BB:CC:DD:EE:01", "192.168.1.20");
    let result = h.svc.process_observation(&obs2).await.unwrap();
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
    let obs = sample_observation("AA:BB:CC:DD:EE:01", "192.168.1.10");

    // Register device.
    h.svc.process_observation(&obs).await.unwrap();

    // Mark it as departed (use a tiny timeout so it departs immediately).
    // We need to wait a small amount of time for the Instant to advance.
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    let departed = h.svc.scan_departures(0).await.unwrap();
    assert_eq!(departed.len(), 1);

    // Clear events.
    h.events.events.lock().unwrap().clear();

    // Device reappears.
    let result = h.svc.process_observation(&obs).await.unwrap();
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
    let obs = sample_observation("AA:BB:CC:DD:EE:01", "192.168.1.10");
    h.svc.process_observation(&obs).await.unwrap();

    let count = h.svc.flush_last_seen().await.unwrap();
    assert_eq!(count, 1);

    let batch = h.repo.batch_updates.lock().unwrap();
    assert_eq!(batch.len(), 1);
}

#[tokio::test]
async fn scan_departures_emits_device_gone() {
    let h = build_harness();
    let obs = sample_observation("AA:BB:CC:DD:EE:01", "192.168.1.10");
    h.svc.process_observation(&obs).await.unwrap();

    // Clear events from registration.
    h.events.events.lock().unwrap().clear();

    // Wait briefly and scan with a zero timeout to force departure.
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    let departed = h.svc.scan_departures(0).await.unwrap();
    assert_eq!(departed.len(), 1);

    // Verify DeviceGone event was emitted.
    let events = h.events.published_events();
    assert_eq!(events.len(), 1);
    assert!(matches!(
        &events[0],
        WardnetEvent::DeviceGone {
            mac, last_ip, ..
        } if mac == "AA:BB:CC:DD:EE:01" && last_ip == "192.168.1.10"
    ));
}

#[tokio::test]
async fn scan_departures_ignores_recent() {
    let h = build_harness();
    let obs = sample_observation("AA:BB:CC:DD:EE:01", "192.168.1.10");
    h.svc.process_observation(&obs).await.unwrap();

    // Clear events.
    h.events.events.lock().unwrap().clear();

    // Scan with a very large timeout -- device should not be departed.
    let departed = h.svc.scan_departures(3600).await.unwrap();
    assert!(departed.is_empty());

    // No events emitted.
    let events = h.events.published_events();
    assert!(events.is_empty());
}

#[tokio::test]
async fn get_all_devices_as_admin() {
    let device = sample_device(
        "00000000-0000-0000-0000-000000000001",
        "AA:BB:CC:DD:EE:01",
        "192.168.1.10",
    );
    let h = build_harness_with_devices(vec![device]);

    let devices = auth_context::with_context(admin_ctx(), h.svc.get_all_devices())
        .await
        .unwrap();
    assert_eq!(devices.len(), 1);
    assert_eq!(devices[0].mac, "AA:BB:CC:DD:EE:01");
}

#[tokio::test]
async fn get_all_devices_as_device_returns_own_only() {
    let device1 = sample_device(
        "00000000-0000-0000-0000-000000000001",
        "AA:BB:CC:DD:EE:01",
        "192.168.1.10",
    );
    let device2 = sample_device(
        "00000000-0000-0000-0000-000000000002",
        "AA:BB:CC:DD:EE:02",
        "192.168.1.11",
    );
    let h = build_harness_with_devices(vec![device1, device2]);

    let devices =
        auth_context::with_context(device_ctx("AA:BB:CC:DD:EE:01"), h.svc.get_all_devices())
            .await
            .unwrap();
    assert_eq!(devices.len(), 1);
    assert_eq!(devices[0].mac, "AA:BB:CC:DD:EE:01");
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
        "AA:BB:CC:DD:EE:01",
        "192.168.1.10",
    );
    let id = device.id;
    let h = build_harness_with_devices(vec![device]);

    let result =
        auth_context::with_context(device_ctx("AA:BB:CC:DD:EE:01"), h.svc.get_device_by_id(id))
            .await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap().mac, "AA:BB:CC:DD:EE:01");
}

#[tokio::test]
async fn get_device_by_id_device_cannot_view_other() {
    let device = sample_device(
        "00000000-0000-0000-0000-000000000001",
        "AA:BB:CC:DD:EE:01",
        "192.168.1.10",
    );
    let id = device.id;
    let h = build_harness_with_devices(vec![device]);

    let result =
        auth_context::with_context(device_ctx("AA:BB:CC:DD:EE:99"), h.svc.get_device_by_id(id))
            .await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}

#[tokio::test]
async fn update_device_sets_name_and_type_as_admin() {
    let device = sample_device(
        "00000000-0000-0000-0000-000000000001",
        "AA:BB:CC:DD:EE:01",
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
async fn update_device_anonymous_forbidden() {
    let device = sample_device(
        "00000000-0000-0000-0000-000000000001",
        "AA:BB:CC:DD:EE:01",
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
        "AA:BB:CC:DD:EE:01",
        "192.168.1.10",
    );
    let id = device.id;
    let h = build_harness_with_devices(vec![device]);

    let result = auth_context::with_context(
        device_ctx("AA:BB:CC:DD:EE:01"),
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
        "AA:BB:CC:DD:EE:01",
        "192.168.1.10",
    );
    let id = device.id;
    let h = build_harness_with_devices(vec![device]);

    let result = auth_context::with_context(
        device_ctx("AA:BB:CC:DD:EE:99"),
        h.svc.update_device(id, Some("name"), None),
    )
    .await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}

#[tokio::test]
async fn update_device_as_device_admin_locked_forbidden() {
    let mut device = sample_device(
        "00000000-0000-0000-0000-000000000001",
        "AA:BB:CC:DD:EE:01",
        "192.168.1.10",
    );
    device.admin_locked = true;
    let id = device.id;
    let h = build_harness_with_devices(vec![device]);

    let result = auth_context::with_context(
        device_ctx("AA:BB:CC:DD:EE:01"),
        h.svc.update_device(id, Some("name"), None),
    )
    .await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}

#[tokio::test]
async fn resolve_hostname_updates_db() {
    let device = sample_device(
        "00000000-0000-0000-0000-000000000001",
        "AA:BB:CC:DD:EE:01",
        "192.168.1.10",
    );
    let id = device.id;
    let mut mappings = HashMap::new();
    mappings.insert("192.168.1.10".to_owned(), "myphone.local".to_owned());
    let h = build_harness_with_resolver(vec![device], mappings);

    h.svc
        .resolve_hostname(id, "192.168.1.10".to_owned())
        .await
        .unwrap();

    let updates = h.repo.hostname_updates.lock().unwrap();
    assert_eq!(updates.len(), 1);
    assert_eq!(updates[0].1, "myphone.local");
}

#[tokio::test]
async fn resolve_hostname_no_result_does_not_update_db() {
    let device = sample_device(
        "00000000-0000-0000-0000-000000000001",
        "AA:BB:CC:DD:EE:01",
        "192.168.1.10",
    );
    let id = device.id;
    // Empty resolver: no mappings, resolve returns None.
    let h = build_harness_with_resolver(vec![device], HashMap::new());

    h.svc
        .resolve_hostname(id, "192.168.1.10".to_owned())
        .await
        .unwrap();

    let updates = h.repo.hostname_updates.lock().unwrap();
    assert!(
        updates.is_empty(),
        "no hostname update when resolver returns None"
    );
}

#[tokio::test]
async fn restore_devices_populates_in_memory_state() {
    let device = sample_device(
        "00000000-0000-0000-0000-000000000001",
        "AA:BB:CC:DD:EE:01",
        "192.168.1.10",
    );
    let h = build_harness_with_devices(vec![device]);

    h.svc.restore_devices().await.unwrap();

    // After restore, processing the same MAC should not create a new device
    // because restore marks devices as gone; re-observing should reappear them.
    let obs = sample_observation("AA:BB:CC:DD:EE:01", "192.168.1.10");
    let result = h.svc.process_observation(&obs).await.unwrap();
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
    h.svc.restore_devices().await.unwrap();
}

#[tokio::test]
async fn update_device_without_device_type_preserves_existing() {
    let device = sample_device(
        "00000000-0000-0000-0000-000000000001",
        "AA:BB:CC:DD:EE:01",
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
        "AA:BB:CC:DD:EE:01",
        "192.168.1.10",
    );
    let id = device.id;
    let h = build_harness_with_devices(vec![device]);

    let result = auth_context::with_context(admin_ctx(), h.svc.get_device_by_id(id))
        .await
        .unwrap();
    assert_eq!(result.id, id);
    assert_eq!(result.mac, "AA:BB:CC:DD:EE:01");
}

#[tokio::test]
async fn flush_last_seen_with_gone_devices_skips_them() {
    let h = build_harness();
    let obs = sample_observation("AA:BB:CC:DD:EE:01", "192.168.1.10");
    h.svc.process_observation(&obs).await.unwrap();

    // Mark device as departed.
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    h.svc.scan_departures(0).await.unwrap();

    // Flush should skip gone devices.
    let count = h.svc.flush_last_seen().await.unwrap();
    assert_eq!(count, 0, "gone devices should not be flushed");
}

#[tokio::test]
async fn process_observation_reappear_from_db_not_memory() {
    // Device exists in DB (from a previous daemon run) but not in memory.
    let device = sample_device(
        "00000000-0000-0000-0000-000000000001",
        "AA:BB:CC:DD:EE:01",
        "192.168.1.10",
    );
    let h = build_harness_with_devices(vec![device]);

    // Do NOT call restore_devices, so the in-memory map is empty.
    // Observing should find the device in the DB and reappear it.
    let obs = sample_observation("AA:BB:CC:DD:EE:01", "192.168.1.20");
    let result = h.svc.process_observation(&obs).await.unwrap();
    assert!(
        matches!(result, ObservationResult::Reappeared(_)),
        "expected Reappeared from DB, got {result:?}"
    );

    // Verify a DeviceDiscovered event was emitted.
    let events = h.events.published_events();
    assert_eq!(events.len(), 1);
    assert!(matches!(&events[0], WardnetEvent::DeviceDiscovered { .. }));
}
