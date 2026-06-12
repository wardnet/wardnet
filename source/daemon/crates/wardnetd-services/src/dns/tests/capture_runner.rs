use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use tokio::sync::{Mutex, broadcast, mpsc};
use uuid::Uuid;
use wardnet_common::device::{Device, DeviceType};
use wardnet_common::event::WardnetEvent;
use wardnet_common::routing::RoutingRule;
use wardnetd_data::repository::device::DeviceRow;
use wardnetd_data::repository::dns_events::{DnsCaptureStats, DnsEventsRepository};
use wardnetd_data::repository::{DeviceRepository, QueryLogRow};

use crate::dns::DnsCaptureRunner;
use crate::event::EventPublisher;

// ── Mocks ─────────────────────────────────────────────────────────────────

struct MockDeviceRepo {
    /// Device IDs that have capture enabled (returned by `find_all_capture_enabled_ids`).
    enabled_ids: Vec<String>,
    /// Optional single device returned by `find_by_id`.
    device: Option<Device>,
}

#[async_trait]
impl DeviceRepository for MockDeviceRepo {
    async fn find_by_ip(&self, _ip: &str) -> anyhow::Result<Option<Device>> {
        Ok(self.device.clone())
    }
    async fn find_by_id(&self, _id: &str) -> anyhow::Result<Option<Device>> {
        Ok(self.device.clone())
    }
    async fn find_by_mac(&self, _mac: &str) -> anyhow::Result<Option<Device>> {
        Ok(self.device.clone())
    }
    async fn find_all(&self) -> anyhow::Result<Vec<Device>> {
        Ok(self.device.clone().into_iter().collect())
    }
    async fn insert(&self, _device: &DeviceRow) -> anyhow::Result<()> {
        Ok(())
    }
    async fn update_last_seen_and_ip(
        &self,
        _id: &str,
        _ip: &str,
        _last_seen: &str,
    ) -> anyhow::Result<()> {
        Ok(())
    }
    async fn update_last_seen_batch(&self, _updates: &[(String, String)]) -> anyhow::Result<()> {
        Ok(())
    }
    async fn update_hostname(&self, _id: &str, _hostname: &str) -> anyhow::Result<()> {
        Ok(())
    }
    async fn update_name_and_type(
        &self,
        _id: &str,
        _name: Option<&str>,
        _device_type: &str,
    ) -> anyhow::Result<()> {
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
    async fn update_admin_locked(&self, _id: &str, _locked: bool) -> anyhow::Result<()> {
        Ok(())
    }
    async fn find_devices_for_tunnel(&self, _tid: &str) -> anyhow::Result<Vec<Device>> {
        Ok(vec![])
    }
    async fn switch_tunnel_rules_to_direct(
        &self,
        _tid: &str,
        _now: &str,
    ) -> anyhow::Result<Vec<String>> {
        Ok(vec![])
    }
    async fn count(&self) -> anyhow::Result<i64> {
        Ok(0)
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
        Ok(self.enabled_ids.clone())
    }
}

/// Records every insert call's (`device_id`, `domain`) pair for later assertion.
struct RecordingDnsEventsRepo {
    inserts: Mutex<Vec<(String, String)>>,
}

impl RecordingDnsEventsRepo {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            inserts: Mutex::new(vec![]),
        })
    }

    async fn recorded_inserts(&self) -> Vec<(String, String)> {
        self.inserts.lock().await.clone()
    }
}

#[async_trait]
impl DnsEventsRepository for RecordingDnsEventsRepo {
    async fn insert(
        &self,
        device_id: &str,
        domain: &str,
        _status: &str,
        _captured_at: &str,
    ) -> anyhow::Result<()> {
        self.inserts
            .lock()
            .await
            .push((device_id.to_owned(), domain.to_owned()));
        Ok(())
    }
    async fn stats_for_device(&self, _device_id: &str) -> anyhow::Result<DnsCaptureStats> {
        Ok(DnsCaptureStats {
            row_count: 0,
            size_bytes: 0,
        })
    }
    async fn prune_for_device(
        &self,
        _device_id: &str,
        _cap_count: i64,
        _cap_days: i64,
    ) -> anyhow::Result<u64> {
        Ok(0)
    }
    async fn delete_all_for_device(&self, _device_id: &str) -> anyhow::Result<u64> {
        Ok(0)
    }
    async fn find_device_ids_with_data(&self) -> anyhow::Result<Vec<String>> {
        Ok(vec![])
    }
}

/// A real broadcast-backed event publisher so tests can send events into the runner.
struct TestEventBus {
    sender: broadcast::Sender<WardnetEvent>,
}

impl TestEventBus {
    fn new() -> Arc<Self> {
        let (sender, _) = broadcast::channel(32);
        Arc::new(Self { sender })
    }

    fn send(&self, event: WardnetEvent) {
        let _ = self.sender.send(event);
    }
}

impl EventPublisher for TestEventBus {
    fn publish(&self, event: WardnetEvent) {
        let _ = self.sender.send(event);
    }

    fn subscribe(&self) -> broadcast::Receiver<WardnetEvent> {
        self.sender.subscribe()
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────

const DEV1: &str = "00000000-0000-0000-0000-000000000001";

fn sample_row(device_id: Option<&str>, domain: &str) -> QueryLogRow {
    QueryLogRow {
        timestamp: Utc::now().to_rfc3339(),
        client_ip: "192.168.1.10".to_owned(),
        domain: domain.to_owned(),
        query_type: "A".to_owned(),
        result: "NOERROR".to_owned(),
        upstream: None,
        latency_ms: 1.0,
        device_id: device_id.map(str::to_owned),
    }
}

fn sample_device(capture_enabled: bool) -> Device {
    Device {
        id: Uuid::parse_str(DEV1).unwrap(),
        mac: "AA:BB:CC:DD:EE:01".to_owned(),
        name: Some("Test Device".to_owned()),
        hostname: None,
        manufacturer: None,
        device_type: DeviceType::Phone,
        first_seen: "2026-01-01T00:00:00Z".parse().unwrap(),
        last_seen: "2026-01-01T00:00:00Z".parse().unwrap(),
        last_ip: "192.168.1.10".to_owned(),
        admin_locked: false,
        dns_capture_enabled: capture_enabled,
        dns_capture_cap_count: 1000,
        dns_capture_cap_days: 7,
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn rows_with_enabled_device_are_inserted() {
    let (tx, rx) = mpsc::channel(16);
    let device_repo: Arc<dyn DeviceRepository> = Arc::new(MockDeviceRepo {
        enabled_ids: vec![DEV1.to_owned()],
        device: Some(sample_device(true)),
    });
    let dns_repo = RecordingDnsEventsRepo::new();
    let dns_repo_dyn: Arc<dyn DnsEventsRepository> =
        Arc::clone(&dns_repo) as Arc<dyn DnsEventsRepository>;
    let events: Arc<dyn EventPublisher> = TestEventBus::new();

    let runner = DnsCaptureRunner::start(
        rx,
        device_repo,
        dns_repo_dyn,
        Arc::clone(&events),
        &tracing::Span::current(),
    );

    tx.send(sample_row(Some(DEV1), "example.com"))
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;

    runner.shutdown().await;

    let inserts = dns_repo.recorded_inserts().await;
    assert_eq!(inserts.len(), 1);
    assert_eq!(inserts[0].0, DEV1);
    assert_eq!(inserts[0].1, "example.com");
}

#[tokio::test]
async fn rows_without_device_id_are_skipped() {
    let (tx, rx) = mpsc::channel(16);
    let device_repo: Arc<dyn DeviceRepository> = Arc::new(MockDeviceRepo {
        enabled_ids: vec![DEV1.to_owned()],
        device: None,
    });
    let dns_repo = RecordingDnsEventsRepo::new();
    let dns_repo_dyn: Arc<dyn DnsEventsRepository> =
        Arc::clone(&dns_repo) as Arc<dyn DnsEventsRepository>;
    let events: Arc<dyn EventPublisher> = TestEventBus::new();

    let runner = DnsCaptureRunner::start(
        rx,
        device_repo,
        dns_repo_dyn,
        Arc::clone(&events),
        &tracing::Span::current(),
    );

    // device_id is None — should be skipped regardless of enabled set
    tx.send(sample_row(None, "example.com")).await.unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;

    runner.shutdown().await;

    let inserts = dns_repo.recorded_inserts().await;
    assert!(inserts.is_empty(), "expected no inserts, got {inserts:?}");
}

#[tokio::test]
async fn rows_for_non_enabled_device_are_skipped() {
    let (tx, rx) = mpsc::channel(16);
    let device_repo: Arc<dyn DeviceRepository> = Arc::new(MockDeviceRepo {
        // DEV1 is NOT in the enabled set
        enabled_ids: vec![],
        device: Some(sample_device(false)),
    });
    let dns_repo = RecordingDnsEventsRepo::new();
    let dns_repo_dyn: Arc<dyn DnsEventsRepository> =
        Arc::clone(&dns_repo) as Arc<dyn DnsEventsRepository>;
    let events: Arc<dyn EventPublisher> = TestEventBus::new();

    let runner = DnsCaptureRunner::start(
        rx,
        device_repo,
        dns_repo_dyn,
        Arc::clone(&events),
        &tracing::Span::current(),
    );

    tx.send(sample_row(Some(DEV1), "example.com"))
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;

    runner.shutdown().await;

    let inserts = dns_repo.recorded_inserts().await;
    assert!(inserts.is_empty(), "expected no inserts, got {inserts:?}");
}

#[tokio::test]
async fn settings_changed_event_enables_device() {
    let (tx, rx) = mpsc::channel(16);
    let device_repo: Arc<dyn DeviceRepository> = Arc::new(MockDeviceRepo {
        // Start with DEV1 disabled (not in enabled set)
        enabled_ids: vec![],
        device: None,
    });
    let dns_repo = RecordingDnsEventsRepo::new();
    let dns_repo_dyn: Arc<dyn DnsEventsRepository> =
        Arc::clone(&dns_repo) as Arc<dyn DnsEventsRepository>;
    let event_bus = TestEventBus::new();
    let events: Arc<dyn EventPublisher> = Arc::clone(&event_bus) as Arc<dyn EventPublisher>;

    let runner = DnsCaptureRunner::start(
        rx,
        device_repo,
        dns_repo_dyn,
        Arc::clone(&events),
        &tracing::Span::current(),
    );

    // Allow runner to initialize before publishing the event
    tokio::time::sleep(Duration::from_millis(10)).await;

    // Publish a settings-changed event that enables DEV1
    event_bus.send(WardnetEvent::DeviceCaptureSettingsChanged {
        device_id: Uuid::parse_str(DEV1).unwrap(),
        enabled: true,
        timestamp: Utc::now(),
    });

    // Give the runner time to process the event before sending the row
    tokio::time::sleep(Duration::from_millis(20)).await;

    tx.send(sample_row(Some(DEV1), "enabled-after-event.com"))
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;

    runner.shutdown().await;

    let inserts = dns_repo.recorded_inserts().await;
    assert_eq!(inserts.len(), 1, "expected one insert, got {inserts:?}");
    assert_eq!(inserts[0].0, DEV1);
    assert_eq!(inserts[0].1, "enabled-after-event.com");
}

#[tokio::test]
async fn settings_changed_event_disables_device() {
    let (tx, rx) = mpsc::channel(16);
    let device_repo: Arc<dyn DeviceRepository> = Arc::new(MockDeviceRepo {
        // Start with DEV1 enabled
        enabled_ids: vec![DEV1.to_owned()],
        device: Some(sample_device(true)),
    });
    let dns_repo = RecordingDnsEventsRepo::new();
    let dns_repo_dyn: Arc<dyn DnsEventsRepository> =
        Arc::clone(&dns_repo) as Arc<dyn DnsEventsRepository>;
    let event_bus = TestEventBus::new();
    let events: Arc<dyn EventPublisher> = Arc::clone(&event_bus) as Arc<dyn EventPublisher>;

    let runner = DnsCaptureRunner::start(
        rx,
        device_repo,
        dns_repo_dyn,
        Arc::clone(&events),
        &tracing::Span::current(),
    );

    // Allow runner to initialize before publishing the event
    tokio::time::sleep(Duration::from_millis(10)).await;

    // Publish a settings-changed event that disables DEV1
    event_bus.send(WardnetEvent::DeviceCaptureSettingsChanged {
        device_id: Uuid::parse_str(DEV1).unwrap(),
        enabled: false,
        timestamp: Utc::now(),
    });

    // Give the runner time to process the event before sending the row
    tokio::time::sleep(Duration::from_millis(20)).await;

    tx.send(sample_row(Some(DEV1), "should-be-skipped.com"))
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;

    runner.shutdown().await;

    let inserts = dns_repo.recorded_inserts().await;
    assert!(
        inserts.is_empty(),
        "expected no inserts after disable event, got {inserts:?}"
    );
}

#[tokio::test]
async fn shutdown_completes() {
    let (_tx, rx) = mpsc::channel::<QueryLogRow>(16);
    let device_repo: Arc<dyn DeviceRepository> = Arc::new(MockDeviceRepo {
        enabled_ids: vec![],
        device: None,
    });
    let dns_repo: Arc<dyn DnsEventsRepository> = RecordingDnsEventsRepo::new();
    let events: Arc<dyn EventPublisher> = TestEventBus::new();

    let runner =
        DnsCaptureRunner::start(rx, device_repo, dns_repo, events, &tracing::Span::current());

    // Immediately shut down — should complete without panic
    runner.shutdown().await;
}
