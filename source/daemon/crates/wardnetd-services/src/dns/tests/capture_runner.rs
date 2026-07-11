use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use std::collections::HashMap;
use tokio::sync::{Mutex, broadcast, mpsc};
use uuid::Uuid;

use wardnet_common::api::{
    DeviceMeResponse, DnsCaptureSettingsResponse, DnsEventItem, SetMyRuleResponse,
};
use wardnet_common::device::{Device, DeviceType};
use wardnet_common::event::WardnetEvent;
use wardnet_common::routing::RoutingTarget;
use wardnetd_data::repository::QueryLogRow;
use wardnetd_data::repository::dns_events::{DnsCaptureStats, DnsEventsRepository};

use crate::device::DeviceService;
use crate::dns::DnsCaptureRunner;
use crate::error::AppError;
use crate::event::EventPublisher;

// ── Mocks ─────────────────────────────────────────────────────────────────

struct MockDeviceService {
    /// Device IDs that have capture enabled.
    enabled_ids: Vec<String>,
    /// Optional single device for capture-settings lookups.
    device: Option<Device>,
    /// When `true`, `get_device_capture_settings` returns an error.
    error_on_settings: bool,
}

#[async_trait]
impl DeviceService for MockDeviceService {
    async fn get_device(
        &self,
        _device_id: &str,
    ) -> Result<Option<wardnet_common::device::Device>, AppError> {
        unimplemented!("not used by DnsCaptureRunner")
    }
    async fn clear_remote_connection_mode(&self, _device_id: &str) -> Result<(), AppError> {
        unimplemented!("not used by DnsCaptureRunner")
    }
    async fn get_device_for_ip(&self, _ip: &str) -> Result<DeviceMeResponse, AppError> {
        unimplemented!("not used by DnsCaptureRunner")
    }
    async fn set_rule_for_ip(
        &self,
        _ip: &str,
        _target: RoutingTarget,
    ) -> Result<SetMyRuleResponse, AppError> {
        unimplemented!()
    }
    async fn set_rule(&self, _device_id: &str, _target: RoutingTarget) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn current_rules(&self) -> Result<HashMap<uuid::Uuid, RoutingTarget>, AppError> {
        unimplemented!()
    }
    async fn update_admin_locked(&self, _device_id: &str, _locked: bool) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn get_dns_capture_settings(
        &self,
        _device_id: &str,
    ) -> Result<DnsCaptureSettingsResponse, AppError> {
        unimplemented!()
    }
    async fn update_dns_capture_settings(
        &self,
        _device_id: &str,
        _enabled: Option<bool>,
        _cap_count: Option<i64>,
        _cap_days: Option<i64>,
    ) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn set_my_capture_enabled(
        &self,
        _ip: &str,
        _enabled: bool,
    ) -> Result<DnsCaptureSettingsResponse, AppError> {
        unimplemented!()
    }
    async fn fetch_pending_dns_events(
        &self,
        _device_id: &str,
        _after_id: i64,
        _limit: i64,
    ) -> Result<Vec<DnsEventItem>, AppError> {
        unimplemented!()
    }
    async fn mark_dns_events_synced(
        &self,
        _device_id: &str,
        _up_to_id: i64,
    ) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn ack_dns_events(&self, _device_id: &str, _up_to_id: i64) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn list_capture_enabled_device_ids(&self) -> Result<Vec<String>, AppError> {
        Ok(self.enabled_ids.clone())
    }
    async fn get_device_capture_settings(
        &self,
        _device_id: &str,
    ) -> Result<Option<(bool, i64, i64)>, AppError> {
        if self.error_on_settings {
            return Err(AppError::Internal(anyhow::anyhow!("db error")));
        }
        Ok(self.device.as_ref().map(|d| {
            (
                d.dns_capture_enabled,
                d.dns_capture_cap_count,
                d.dns_capture_cap_days,
            )
        }))
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
    ) -> anyhow::Result<i64> {
        self.inserts
            .lock()
            .await
            .push((device_id.to_owned(), domain.to_owned()));
        Ok(1)
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
    async fn fetch_pending(
        &self,
        _device_id: &str,
        _after_id: i64,
        _limit: i64,
    ) -> anyhow::Result<Vec<wardnetd_data::repository::DnsEventRow>> {
        Ok(vec![])
    }
    async fn mark_synced_up_to(&self, _device_id: &str, _up_to_id: i64) -> anyhow::Result<u64> {
        Ok(0)
    }
    async fn delete_up_to(&self, _device_id: &str, _up_to_id: i64) -> anyhow::Result<u64> {
        Ok(0)
    }
}

/// Tracks `prune_for_device` and `delete_all_for_device` calls for prune-loop tests.
struct PruningDnsEventsRepo {
    device_ids: Vec<String>,
    prune_calls: Mutex<Vec<String>>,
    delete_calls: Mutex<Vec<String>>,
}

impl PruningDnsEventsRepo {
    fn new(device_ids: &[&str]) -> Arc<Self> {
        Arc::new(Self {
            device_ids: device_ids.iter().map(|s| (*s).to_string()).collect(),
            prune_calls: Mutex::new(vec![]),
            delete_calls: Mutex::new(vec![]),
        })
    }
}

#[async_trait]
impl DnsEventsRepository for PruningDnsEventsRepo {
    async fn insert(
        &self,
        _device_id: &str,
        _domain: &str,
        _status: &str,
        _captured_at: &str,
    ) -> anyhow::Result<i64> {
        Ok(1)
    }
    async fn stats_for_device(&self, _device_id: &str) -> anyhow::Result<DnsCaptureStats> {
        Ok(DnsCaptureStats {
            row_count: 0,
            size_bytes: 0,
        })
    }
    async fn prune_for_device(
        &self,
        device_id: &str,
        _cap_count: i64,
        _cap_days: i64,
    ) -> anyhow::Result<u64> {
        self.prune_calls.lock().await.push(device_id.to_owned());
        Ok(1)
    }
    async fn delete_all_for_device(&self, device_id: &str) -> anyhow::Result<u64> {
        self.delete_calls.lock().await.push(device_id.to_owned());
        Ok(0)
    }
    async fn find_device_ids_with_data(&self) -> anyhow::Result<Vec<String>> {
        Ok(self.device_ids.clone())
    }
    async fn fetch_pending(
        &self,
        _device_id: &str,
        _after_id: i64,
        _limit: i64,
    ) -> anyhow::Result<Vec<wardnetd_data::repository::DnsEventRow>> {
        Ok(vec![])
    }
    async fn mark_synced_up_to(&self, _device_id: &str, _up_to_id: i64) -> anyhow::Result<u64> {
        Ok(0)
    }
    async fn delete_up_to(&self, _device_id: &str, _up_to_id: i64) -> anyhow::Result<u64> {
        Ok(0)
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
        zone_id: "00000000-0000-0000-0000-000000000201".parse().unwrap(),
        dns_capture_enabled: capture_enabled,
        dns_capture_cap_count: 1000,
        dns_capture_cap_days: 7,
        connection_mode: wardnet_common::device::DeviceConnectionMode::Lan,
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn rows_with_enabled_device_are_inserted() {
    let (tx, rx) = mpsc::channel(16);
    let device_service: Arc<dyn DeviceService> = Arc::new(MockDeviceService {
        enabled_ids: vec![DEV1.to_owned()],
        device: Some(sample_device(true)),
        error_on_settings: false,
    });
    let dns_repo = RecordingDnsEventsRepo::new();
    let dns_repo_dyn: Arc<dyn DnsEventsRepository> =
        Arc::clone(&dns_repo) as Arc<dyn DnsEventsRepository>;
    let events: Arc<dyn EventPublisher> = TestEventBus::new();

    let runner = DnsCaptureRunner::start(
        rx,
        device_service,
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
    let device_service: Arc<dyn DeviceService> = Arc::new(MockDeviceService {
        enabled_ids: vec![DEV1.to_owned()],
        device: None,
        error_on_settings: false,
    });
    let dns_repo = RecordingDnsEventsRepo::new();
    let dns_repo_dyn: Arc<dyn DnsEventsRepository> =
        Arc::clone(&dns_repo) as Arc<dyn DnsEventsRepository>;
    let events: Arc<dyn EventPublisher> = TestEventBus::new();

    let runner = DnsCaptureRunner::start(
        rx,
        device_service,
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
    let device_service: Arc<dyn DeviceService> = Arc::new(MockDeviceService {
        // DEV1 is NOT in the enabled set
        enabled_ids: vec![],
        device: Some(sample_device(false)),
        error_on_settings: false,
    });
    let dns_repo = RecordingDnsEventsRepo::new();
    let dns_repo_dyn: Arc<dyn DnsEventsRepository> =
        Arc::clone(&dns_repo) as Arc<dyn DnsEventsRepository>;
    let events: Arc<dyn EventPublisher> = TestEventBus::new();

    let runner = DnsCaptureRunner::start(
        rx,
        device_service,
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
    let device_service: Arc<dyn DeviceService> = Arc::new(MockDeviceService {
        // Start with DEV1 disabled (not in enabled set)
        enabled_ids: vec![],
        device: None,
        error_on_settings: false,
    });
    let dns_repo = RecordingDnsEventsRepo::new();
    let dns_repo_dyn: Arc<dyn DnsEventsRepository> =
        Arc::clone(&dns_repo) as Arc<dyn DnsEventsRepository>;
    let event_bus = TestEventBus::new();
    let events: Arc<dyn EventPublisher> = Arc::clone(&event_bus) as Arc<dyn EventPublisher>;

    let runner = DnsCaptureRunner::start(
        rx,
        device_service,
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
    let device_service: Arc<dyn DeviceService> = Arc::new(MockDeviceService {
        // Start with DEV1 enabled
        enabled_ids: vec![DEV1.to_owned()],
        device: Some(sample_device(true)),
        error_on_settings: false,
    });
    let dns_repo = RecordingDnsEventsRepo::new();
    let dns_repo_dyn: Arc<dyn DnsEventsRepository> =
        Arc::clone(&dns_repo) as Arc<dyn DnsEventsRepository>;
    let event_bus = TestEventBus::new();
    let events: Arc<dyn EventPublisher> = Arc::clone(&event_bus) as Arc<dyn EventPublisher>;

    let runner = DnsCaptureRunner::start(
        rx,
        device_service,
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
    let device_service: Arc<dyn DeviceService> = Arc::new(MockDeviceService {
        enabled_ids: vec![],
        device: None,
        error_on_settings: false,
    });
    let dns_repo: Arc<dyn DnsEventsRepository> = RecordingDnsEventsRepo::new();
    let events: Arc<dyn EventPublisher> = TestEventBus::new();

    let runner = DnsCaptureRunner::start(
        rx,
        device_service,
        dns_repo,
        events,
        &tracing::Span::current(),
    );

    // Immediately shut down — should complete without panic
    runner.shutdown().await;
}

#[tokio::test]
async fn channel_closed_exits_runner() {
    let (tx, rx) = mpsc::channel::<QueryLogRow>(16);
    let device_service: Arc<dyn DeviceService> = Arc::new(MockDeviceService {
        enabled_ids: vec![],
        device: None,
        error_on_settings: false,
    });
    let dns_repo: Arc<dyn DnsEventsRepository> = RecordingDnsEventsRepo::new();
    let events: Arc<dyn EventPublisher> = TestEventBus::new();

    let runner = DnsCaptureRunner::start(
        rx,
        device_service,
        dns_repo,
        events,
        &tracing::Span::current(),
    );

    // Dropping the sender closes the channel; the runner should exit the receive loop.
    drop(tx);
    tokio::time::sleep(Duration::from_millis(100)).await;
    runner.shutdown().await;
}

#[tokio::test]
async fn prune_loop_calls_prune_for_enabled_device() {
    let (_tx, rx) = mpsc::channel::<QueryLogRow>(16);
    let device_service: Arc<dyn DeviceService> = Arc::new(MockDeviceService {
        enabled_ids: vec![DEV1.to_owned()],
        device: Some(sample_device(true)),
        error_on_settings: false,
    });
    let dns_repo = PruningDnsEventsRepo::new(&[DEV1]);
    let dns_repo_dyn: Arc<dyn DnsEventsRepository> =
        Arc::clone(&dns_repo) as Arc<dyn DnsEventsRepository>;
    let events: Arc<dyn EventPublisher> = TestEventBus::new();

    let runner = DnsCaptureRunner::start_with_prune_interval(
        rx,
        device_service,
        dns_repo_dyn,
        Arc::clone(&events),
        Duration::from_millis(50),
        &tracing::Span::current(),
    );

    tokio::time::sleep(Duration::from_millis(200)).await;
    runner.shutdown().await;

    let prune_calls = dns_repo.prune_calls.lock().await;
    assert!(
        !prune_calls.is_empty(),
        "expected prune_for_device to be called at least once"
    );
    assert_eq!(prune_calls[0], DEV1);
}

#[tokio::test]
async fn prune_loop_deletes_data_for_disabled_device() {
    let (_tx, rx) = mpsc::channel::<QueryLogRow>(16);
    let device_service: Arc<dyn DeviceService> = Arc::new(MockDeviceService {
        enabled_ids: vec![],
        device: Some(sample_device(false)),
        error_on_settings: false,
    });
    let dns_repo = PruningDnsEventsRepo::new(&[DEV1]);
    let dns_repo_dyn: Arc<dyn DnsEventsRepository> =
        Arc::clone(&dns_repo) as Arc<dyn DnsEventsRepository>;
    let events: Arc<dyn EventPublisher> = TestEventBus::new();

    let runner = DnsCaptureRunner::start_with_prune_interval(
        rx,
        device_service,
        dns_repo_dyn,
        Arc::clone(&events),
        Duration::from_millis(50),
        &tracing::Span::current(),
    );

    tokio::time::sleep(Duration::from_millis(200)).await;
    runner.shutdown().await;

    let delete_calls = dns_repo.delete_calls.lock().await;
    assert!(
        !delete_calls.is_empty(),
        "expected delete_all_for_device to be called for disabled device"
    );
    assert_eq!(delete_calls[0], DEV1);
}

#[tokio::test]
async fn prune_loop_deletes_data_for_unknown_device() {
    let (_tx, rx) = mpsc::channel::<QueryLogRow>(16);
    let device_service: Arc<dyn DeviceService> = Arc::new(MockDeviceService {
        enabled_ids: vec![],
        device: None, // device has been deleted from the DB
        error_on_settings: false,
    });
    let dns_repo = PruningDnsEventsRepo::new(&[DEV1]);
    let dns_repo_dyn: Arc<dyn DnsEventsRepository> =
        Arc::clone(&dns_repo) as Arc<dyn DnsEventsRepository>;
    let events: Arc<dyn EventPublisher> = TestEventBus::new();

    let runner = DnsCaptureRunner::start_with_prune_interval(
        rx,
        device_service,
        dns_repo_dyn,
        Arc::clone(&events),
        Duration::from_millis(50),
        &tracing::Span::current(),
    );

    tokio::time::sleep(Duration::from_millis(200)).await;
    runner.shutdown().await;

    let delete_calls = dns_repo.delete_calls.lock().await;
    assert!(
        !delete_calls.is_empty(),
        "expected delete_all_for_device to be called for unknown/deleted device"
    );
}

#[tokio::test]
async fn prune_loop_warns_on_device_settings_error() {
    // Verify that the Err arm in run_prune is exercised: when
    // get_device_capture_settings returns Err, the runner logs a warning but
    // does not call prune_for_device or delete_all_for_device.
    let (_tx, rx) = mpsc::channel::<QueryLogRow>(16);
    let device_service: Arc<dyn DeviceService> = Arc::new(MockDeviceService {
        enabled_ids: vec![],
        device: None,
        error_on_settings: true,
    });
    let dns_repo = PruningDnsEventsRepo::new(&[DEV1]);
    let dns_repo_dyn: Arc<dyn DnsEventsRepository> =
        Arc::clone(&dns_repo) as Arc<dyn DnsEventsRepository>;
    let events: Arc<dyn EventPublisher> = TestEventBus::new();

    let runner = DnsCaptureRunner::start_with_prune_interval(
        rx,
        device_service,
        dns_repo_dyn,
        Arc::clone(&events),
        Duration::from_millis(50),
        &tracing::Span::current(),
    );

    tokio::time::sleep(Duration::from_millis(200)).await;
    runner.shutdown().await;

    // Neither prune nor delete should have been called — the error path
    // logs a warning and skips the device.
    assert!(
        dns_repo.prune_calls.lock().await.is_empty(),
        "prune should not be called on device settings error"
    );
    assert!(
        dns_repo.delete_calls.lock().await.is_empty(),
        "delete should not be called on device settings error"
    );
}
