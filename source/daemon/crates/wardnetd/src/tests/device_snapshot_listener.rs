use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use chrono::Utc;
use uuid::Uuid;
use wardnet_common::device::{Device, DeviceConnectionMode};
use wardnet_common::event::WardnetEvent;
use wardnet_common::routing::RoutingRule;

use wardnetd_data::repository::DeviceRepository;
use wardnetd_data::repository::device::DeviceRow;
use wardnetd_services::device::DeviceIpSnapshot;
use wardnetd_services::event::{BroadcastEventBus, EventPublisher};

use crate::device_snapshot_listener::DeviceSnapshotListener;

// ---------------------------------------------------------------------------
// MockDeviceRepo
// ---------------------------------------------------------------------------

/// Counts `find_all` calls so tests can assert whether a rebuild happened,
/// without needing real device fixtures.
struct MockDeviceRepo {
    find_all_calls: AtomicUsize,
    /// When `true`, `find_all` returns an error instead of an empty list.
    fail: bool,
}

impl MockDeviceRepo {
    fn new() -> Self {
        Self {
            find_all_calls: AtomicUsize::new(0),
            fail: false,
        }
    }

    fn failing() -> Self {
        Self {
            find_all_calls: AtomicUsize::new(0),
            fail: true,
        }
    }

    fn call_count(&self) -> usize {
        self.find_all_calls.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl DeviceRepository for MockDeviceRepo {
    async fn delete_rule_for_device(&self, _device_id: &str) -> anyhow::Result<()> {
        Ok(())
    }

    async fn set_managed(&self, _id: &str, _managed: bool) -> anyhow::Result<()> {
        Ok(())
    }

    async fn delete_unmanaged_before(
        &self,
        _cutoff: &str,
    ) -> anyhow::Result<Vec<wardnetd_data::repository::PrunedDevice>> {
        Ok(Vec::new())
    }

    async fn set_owner(
        &self,
        _device_id: &str,
        _owner_user_id: Option<&str>,
    ) -> anyhow::Result<bool> {
        Ok(true)
    }

    async fn find_by_ip(&self, _ip: &str) -> anyhow::Result<Option<Device>> {
        unimplemented!("not exercised by DeviceSnapshotListener")
    }
    async fn find_by_id(&self, _id: &str) -> anyhow::Result<Option<Device>> {
        unimplemented!("not exercised by DeviceSnapshotListener")
    }
    async fn find_by_mac(&self, _mac: &str) -> anyhow::Result<Option<Device>> {
        unimplemented!("not exercised by DeviceSnapshotListener")
    }
    async fn find_all(&self) -> anyhow::Result<Vec<Device>> {
        self.find_all_calls.fetch_add(1, Ordering::SeqCst);
        if self.fail {
            anyhow::bail!("mock: find_all failed");
        }
        Ok(Vec::new())
    }
    async fn insert(&self, _device: &DeviceRow) -> anyhow::Result<()> {
        unimplemented!("not exercised by DeviceSnapshotListener")
    }
    async fn clear_last_ip(&self, _id: &str) -> anyhow::Result<()> {
        unimplemented!("not exercised by DeviceSnapshotListener")
    }
    async fn update_last_seen_and_ip(
        &self,
        _id: &str,
        _ip: &str,
        _last_seen: &str,
        _mode: DeviceConnectionMode,
    ) -> anyhow::Result<()> {
        unimplemented!("not exercised by DeviceSnapshotListener")
    }
    async fn update_last_seen_batch(&self, _updates: &[(String, String)]) -> anyhow::Result<()> {
        unimplemented!("not exercised by DeviceSnapshotListener")
    }
    async fn update_connection_mode(
        &self,
        _id: &str,
        _mode: DeviceConnectionMode,
    ) -> anyhow::Result<()> {
        unimplemented!("not exercised by DeviceSnapshotListener")
    }
    async fn update_hostname(&self, _id: &str, _hostname: &str) -> anyhow::Result<()> {
        unimplemented!("not exercised by DeviceSnapshotListener")
    }
    async fn update_name_and_type(
        &self,
        _id: &str,
        _name: Option<&str>,
        _device_type: &str,
    ) -> anyhow::Result<()> {
        unimplemented!("not exercised by DeviceSnapshotListener")
    }
    async fn find_stale(&self, _before: &str) -> anyhow::Result<Vec<Device>> {
        unimplemented!("not exercised by DeviceSnapshotListener")
    }
    async fn find_rule_for_device(&self, _device_id: &str) -> anyhow::Result<Option<RoutingRule>> {
        unimplemented!("not exercised by DeviceSnapshotListener")
    }
    async fn find_all_rules(&self) -> anyhow::Result<Vec<RoutingRule>> {
        unimplemented!("not exercised by DeviceSnapshotListener")
    }
    async fn upsert_user_rule(
        &self,
        _device_id: &str,
        _target_json: &str,
        _now: &str,
    ) -> anyhow::Result<()> {
        unimplemented!("not exercised by DeviceSnapshotListener")
    }
    async fn find_devices_for_tunnel(&self, _tunnel_id: &str) -> anyhow::Result<Vec<Device>> {
        unimplemented!("not exercised by DeviceSnapshotListener")
    }
    async fn switch_tunnel_rules_to_direct(
        &self,
        _tunnel_id: &str,
        _now: &str,
    ) -> anyhow::Result<Vec<String>> {
        unimplemented!("not exercised by DeviceSnapshotListener")
    }
    async fn update_admin_locked(&self, _id: &str, _locked: bool) -> anyhow::Result<()> {
        unimplemented!("not exercised by DeviceSnapshotListener")
    }
    async fn assign_zone(&self, _device_id: &str, _zone_id: &str) -> anyhow::Result<bool> {
        unimplemented!("not exercised by DeviceSnapshotListener")
    }
    async fn count(&self) -> anyhow::Result<i64> {
        unimplemented!("not exercised by DeviceSnapshotListener")
    }
    async fn update_dns_capture_settings(
        &self,
        _id: &str,
        _enabled: Option<bool>,
        _cap_count: Option<i64>,
        _cap_days: Option<i64>,
    ) -> anyhow::Result<bool> {
        unimplemented!("not exercised by DeviceSnapshotListener")
    }
    async fn find_all_capture_enabled_ids(&self) -> anyhow::Result<Vec<String>> {
        unimplemented!("not exercised by DeviceSnapshotListener")
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn device_discovered_triggers_rebuild() {
    let bus: Arc<dyn EventPublisher> = Arc::new(BroadcastEventBus::new(16));
    let repo = Arc::new(MockDeviceRepo::new());
    let snapshot = Arc::new(DeviceIpSnapshot::new(repo.clone()));

    let parent = tracing::info_span!("test");
    let listener = DeviceSnapshotListener::start(&bus, snapshot, &parent);

    bus.publish(WardnetEvent::DeviceDiscovered {
        device_id: Uuid::new_v4(),
        mac: "AA:BB:CC:DD:EE:01".to_owned(),
        ip: "192.168.1.10".to_owned(),
        hostname: None,
        timestamp: Utc::now(),
    });

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    listener.shutdown().await;

    assert_eq!(repo.call_count(), 1);
}

#[tokio::test]
async fn device_ip_changed_triggers_rebuild() {
    let bus: Arc<dyn EventPublisher> = Arc::new(BroadcastEventBus::new(16));
    let repo = Arc::new(MockDeviceRepo::new());
    let snapshot = Arc::new(DeviceIpSnapshot::new(repo.clone()));

    let parent = tracing::info_span!("test");
    let listener = DeviceSnapshotListener::start(&bus, snapshot, &parent);

    bus.publish(WardnetEvent::DeviceIpChanged {
        device_id: Uuid::new_v4(),
        mac: "AA:BB:CC:DD:EE:01".to_owned(),
        old_ip: "192.168.1.10".to_owned(),
        new_ip: "192.168.1.20".to_owned(),
        timestamp: Utc::now(),
    });

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    listener.shutdown().await;

    assert_eq!(repo.call_count(), 1);
}

#[tokio::test]
async fn device_gone_triggers_rebuild() {
    let bus: Arc<dyn EventPublisher> = Arc::new(BroadcastEventBus::new(16));
    let repo = Arc::new(MockDeviceRepo::new());
    let snapshot = Arc::new(DeviceIpSnapshot::new(repo.clone()));

    let parent = tracing::info_span!("test");
    let listener = DeviceSnapshotListener::start(&bus, snapshot, &parent);

    bus.publish(WardnetEvent::DeviceGone {
        device_id: Uuid::new_v4(),
        mac: "AA:BB:CC:DD:EE:01".to_owned(),
        last_ip: "192.168.1.10".to_owned(),
        timestamp: Utc::now(),
    });

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    listener.shutdown().await;

    assert_eq!(repo.call_count(), 1);
}

#[tokio::test]
async fn unrelated_events_are_ignored() {
    let bus: Arc<dyn EventPublisher> = Arc::new(BroadcastEventBus::new(16));
    let repo = Arc::new(MockDeviceRepo::new());
    let snapshot = Arc::new(DeviceIpSnapshot::new(repo.clone()));

    let parent = tracing::info_span!("test");
    let listener = DeviceSnapshotListener::start(&bus, snapshot, &parent);

    bus.publish(WardnetEvent::TunnelStatsUpdated {
        tunnel_id: Uuid::new_v4(),
        status: wardnet_common::tunnel::TunnelStatus::Up,
        bytes_tx: 1000,
        bytes_rx: 2000,
        last_handshake: None,
        timestamp: Utc::now(),
    });

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    listener.shutdown().await;

    assert_eq!(
        repo.call_count(),
        0,
        "an irrelevant event must not trigger a rebuild"
    );
}

#[tokio::test]
async fn shutdown_without_events_completes_cleanly() {
    let bus: Arc<dyn EventPublisher> = Arc::new(BroadcastEventBus::new(16));
    let repo = Arc::new(MockDeviceRepo::new());
    let snapshot = Arc::new(DeviceIpSnapshot::new(repo.clone()));

    let parent = tracing::info_span!("test");
    let listener = DeviceSnapshotListener::start(&bus, snapshot, &parent);

    listener.shutdown().await;

    assert_eq!(repo.call_count(), 0);
}

#[tokio::test]
async fn rebuild_error_does_not_panic() {
    // Even if the rebuild errors, the listener loop must keep running —
    // this is the "warn and continue" branch in `rebuild`.
    let bus: Arc<dyn EventPublisher> = Arc::new(BroadcastEventBus::new(16));
    let repo = Arc::new(MockDeviceRepo::failing());
    let snapshot = Arc::new(DeviceIpSnapshot::new(repo.clone()));

    let parent = tracing::info_span!("test");
    let listener = DeviceSnapshotListener::start(&bus, snapshot, &parent);

    bus.publish(WardnetEvent::DeviceDiscovered {
        device_id: Uuid::new_v4(),
        mac: "AA:BB:CC:DD:EE:01".to_owned(),
        ip: "192.168.1.10".to_owned(),
        hostname: None,
        timestamp: Utc::now(),
    });

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    listener.shutdown().await;

    assert_eq!(repo.call_count(), 1);
}

#[tokio::test]
async fn burst_of_relevant_events_collapses_into_fewer_rebuilds() {
    // A departure sweep or discovery scan publishes one event per device;
    // `drain_pending` should collapse a burst that's already queued by the
    // time the handler runs into a single rebuild rather than one per event.
    let bus: Arc<dyn EventPublisher> = Arc::new(BroadcastEventBus::new(16));
    let repo = Arc::new(MockDeviceRepo::new());
    let snapshot = Arc::new(DeviceIpSnapshot::new(repo.clone()));

    let parent = tracing::info_span!("test");
    let listener = DeviceSnapshotListener::start(&bus, snapshot, &parent);

    for i in 0..5 {
        bus.publish(WardnetEvent::DeviceDiscovered {
            device_id: Uuid::new_v4(),
            mac: format!("AA:BB:CC:DD:EE:{i:02}"),
            ip: format!("192.168.1.{}", 10 + i),
            hostname: None,
            timestamp: Utc::now(),
        });
    }

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    listener.shutdown().await;

    let calls = repo.call_count();
    assert!(
        (1..5).contains(&calls),
        "expected the burst to collapse into fewer rebuilds than events, got {calls}"
    );
}
