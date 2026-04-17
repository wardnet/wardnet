use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;
use wardnet_common::device::{Device, DeviceType};

use crate::device_detector::DeviceDetector;
use wardnet_common::config::DetectionConfig;
use wardnetd_services::device::packet_capture::{ObservedDevice, PacketCapture, PacketSource};
use wardnetd_services::error::AppError;
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

    async fn resolve_hostname(&self, _device_id: Uuid, _ip: String) -> Result<(), AppError> {
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
