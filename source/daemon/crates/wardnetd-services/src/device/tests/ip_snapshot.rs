use std::net::Ipv4Addr;
use std::sync::Arc;

use async_trait::async_trait;
use uuid::Uuid;
use wardnet_common::device::{Device, DeviceConnectionMode, DeviceType};
use wardnet_common::routing::RoutingRule;

use crate::device::ip_snapshot::DeviceIpSnapshot;
use wardnetd_data::repository::DeviceRepository;
use wardnetd_data::repository::device::DeviceRow;

// -- Mock repository ------------------------------------------------------

struct MockDeviceRepo {
    devices: Vec<Device>,
    /// When `true`, `find_all` returns an error instead of `devices`.
    fail: bool,
}

impl MockDeviceRepo {
    fn new(devices: Vec<Device>) -> Self {
        Self {
            devices,
            fail: false,
        }
    }

    fn failing() -> Self {
        Self {
            devices: Vec::new(),
            fail: true,
        }
    }
}

#[async_trait]
impl DeviceRepository for MockDeviceRepo {
    async fn find_by_ip(&self, _ip: &str) -> anyhow::Result<Option<Device>> {
        unimplemented!("not exercised by DeviceIpSnapshot")
    }
    async fn find_by_id(&self, _id: &str) -> anyhow::Result<Option<Device>> {
        unimplemented!("not exercised by DeviceIpSnapshot")
    }
    async fn find_by_mac(&self, _mac: &str) -> anyhow::Result<Option<Device>> {
        unimplemented!("not exercised by DeviceIpSnapshot")
    }
    async fn find_all(&self) -> anyhow::Result<Vec<Device>> {
        if self.fail {
            anyhow::bail!("mock: find_all failed");
        }
        Ok(self.devices.clone())
    }
    async fn insert(&self, _device: &DeviceRow) -> anyhow::Result<()> {
        unimplemented!("not exercised by DeviceIpSnapshot")
    }
    async fn clear_last_ip(&self, _id: &str) -> anyhow::Result<()> {
        unimplemented!("not exercised by DeviceIpSnapshot")
    }
    async fn update_last_seen_and_ip(
        &self,
        _id: &str,
        _ip: &str,
        _last_seen: &str,
        _mode: DeviceConnectionMode,
    ) -> anyhow::Result<()> {
        unimplemented!("not exercised by DeviceIpSnapshot")
    }
    async fn update_last_seen_batch(&self, _updates: &[(String, String)]) -> anyhow::Result<()> {
        unimplemented!("not exercised by DeviceIpSnapshot")
    }
    async fn update_connection_mode(
        &self,
        _id: &str,
        _mode: DeviceConnectionMode,
    ) -> anyhow::Result<()> {
        unimplemented!("not exercised by DeviceIpSnapshot")
    }
    async fn update_hostname(&self, _id: &str, _hostname: &str) -> anyhow::Result<()> {
        unimplemented!("not exercised by DeviceIpSnapshot")
    }
    async fn update_name_and_type(
        &self,
        _id: &str,
        _name: Option<&str>,
        _device_type: &str,
    ) -> anyhow::Result<()> {
        unimplemented!("not exercised by DeviceIpSnapshot")
    }
    async fn find_stale(&self, _before: &str) -> anyhow::Result<Vec<Device>> {
        unimplemented!("not exercised by DeviceIpSnapshot")
    }
    async fn find_rule_for_device(&self, _device_id: &str) -> anyhow::Result<Option<RoutingRule>> {
        unimplemented!("not exercised by DeviceIpSnapshot")
    }
    async fn find_all_rules(&self) -> anyhow::Result<Vec<RoutingRule>> {
        unimplemented!("not exercised by DeviceIpSnapshot")
    }
    async fn upsert_user_rule(
        &self,
        _device_id: &str,
        _target_json: &str,
        _now: &str,
    ) -> anyhow::Result<()> {
        unimplemented!("not exercised by DeviceIpSnapshot")
    }
    async fn find_devices_for_tunnel(&self, _tunnel_id: &str) -> anyhow::Result<Vec<Device>> {
        unimplemented!("not exercised by DeviceIpSnapshot")
    }
    async fn switch_tunnel_rules_to_direct(
        &self,
        _tunnel_id: &str,
        _now: &str,
    ) -> anyhow::Result<Vec<String>> {
        unimplemented!("not exercised by DeviceIpSnapshot")
    }
    async fn update_admin_locked(&self, _id: &str, _locked: bool) -> anyhow::Result<()> {
        unimplemented!("not exercised by DeviceIpSnapshot")
    }
    async fn assign_zone(&self, _device_id: &str, _zone_id: &str) -> anyhow::Result<bool> {
        unimplemented!("not exercised by DeviceIpSnapshot")
    }
    async fn count(&self) -> anyhow::Result<i64> {
        unimplemented!("not exercised by DeviceIpSnapshot")
    }
    async fn update_dns_capture_settings(
        &self,
        _id: &str,
        _enabled: Option<bool>,
        _cap_count: Option<i64>,
        _cap_days: Option<i64>,
    ) -> anyhow::Result<bool> {
        unimplemented!("not exercised by DeviceIpSnapshot")
    }
    async fn find_all_capture_enabled_ids(&self) -> anyhow::Result<Vec<String>> {
        unimplemented!("not exercised by DeviceIpSnapshot")
    }
}

// -- Helpers ----------------------------------------------------------------

fn sample_device(id: &str, ip: &str, last_seen: &str) -> Device {
    Device {
        id: Uuid::parse_str(id).unwrap(),
        mac: "AA:BB:CC:DD:EE:01".to_owned(),
        name: None,
        hostname: None,
        manufacturer: None,
        manufacturer_source: None,
        is_randomized: false,
        device_type: DeviceType::Phone,
        first_seen: last_seen.parse().unwrap(),
        last_seen: last_seen.parse().unwrap(),
        last_ip: ip.to_owned(),
        admin_locked: false,
        zone_id: "00000000-0000-0000-0000-000000000201".parse().unwrap(),
        dns_capture_enabled: false,
        dns_capture_cap_count: 1000,
        dns_capture_cap_days: 7,
        connection_mode: DeviceConnectionMode::Lan,
    }
}

// -- Tests --------------------------------------------------------------

#[tokio::test]
async fn new_snapshot_is_empty_before_rebuild() {
    let repo = Arc::new(MockDeviceRepo::new(vec![]));
    let snapshot = DeviceIpSnapshot::new(repo);

    assert!(snapshot.snapshot().load().is_empty());
}

#[tokio::test]
async fn rebuild_populates_snapshot_from_devices() {
    let device_id = Uuid::new_v4();
    let repo = Arc::new(MockDeviceRepo::new(vec![sample_device(
        &device_id.to_string(),
        "192.168.1.10",
        "2026-03-07T00:00:00Z",
    )]));
    let snapshot = DeviceIpSnapshot::new(repo);

    snapshot.rebuild().await.unwrap();

    let map = snapshot.snapshot().load_full();
    assert_eq!(map.len(), 1);
    assert_eq!(
        map.get(&std::net::IpAddr::V4(
            "192.168.1.10".parse::<Ipv4Addr>().unwrap()
        )),
        Some(&device_id)
    );
}

#[tokio::test]
async fn rebuild_skips_devices_with_empty_last_ip() {
    let repo = Arc::new(MockDeviceRepo::new(vec![sample_device(
        &Uuid::new_v4().to_string(),
        "",
        "2026-03-07T00:00:00Z",
    )]));
    let snapshot = DeviceIpSnapshot::new(repo);

    snapshot.rebuild().await.unwrap();

    assert!(snapshot.snapshot().load().is_empty());
}

#[tokio::test]
async fn rebuild_skips_devices_with_unparsable_last_ip() {
    let repo = Arc::new(MockDeviceRepo::new(vec![sample_device(
        &Uuid::new_v4().to_string(),
        "not-an-ip",
        "2026-03-07T00:00:00Z",
    )]));
    let snapshot = DeviceIpSnapshot::new(repo);

    snapshot.rebuild().await.unwrap();

    assert!(snapshot.snapshot().load().is_empty());
}

#[tokio::test]
async fn rebuild_resolves_ip_collision_in_favor_of_most_recently_seen() {
    // Two devices transiently claim the same IP (old device not yet re-seen
    // after DHCP handed its address to a new one). The most recently seen
    // claimant must win.
    let stale_id = Uuid::new_v4();
    let fresh_id = Uuid::new_v4();
    let repo = Arc::new(MockDeviceRepo::new(vec![
        sample_device(
            &stale_id.to_string(),
            "192.168.1.50",
            "2026-03-07T00:00:00Z",
        ),
        sample_device(
            &fresh_id.to_string(),
            "192.168.1.50",
            "2026-03-08T00:00:00Z",
        ),
    ]));
    let snapshot = DeviceIpSnapshot::new(repo);

    snapshot.rebuild().await.unwrap();

    let map = snapshot.snapshot().load_full();
    assert_eq!(map.len(), 1);
    assert_eq!(
        map.get(&std::net::IpAddr::V4(
            "192.168.1.50".parse::<Ipv4Addr>().unwrap()
        )),
        Some(&fresh_id)
    );
}

#[tokio::test]
async fn rebuild_propagates_repository_error() {
    let repo = Arc::new(MockDeviceRepo::failing());
    let snapshot = DeviceIpSnapshot::new(repo);

    assert!(snapshot.rebuild().await.is_err());
}

#[tokio::test]
async fn snapshot_returns_shared_handle_updated_by_rebuild() {
    let device_id = Uuid::new_v4();
    let repo = Arc::new(MockDeviceRepo::new(vec![sample_device(
        &device_id.to_string(),
        "10.0.0.5",
        "2026-03-07T00:00:00Z",
    )]));
    let snapshot = DeviceIpSnapshot::new(repo);

    // Hold a handle taken before the rebuild — it must observe the swap.
    let handle = snapshot.snapshot();
    assert!(handle.load().is_empty());

    snapshot.rebuild().await.unwrap();

    assert_eq!(handle.load().len(), 1);
}
