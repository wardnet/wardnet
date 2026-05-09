use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;
use wardnet_common::device::{Device, DeviceType};
use wardnet_common::event::WardnetEvent;

use crate::device_detector::DeviceDetector;
use std::sync::Mutex;
use wardnet_common::config::DetectionConfig;
use wardnetd_data::repository::SystemConfigRepository;
use wardnetd_services::device::packet_capture::{ObservedDevice, PacketCapture, PacketSource};
use wardnetd_services::error::AppError;
use wardnetd_services::event::{BroadcastEventBus, EventPublisher};
use wardnetd_services::{DeviceDiscoveryService, ObservationResult};

// ---------------------------------------------------------------------------
// Mock: PacketCapture
// ---------------------------------------------------------------------------

/// Mock packet capture that optionally sends observations and tracks calls.
struct MockCapture {
    /// Number of times `arp_scan` was called.
    arp_scan_count: Arc<AtomicUsize>,
    /// If set, `capture_loop` returns this error immediately.
    capture_error: Option<String>,
    /// If set, `arp_scan` returns this error.
    arp_scan_error: Option<String>,
}

impl MockCapture {
    fn new(arp_scan_count: Arc<AtomicUsize>) -> Self {
        Self {
            arp_scan_count,
            capture_error: None,
            arp_scan_error: None,
        }
    }

    fn with_capture_error(mut self, msg: &str) -> Self {
        self.capture_error = Some(msg.to_owned());
        self
    }

    fn with_arp_scan_error(mut self, msg: &str) -> Self {
        self.arp_scan_error = Some(msg.to_owned());
        self
    }
}

#[async_trait]
impl PacketCapture for MockCapture {
    async fn capture_loop(
        &self,
        _interface: &str,
        _sender: mpsc::Sender<ObservedDevice>,
        cancel: CancellationToken,
    ) -> anyhow::Result<()> {
        if let Some(ref msg) = self.capture_error {
            return Err(anyhow::anyhow!("{msg}"));
        }
        // Block until cancelled.
        cancel.cancelled().await;
        Ok(())
    }

    async fn arp_scan(&self, _interface: &str) -> anyhow::Result<()> {
        self.arp_scan_count.fetch_add(1, Ordering::SeqCst);
        if let Some(ref msg) = self.arp_scan_error {
            return Err(anyhow::anyhow!("{msg}"));
        }
        Ok(())
    }
}

/// Mock capture that sends a single observation then waits for cancellation.
struct SingleObservationCapture {
    obs: ObservedDevice,
}

#[async_trait]
impl PacketCapture for SingleObservationCapture {
    async fn capture_loop(
        &self,
        _interface: &str,
        sender: mpsc::Sender<ObservedDevice>,
        cancel: CancellationToken,
    ) -> anyhow::Result<()> {
        let _ = sender.send(self.obs.clone()).await;
        cancel.cancelled().await;
        Ok(())
    }

    async fn arp_scan(&self, _interface: &str) -> anyhow::Result<()> {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Mock: DeviceDiscoveryService
// ---------------------------------------------------------------------------

/// Mock discovery service that records calls and returns configurable results.
struct MockDiscovery {
    /// Number of times `process_observation` was called.
    process_count: Arc<AtomicUsize>,
    /// Number of times `flush_last_seen` was called.
    flush_count: Arc<AtomicUsize>,
    /// Number of times `scan_departures` was called.
    departure_count: Arc<AtomicUsize>,
    /// Number of times `resolve_hostname` was called.
    resolve_count: Arc<AtomicUsize>,
    /// Result to return from `process_observation`.
    observation_result: ObservationResultFactory,
}

/// Factory for generating observation results.
enum ObservationResultFactory {
    NewDevice,
    IpChanged,
    Reappeared,
    Seen,
    Error,
}

impl MockDiscovery {
    fn new(factory: ObservationResultFactory) -> Self {
        Self {
            process_count: Arc::new(AtomicUsize::new(0)),
            flush_count: Arc::new(AtomicUsize::new(0)),
            departure_count: Arc::new(AtomicUsize::new(0)),
            resolve_count: Arc::new(AtomicUsize::new(0)),
            observation_result: factory,
        }
    }
}

#[async_trait]
impl DeviceDiscoveryService for MockDiscovery {
    async fn restore_devices(&self) -> Result<(), AppError> {
        Ok(())
    }

    async fn process_observation(
        &self,
        _obs: &ObservedDevice,
    ) -> Result<ObservationResult, AppError> {
        self.process_count.fetch_add(1, Ordering::SeqCst);
        match self.observation_result {
            ObservationResultFactory::NewDevice => Ok(ObservationResult::NewDevice {
                device_id: Uuid::nil(),
                manufacturer: None,
                device_type: DeviceType::Unknown,
            }),
            ObservationResultFactory::IpChanged => Ok(ObservationResult::IpChanged {
                device_id: Uuid::nil(),
                old_ip: "10.0.0.1".to_owned(),
            }),
            ObservationResultFactory::Reappeared => Ok(ObservationResult::Reappeared(Uuid::nil())),
            ObservationResultFactory::Seen => Ok(ObservationResult::Seen(Uuid::nil())),
            ObservationResultFactory::Error => {
                Err(AppError::Internal(anyhow::anyhow!("mock error")))
            }
        }
    }

    async fn flush_last_seen(&self) -> Result<u64, AppError> {
        self.flush_count.fetch_add(1, Ordering::SeqCst);
        Ok(0)
    }

    async fn scan_departures(&self, _timeout_secs: u64) -> Result<Vec<Uuid>, AppError> {
        self.departure_count.fetch_add(1, Ordering::SeqCst);
        Ok(vec![])
    }

    async fn resolve_hostname(&self, _mac: &str, _ip: &str) -> Result<(), AppError> {
        self.resolve_count.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    async fn get_all_devices(&self) -> Result<Vec<Device>, AppError> {
        Ok(vec![])
    }

    async fn get_device_by_id(&self, _id: Uuid) -> Result<Device, AppError> {
        Err(AppError::NotFound("mock".to_owned()))
    }

    async fn update_device(
        &self,
        _id: Uuid,
        _name: Option<&str>,
        _device_type: Option<DeviceType>,
    ) -> Result<Device, AppError> {
        Err(AppError::NotFound("mock".to_owned()))
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Empty in-memory `SystemConfigRepository` stub. The `garp_learning`
/// hook is exercised by its own dedicated tests; this stub just
/// satisfies the trait bound so `DeviceDetector::start` accepts it.
#[derive(Default)]
struct StubSystemConfig {
    data: Mutex<HashMap<String, String>>,
}

#[async_trait]
impl SystemConfigRepository for StubSystemConfig {
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

fn stub_system_config() -> Arc<dyn SystemConfigRepository> {
    Arc::new(StubSystemConfig::default())
}

/// Build a fast detection config with 1-second intervals for tests.
fn fast_config() -> DetectionConfig {
    DetectionConfig {
        enabled: true,
        departure_timeout_secs: 1,
        batch_flush_interval_secs: 1,
        departure_scan_interval_secs: 1,
        arp_scan_interval_secs: 1,
    }
}

/// Build a sample observation for tests.
fn sample_observation() -> ObservedDevice {
    ObservedDevice {
        mac: "AA:BB:CC:DD:EE:FF".to_owned(),
        ip: "192.168.1.42".to_owned(),
        source: PacketSource::Arp,
    }
}

/// Root span for tests.
fn test_span() -> tracing::Span {
    tracing::info_span!("test")
}

/// In-memory event bus the detector can subscribe to without external state.
fn test_events() -> BroadcastEventBus {
    BroadcastEventBus::new(16)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn start_and_shutdown() {
    let arp_count = Arc::new(AtomicUsize::new(0));
    let capture: Arc<dyn PacketCapture> = Arc::new(MockCapture::new(arp_count));
    let discovery: Arc<dyn DeviceDiscoveryService> =
        Arc::new(MockDiscovery::new(ObservationResultFactory::Seen));

    let detector = DeviceDetector::start(
        capture,
        discovery,
        stub_system_config(),
        &test_events(),
        &fast_config(),
        "eth0".to_owned(),
        &test_span(),
    );

    // Shutdown should complete without hanging or panicking.
    detector.shutdown().await;
}

#[tokio::test]
async fn processor_handles_new_device() {
    let discovery = Arc::new(MockDiscovery::new(ObservationResultFactory::NewDevice));
    let capture: Arc<dyn PacketCapture> = Arc::new(SingleObservationCapture {
        obs: sample_observation(),
    });

    let process_count = discovery.process_count.clone();
    let detector = DeviceDetector::start(
        capture,
        discovery as Arc<dyn DeviceDiscoveryService>,
        stub_system_config(),
        &test_events(),
        &fast_config(),
        "eth0".to_owned(),
        &test_span(),
    );

    // Wait for the observation to be processed.
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    assert!(
        process_count.load(Ordering::SeqCst) >= 1,
        "process_observation should have been called at least once"
    );

    detector.shutdown().await;
}

#[tokio::test]
async fn processor_handles_ip_changed() {
    let discovery = Arc::new(MockDiscovery::new(ObservationResultFactory::IpChanged));
    let capture: Arc<dyn PacketCapture> = Arc::new(SingleObservationCapture {
        obs: sample_observation(),
    });

    let process_count = discovery.process_count.clone();
    let detector = DeviceDetector::start(
        capture,
        discovery as Arc<dyn DeviceDiscoveryService>,
        stub_system_config(),
        &test_events(),
        &fast_config(),
        "eth0".to_owned(),
        &test_span(),
    );

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    assert!(
        process_count.load(Ordering::SeqCst) >= 1,
        "process_observation should have been called for IpChanged"
    );

    detector.shutdown().await;
}

#[tokio::test]
async fn processor_handles_reappeared() {
    let discovery = Arc::new(MockDiscovery::new(ObservationResultFactory::Reappeared));
    let capture: Arc<dyn PacketCapture> = Arc::new(SingleObservationCapture {
        obs: sample_observation(),
    });

    let process_count = discovery.process_count.clone();
    let detector = DeviceDetector::start(
        capture,
        discovery as Arc<dyn DeviceDiscoveryService>,
        stub_system_config(),
        &test_events(),
        &fast_config(),
        "eth0".to_owned(),
        &test_span(),
    );

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    assert!(
        process_count.load(Ordering::SeqCst) >= 1,
        "process_observation should have been called for Reappeared"
    );

    detector.shutdown().await;
}

#[tokio::test]
async fn processor_handles_error() {
    let discovery = Arc::new(MockDiscovery::new(ObservationResultFactory::Error));
    let capture: Arc<dyn PacketCapture> = Arc::new(SingleObservationCapture {
        obs: sample_observation(),
    });

    let process_count = discovery.process_count.clone();
    let detector = DeviceDetector::start(
        capture,
        discovery as Arc<dyn DeviceDiscoveryService>,
        stub_system_config(),
        &test_events(),
        &fast_config(),
        "eth0".to_owned(),
        &test_span(),
    );

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    assert!(
        process_count.load(Ordering::SeqCst) >= 1,
        "process_observation should have been called even when returning Err"
    );

    // The detector should not crash; shutdown should complete cleanly.
    detector.shutdown().await;
}

#[tokio::test]
async fn capture_task_logs_error_on_failure() {
    let arp_count = Arc::new(AtomicUsize::new(0));
    let capture: Arc<dyn PacketCapture> =
        Arc::new(MockCapture::new(arp_count).with_capture_error("pcap open failed"));
    let discovery: Arc<dyn DeviceDiscoveryService> =
        Arc::new(MockDiscovery::new(ObservationResultFactory::Seen));

    let detector = DeviceDetector::start(
        capture,
        discovery,
        stub_system_config(),
        &test_events(),
        &fast_config(),
        "eth0".to_owned(),
        &test_span(),
    );

    // Give the capture task time to fail and log.
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Should not panic; shutdown should complete.
    detector.shutdown().await;
}

#[tokio::test]
async fn flush_task_runs_and_cancels() {
    let arp_count = Arc::new(AtomicUsize::new(0));
    let capture: Arc<dyn PacketCapture> = Arc::new(MockCapture::new(arp_count));
    let discovery = Arc::new(MockDiscovery::new(ObservationResultFactory::Seen));
    let flush_count = discovery.flush_count.clone();

    let detector = DeviceDetector::start(
        capture,
        discovery as Arc<dyn DeviceDiscoveryService>,
        stub_system_config(),
        &test_events(),
        &fast_config(),
        "eth0".to_owned(),
        &test_span(),
    );

    // tokio::time::interval fires immediately on first tick, so flush should
    // be called at least once within a short window.
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    assert!(
        flush_count.load(Ordering::SeqCst) >= 1,
        "flush_last_seen should have been called at least once"
    );

    detector.shutdown().await;
}

#[tokio::test]
async fn departure_task_runs_and_cancels() {
    let arp_count = Arc::new(AtomicUsize::new(0));
    let capture: Arc<dyn PacketCapture> = Arc::new(MockCapture::new(arp_count));
    let discovery = Arc::new(MockDiscovery::new(ObservationResultFactory::Seen));
    let departure_count = discovery.departure_count.clone();

    let detector = DeviceDetector::start(
        capture,
        discovery as Arc<dyn DeviceDiscoveryService>,
        stub_system_config(),
        &test_events(),
        &fast_config(),
        "eth0".to_owned(),
        &test_span(),
    );

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    assert!(
        departure_count.load(Ordering::SeqCst) >= 1,
        "scan_departures should have been called at least once"
    );

    detector.shutdown().await;
}

#[tokio::test]
async fn arp_scan_task_runs_and_cancels() {
    let arp_count = Arc::new(AtomicUsize::new(0));
    let capture: Arc<dyn PacketCapture> = Arc::new(MockCapture::new(arp_count.clone()));
    let discovery: Arc<dyn DeviceDiscoveryService> =
        Arc::new(MockDiscovery::new(ObservationResultFactory::Seen));

    let detector = DeviceDetector::start(
        capture,
        discovery,
        stub_system_config(),
        &test_events(),
        &fast_config(),
        "eth0".to_owned(),
        &test_span(),
    );

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    assert!(
        arp_count.load(Ordering::SeqCst) >= 1,
        "arp_scan should have been called at least once"
    );

    detector.shutdown().await;
}

#[tokio::test]
async fn arp_scan_task_handles_error() {
    let arp_count = Arc::new(AtomicUsize::new(0));
    let capture: Arc<dyn PacketCapture> =
        Arc::new(MockCapture::new(arp_count.clone()).with_arp_scan_error("scan failed"));
    let discovery: Arc<dyn DeviceDiscoveryService> =
        Arc::new(MockDiscovery::new(ObservationResultFactory::Seen));

    let detector = DeviceDetector::start(
        capture,
        discovery,
        stub_system_config(),
        &test_events(),
        &fast_config(),
        "eth0".to_owned(),
        &test_span(),
    );

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    assert!(
        arp_count.load(Ordering::SeqCst) >= 1,
        "arp_scan should have been called even when returning error"
    );

    // Should not crash; shutdown completes cleanly.
    detector.shutdown().await;
}

// ---------------------------------------------------------------------------
// Hostname listener tests
// ---------------------------------------------------------------------------

/// Build a detector that consumes from `events` so tests can publish their
/// own DHCP lease events into the listener task.
fn detector_with_event_bus(
    discovery: Arc<dyn DeviceDiscoveryService>,
    events: &BroadcastEventBus,
) -> DeviceDetector {
    let arp_count = Arc::new(AtomicUsize::new(0));
    let capture: Arc<dyn PacketCapture> = Arc::new(MockCapture::new(arp_count));
    DeviceDetector::start(
        capture,
        discovery,
        stub_system_config(),
        events,
        &fast_config(),
        "eth0".to_owned(),
        &test_span(),
    )
}

fn assigned_with_hostname(hostname: Option<&str>) -> WardnetEvent {
    WardnetEvent::DhcpLeaseAssigned {
        lease_id: Uuid::nil(),
        mac: "aa:bb:cc:dd:ee:01".to_owned(),
        ip: "192.168.1.10".to_owned(),
        hostname: hostname.map(ToOwned::to_owned),
        timestamp: chrono::Utc::now(),
    }
}

fn renewed_with_hostname(hostname: Option<&str>) -> WardnetEvent {
    WardnetEvent::DhcpLeaseRenewed {
        lease_id: Uuid::nil(),
        mac: "aa:bb:cc:dd:ee:01".to_owned(),
        ip: "192.168.1.10".to_owned(),
        hostname: hostname.map(ToOwned::to_owned),
        new_expiry: chrono::Utc::now(),
        timestamp: chrono::Utc::now(),
    }
}

/// Wait briefly for the spawned listener to consume an event before asserting,
/// since `publish` returns immediately while the listener task is async.
async fn drain_event_bus() {
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
}

#[tokio::test]
async fn hostname_listener_resolves_on_lease_assigned_with_hostname() {
    let discovery = Arc::new(MockDiscovery::new(ObservationResultFactory::Seen));
    let resolve_count = discovery.resolve_count.clone();
    let events = test_events();

    let detector = detector_with_event_bus(discovery as Arc<dyn DeviceDiscoveryService>, &events);

    events.publish(assigned_with_hostname(Some("kitchen-tablet")));
    drain_event_bus().await;

    assert_eq!(
        resolve_count.load(Ordering::SeqCst),
        1,
        "non-empty hostname on DhcpLeaseAssigned must trigger resolve_hostname"
    );

    detector.shutdown().await;
}

#[tokio::test]
async fn hostname_listener_resolves_on_lease_renewed_with_hostname() {
    let discovery = Arc::new(MockDiscovery::new(ObservationResultFactory::Seen));
    let resolve_count = discovery.resolve_count.clone();
    let events = test_events();

    let detector = detector_with_event_bus(discovery as Arc<dyn DeviceDiscoveryService>, &events);

    events.publish(renewed_with_hostname(Some("kitchen-tablet")));
    drain_event_bus().await;

    assert_eq!(resolve_count.load(Ordering::SeqCst), 1);

    detector.shutdown().await;
}

#[tokio::test]
async fn hostname_listener_skips_when_event_hostname_is_none() {
    let discovery = Arc::new(MockDiscovery::new(ObservationResultFactory::Seen));
    let resolve_count = discovery.resolve_count.clone();
    let events = test_events();

    let detector = detector_with_event_bus(discovery as Arc<dyn DeviceDiscoveryService>, &events);

    events.publish(assigned_with_hostname(None));
    events.publish(renewed_with_hostname(None));
    drain_event_bus().await;

    assert_eq!(
        resolve_count.load(Ordering::SeqCst),
        0,
        "events without a hostname must not trigger resolve_hostname"
    );

    detector.shutdown().await;
}

#[tokio::test]
async fn hostname_listener_skips_when_event_hostname_is_whitespace() {
    let discovery = Arc::new(MockDiscovery::new(ObservationResultFactory::Seen));
    let resolve_count = discovery.resolve_count.clone();
    let events = test_events();

    let detector = detector_with_event_bus(discovery as Arc<dyn DeviceDiscoveryService>, &events);

    events.publish(assigned_with_hostname(Some("   ")));
    drain_event_bus().await;

    assert_eq!(resolve_count.load(Ordering::SeqCst), 0);

    detector.shutdown().await;
}

#[tokio::test]
async fn hostname_listener_ignores_unrelated_events() {
    let discovery = Arc::new(MockDiscovery::new(ObservationResultFactory::Seen));
    let resolve_count = discovery.resolve_count.clone();
    let events = test_events();

    let detector = detector_with_event_bus(discovery as Arc<dyn DeviceDiscoveryService>, &events);

    // Random unrelated event — listener must ignore it cleanly.
    events.publish(WardnetEvent::DeviceGone {
        device_id: Uuid::nil(),
        mac: "aa:bb:cc:dd:ee:01".to_owned(),
        last_ip: "192.168.1.10".to_owned(),
        timestamp: chrono::Utc::now(),
    });
    drain_event_bus().await;

    assert_eq!(resolve_count.load(Ordering::SeqCst), 0);

    detector.shutdown().await;
}
