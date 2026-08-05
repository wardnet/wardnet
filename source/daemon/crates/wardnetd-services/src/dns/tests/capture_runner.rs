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

use crate::auth_context;
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
    /// When `true`, `list_capture_enabled_device_ids` returns an error,
    /// exercising the runner's start-empty-on-load-failure path.
    error_on_list: bool,
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
    async fn get_rule_for_device(
        &self,
        _device_id: &str,
    ) -> Result<Option<RoutingTarget>, AppError> {
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
    async fn ack_dns_events(&self, _device_id: &str, _up_to_id: i64) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn list_capture_enabled_device_ids(&self) -> Result<Vec<String>, AppError> {
        // Regression: the runner runs outside the HTTP middleware and must
        // establish an admin context before calling the service (issue #839).
        auth_context::require_admin().expect(
            "DnsCaptureRunner must call list_capture_enabled_device_ids under admin context",
        );
        if self.error_on_list {
            return Err(AppError::Internal(anyhow::anyhow!("db error")));
        }
        Ok(self.enabled_ids.clone())
    }
    async fn get_device_capture_settings(
        &self,
        _device_id: &str,
    ) -> Result<Option<(bool, i64, i64)>, AppError> {
        // Regression: the prune path must also carry an admin context (#839).
        auth_context::require_admin()
            .expect("DnsCaptureRunner must call get_device_capture_settings under admin context");
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
    async fn delete_up_to(&self, _device_id: &str, _up_to_id: i64) -> anyhow::Result<u64> {
        Ok(0)
    }
}

/// Tracks `prune_for_device` and `delete_all_for_device` calls for prune-loop tests.
struct PruningDnsEventsRepo {
    device_ids: Vec<String>,
    prune_calls: Mutex<Vec<String>>,
    delete_calls: Mutex<Vec<String>>,
    /// Number of prune-loop ticks observed (one `find_device_ids_with_data`
    /// call per tick). Lets tests wait for a tick to complete instead of
    /// sleeping a guessed interval.
    find_calls: Mutex<u32>,
}

impl PruningDnsEventsRepo {
    fn new(device_ids: &[&str]) -> Arc<Self> {
        Arc::new(Self {
            device_ids: device_ids.iter().map(|s| (*s).to_string()).collect(),
            prune_calls: Mutex::new(vec![]),
            delete_calls: Mutex::new(vec![]),
            find_calls: Mutex::new(0),
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
        *self.find_calls.lock().await += 1;
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
        // Generously sized so the retry-until-captured loops below (which
        // re-publish settings events every ~10ms) cannot overflow the buffer
        // and trip the runner's Lagged→DB-reload path, which would otherwise
        // resurrect a device the test just disabled.
        let (sender, _) = broadcast::channel(4096);
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
/// A second, always-usable device the negative tests enable purely as a
/// synchronization sentinel. Because the runner drains `capture_rx` in FIFO
/// order, a recorded insert for this device proves every row queued before it
/// has already been processed — the happens-after barrier the old fixed sleeps
/// lacked.
const SENT: &str = "00000000-0000-0000-0000-0000000000ff";

/// Poll an async predicate up to `tries` times, 10ms apart, returning `true`
/// as soon as it holds. Mirrors `auth/tests/session_cleanup_runner.rs`'s
/// `wait_until`, adapted for this runner's async observables. Replaces the
/// "sleep a guessed interval, then assert once" pattern so the tests are not
/// flaky under CI load.
async fn wait_until<F, Fut>(tries: u32, mut predicate: F) -> bool
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    for _ in 0..tries {
        if predicate().await {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    predicate().await
}

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
        protocol: "udp".to_owned(),
    }
}

fn sample_device(capture_enabled: bool) -> Device {
    Device {
        id: Uuid::parse_str(DEV1).unwrap(),
        mac: "AA:BB:CC:DD:EE:01".to_owned(),
        name: Some("Test Device".to_owned()),
        hostname: None,
        manufacturer: None,
        manufacturer_source: None,
        is_randomized: false,
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
        error_on_list: false,
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
    assert!(
        wait_until(200, || {
            let repo = Arc::clone(&dns_repo);
            async move { !repo.recorded_inserts().await.is_empty() }
        })
        .await,
        "the row for the enabled device should have been captured"
    );

    runner.shutdown().await;

    let inserts = dns_repo.recorded_inserts().await;
    assert_eq!(inserts.len(), 1);
    assert_eq!(inserts[0].0, DEV1);
    assert_eq!(inserts[0].1, "example.com");
}

#[tokio::test]
async fn start_load_failure_begins_with_empty_cache() {
    let (tx, rx) = mpsc::channel(16);
    // The startup load of capture-enabled IDs errors, so the runner logs and
    // begins with an empty cache — a row for an otherwise-enabled device is
    // therefore dropped rather than captured.
    let device_service: Arc<dyn DeviceService> = Arc::new(MockDeviceService {
        enabled_ids: vec![DEV1.to_owned()],
        device: Some(sample_device(true)),
        error_on_settings: false,
        error_on_list: true,
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

    // The row for DEV1 is sent while the cache is empty (load failed), so it
    // must be dropped. We then enable a sentinel device and push its row
    // through: FIFO draining of `capture_rx` means that once the sentinel row
    // lands, the runner has already consumed — and dropped — the earlier DEV1
    // row, so an empty DEV1 result is a genuine drop, not an unprocessed row.
    tx.send(sample_row(Some(DEV1), "dropped.com"))
        .await
        .unwrap();
    let sentinel_seen = wait_until(300, || {
        let repo = Arc::clone(&dns_repo);
        let tx = tx.clone();
        let event_bus = Arc::clone(&event_bus);
        async move {
            // Re-published each iteration so the enable is not lost if it races
            // ahead of the runner subscribing to the bus.
            event_bus.send(WardnetEvent::DeviceCaptureSettingsChanged {
                device_id: Uuid::parse_str(SENT).unwrap(),
                enabled: true,
                timestamp: Utc::now(),
            });
            tx.send(sample_row(Some(SENT), "sentinel.com"))
                .await
                .unwrap();
            repo.recorded_inserts()
                .await
                .iter()
                .any(|(id, _)| id == SENT)
        }
    })
    .await;
    assert!(
        sentinel_seen,
        "sentinel row was never captured; runner is not making progress"
    );

    runner.shutdown().await;

    let inserts = dns_repo.recorded_inserts().await;
    assert!(
        inserts.iter().all(|(id, _)| id == SENT),
        "a startup load failure must start the cache empty, dropping the DEV1 row: {inserts:?}"
    );
}

#[tokio::test]
async fn rows_without_device_id_are_skipped() {
    let (tx, rx) = mpsc::channel(16);
    let device_service: Arc<dyn DeviceService> = Arc::new(MockDeviceService {
        enabled_ids: vec![DEV1.to_owned()],
        device: None,
        error_on_settings: false,
        error_on_list: false,
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

    // A row with no device_id must be skipped regardless of the enabled set.
    // DEV1 is enabled, so a following DEV1 row acts as a FIFO barrier: once it
    // lands, the None row ahead of it in the queue has already been processed.
    tx.send(sample_row(None, "no-device.com")).await.unwrap();
    tx.send(sample_row(Some(DEV1), "sentinel.com"))
        .await
        .unwrap();
    assert!(
        wait_until(200, || {
            let repo = Arc::clone(&dns_repo);
            async move {
                repo.recorded_inserts()
                    .await
                    .iter()
                    .any(|(_, d)| d == "sentinel.com")
            }
        })
        .await,
        "sentinel row was never captured"
    );

    runner.shutdown().await;

    let inserts = dns_repo.recorded_inserts().await;
    assert_eq!(
        inserts,
        vec![(DEV1.to_owned(), "sentinel.com".to_owned())],
        "the row without a device_id must be skipped: {inserts:?}"
    );
}

#[tokio::test]
async fn rows_for_non_enabled_device_are_skipped() {
    let (tx, rx) = mpsc::channel(16);
    let device_service: Arc<dyn DeviceService> = Arc::new(MockDeviceService {
        // DEV1 is NOT in the enabled set; SENT is enabled purely as a FIFO
        // synchronization sentinel.
        enabled_ids: vec![SENT.to_owned()],
        device: Some(sample_device(false)),
        error_on_settings: false,
        error_on_list: false,
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

    // DEV1 is not enabled, so its row must be dropped. The SENT sentinel row
    // behind it is the FIFO barrier confirming the DEV1 row was processed.
    tx.send(sample_row(Some(DEV1), "dropped.com"))
        .await
        .unwrap();
    tx.send(sample_row(Some(SENT), "sentinel.com"))
        .await
        .unwrap();
    assert!(
        wait_until(200, || {
            let repo = Arc::clone(&dns_repo);
            async move {
                repo.recorded_inserts()
                    .await
                    .iter()
                    .any(|(id, _)| id == SENT)
            }
        })
        .await,
        "sentinel row was never captured"
    );

    runner.shutdown().await;

    let inserts = dns_repo.recorded_inserts().await;
    assert!(
        !inserts.iter().any(|(id, _)| id == DEV1),
        "a row for a non-enabled device must be dropped: {inserts:?}"
    );
}

#[tokio::test]
async fn settings_changed_event_enables_device() {
    let (tx, rx) = mpsc::channel(16);
    let device_service: Arc<dyn DeviceService> = Arc::new(MockDeviceService {
        // Start with DEV1 disabled (not in enabled set)
        enabled_ids: vec![],
        device: None,
        error_on_settings: false,
        error_on_list: false,
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

    // Enable DEV1 (plus a SENT sentinel) via the broadcast bus. The runner
    // subscribes only after its startup load, and the event/row channels then
    // race in its `select!`, so a fixed sleep can't reliably order any of this.
    // We re-publish both enables each iteration and send only SENT probe rows
    // until one is captured; because the pair is published FIFO with DEV1 first,
    // a captured SENT row proves the DEV1 enable was applied too. No DEV1 rows
    // are sent in this phase, so the DEV1 insert count stays at zero.
    let enabled = wait_until(300, || {
        let repo = Arc::clone(&dns_repo);
        let tx = tx.clone();
        let event_bus = Arc::clone(&event_bus);
        async move {
            event_bus.send(WardnetEvent::DeviceCaptureSettingsChanged {
                device_id: Uuid::parse_str(DEV1).unwrap(),
                enabled: true,
                timestamp: Utc::now(),
            });
            event_bus.send(WardnetEvent::DeviceCaptureSettingsChanged {
                device_id: Uuid::parse_str(SENT).unwrap(),
                enabled: true,
                timestamp: Utc::now(),
            });
            tx.send(sample_row(Some(SENT), "sentinel.com"))
                .await
                .unwrap();
            repo.recorded_inserts()
                .await
                .iter()
                .any(|(id, _)| id == SENT)
        }
    })
    .await;
    assert!(
        enabled,
        "sentinel never captured; cannot confirm the enable was applied"
    );

    // Send exactly one DEV1 row, with a SENT barrier behind it so we only assert
    // once the DEV1 row has been processed. This keeps the "exactly one insert
    // per row" invariant: a regression that double-inserts a captured row fails
    // the `== 1` check below.
    tx.send(sample_row(Some(DEV1), "enabled-after-event.com"))
        .await
        .unwrap();
    tx.send(sample_row(Some(SENT), "barrier.com"))
        .await
        .unwrap();
    assert!(
        wait_until(300, || {
            let repo = Arc::clone(&dns_repo);
            async move {
                repo.recorded_inserts()
                    .await
                    .iter()
                    .any(|(_, d)| d == "barrier.com")
            }
        })
        .await,
        "barrier row was never captured"
    );

    runner.shutdown().await;

    let dev1_inserts = dns_repo
        .recorded_inserts()
        .await
        .into_iter()
        .filter(|(id, d)| id == DEV1 && d == "enabled-after-event.com")
        .count();
    assert_eq!(
        dev1_inserts, 1,
        "the single enabled DEV1 row must be captured exactly once"
    );
}

#[tokio::test]
async fn settings_changed_event_disables_device() {
    let (tx, rx) = mpsc::channel(16);
    let device_service: Arc<dyn DeviceService> = Arc::new(MockDeviceService {
        // Start with DEV1 enabled
        enabled_ids: vec![DEV1.to_owned()],
        device: Some(sample_device(true)),
        error_on_settings: false,
        error_on_list: false,
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

    // DEV1 starts enabled: confirm it is actually capturing before we disable
    // it, so the test cannot pass vacuously against a never-enabled device.
    tx.send(sample_row(Some(DEV1), "before-disable.com"))
        .await
        .unwrap();
    assert!(
        wait_until(200, || {
            let repo = Arc::clone(&dns_repo);
            async move {
                repo.recorded_inserts()
                    .await
                    .iter()
                    .any(|(_, d)| d == "before-disable.com")
            }
        })
        .await,
        "DEV1 should be capturing before the disable event"
    );

    // Each iteration disables DEV1, then enables the sentinel — both on the
    // broadcast bus, which the runner receives in FIFO order. Re-publishing every
    // iteration guards against the events racing ahead of the runner subscribing
    // to the bus. Once a sentinel row is captured (the enable was applied), the
    // disable that preceded it in the same FIFO pair has necessarily been applied
    // too. That ordering is what lets us assert the DEV1 drop deterministically
    // despite the event-vs-row `select!` race.
    let sentinel_seen = wait_until(300, || {
        let repo = Arc::clone(&dns_repo);
        let tx = tx.clone();
        let event_bus = Arc::clone(&event_bus);
        async move {
            event_bus.send(WardnetEvent::DeviceCaptureSettingsChanged {
                device_id: Uuid::parse_str(DEV1).unwrap(),
                enabled: false,
                timestamp: Utc::now(),
            });
            event_bus.send(WardnetEvent::DeviceCaptureSettingsChanged {
                device_id: Uuid::parse_str(SENT).unwrap(),
                enabled: true,
                timestamp: Utc::now(),
            });
            tx.send(sample_row(Some(SENT), "sentinel.com"))
                .await
                .unwrap();
            repo.recorded_inserts()
                .await
                .iter()
                .any(|(id, _)| id == SENT)
        }
    })
    .await;
    assert!(
        sentinel_seen,
        "sentinel never captured; cannot confirm the disable was applied"
    );

    // The DEV1 row must now be dropped. A SENT barrier row behind it confirms
    // the DEV1 row was processed before we assert its absence.
    tx.send(sample_row(Some(DEV1), "should-be-skipped.com"))
        .await
        .unwrap();
    tx.send(sample_row(Some(SENT), "barrier.com"))
        .await
        .unwrap();
    assert!(
        wait_until(300, || {
            let repo = Arc::clone(&dns_repo);
            async move {
                repo.recorded_inserts()
                    .await
                    .iter()
                    .any(|(_, d)| d == "barrier.com")
            }
        })
        .await,
        "barrier row was never captured"
    );

    runner.shutdown().await;

    let inserts = dns_repo.recorded_inserts().await;
    assert!(
        !inserts.iter().any(|(_, d)| d == "should-be-skipped.com"),
        "the row for the disabled DEV1 must be dropped: {inserts:?}"
    );
}

#[tokio::test]
async fn shutdown_completes() {
    let (_tx, rx) = mpsc::channel::<QueryLogRow>(16);
    let device_service: Arc<dyn DeviceService> = Arc::new(MockDeviceService {
        enabled_ids: vec![],
        device: None,
        error_on_settings: false,
        error_on_list: false,
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
        error_on_list: false,
    });
    // Use a pruning repo on a short interval so the prune tick gives us an
    // observable heartbeat: `find_calls` advances once per loop iteration while
    // the runner is alive, and freezes once the loop exits.
    let dns_repo = PruningDnsEventsRepo::new(&[DEV1]);
    let dns_repo_dyn: Arc<dyn DnsEventsRepository> =
        Arc::clone(&dns_repo) as Arc<dyn DnsEventsRepository>;
    let events: Arc<dyn EventPublisher> = TestEventBus::new();

    let runner = DnsCaptureRunner::start_with_prune_interval(
        rx,
        device_service,
        dns_repo_dyn,
        events,
        Duration::from_millis(20),
        &tracing::Span::current(),
    );

    // Confirm the loop is alive (its prune ticker is running).
    assert!(
        wait_until(300, || {
            let repo = Arc::clone(&dns_repo);
            async move { *repo.find_calls.lock().await >= 1 }
        })
        .await,
        "runner should be ticking while its channel is open"
    );

    // Close the channel. The `capture_rx.recv() == None` arm must break the
    // whole `select!` loop, which also stops the prune ticker. Without calling
    // shutdown() (whose cancellation would mask a regression), verify the loop
    // really stopped: `find_calls` must NOT advance by two more ticks. If the
    // recv-None arm regressed to not break, the ticker keeps firing and this
    // trips within a couple of intervals.
    drop(tx);
    let baseline = *dns_repo.find_calls.lock().await;
    let kept_ticking = wait_until(50, || {
        let repo = Arc::clone(&dns_repo);
        async move { *repo.find_calls.lock().await >= baseline + 2 }
    })
    .await;
    assert!(
        !kept_ticking,
        "closing the channel must exit the runner loop, but the prune ticker kept firing"
    );

    runner.shutdown().await;
}

#[tokio::test]
async fn prune_loop_calls_prune_for_enabled_device() {
    let (_tx, rx) = mpsc::channel::<QueryLogRow>(16);
    let device_service: Arc<dyn DeviceService> = Arc::new(MockDeviceService {
        enabled_ids: vec![DEV1.to_owned()],
        device: Some(sample_device(true)),
        error_on_settings: false,
        error_on_list: false,
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

    assert!(
        wait_until(300, || {
            let repo = Arc::clone(&dns_repo);
            async move { !repo.prune_calls.lock().await.is_empty() }
        })
        .await,
        "expected prune_for_device to be called at least once"
    );
    runner.shutdown().await;

    let prune_calls = dns_repo.prune_calls.lock().await;
    assert_eq!(prune_calls[0], DEV1);
}

#[tokio::test]
async fn prune_loop_deletes_data_for_disabled_device() {
    let (_tx, rx) = mpsc::channel::<QueryLogRow>(16);
    let device_service: Arc<dyn DeviceService> = Arc::new(MockDeviceService {
        enabled_ids: vec![],
        device: Some(sample_device(false)),
        error_on_settings: false,
        error_on_list: false,
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

    assert!(
        wait_until(300, || {
            let repo = Arc::clone(&dns_repo);
            async move { !repo.delete_calls.lock().await.is_empty() }
        })
        .await,
        "expected delete_all_for_device to be called for disabled device"
    );
    runner.shutdown().await;

    let delete_calls = dns_repo.delete_calls.lock().await;
    assert_eq!(delete_calls[0], DEV1);
}

#[tokio::test]
async fn prune_loop_deletes_data_for_unknown_device() {
    let (_tx, rx) = mpsc::channel::<QueryLogRow>(16);
    let device_service: Arc<dyn DeviceService> = Arc::new(MockDeviceService {
        enabled_ids: vec![],
        device: None, // device has been deleted from the DB
        error_on_settings: false,
        error_on_list: false,
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

    assert!(
        wait_until(300, || {
            let repo = Arc::clone(&dns_repo);
            async move { !repo.delete_calls.lock().await.is_empty() }
        })
        .await,
        "expected delete_all_for_device to be called for unknown/deleted device"
    );
    runner.shutdown().await;

    let delete_calls = dns_repo.delete_calls.lock().await;
    assert_eq!(delete_calls[0], DEV1);
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
        error_on_list: false,
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

    // Wait until the prune loop has ticked at least twice (each tick calls
    // find_device_ids_with_data once), so we know the error path ran and settled
    // before asserting it produced no prune/delete calls.
    assert!(
        wait_until(300, || {
            let repo = Arc::clone(&dns_repo);
            async move { *repo.find_calls.lock().await >= 2 }
        })
        .await,
        "prune loop should have ticked at least twice"
    );
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
