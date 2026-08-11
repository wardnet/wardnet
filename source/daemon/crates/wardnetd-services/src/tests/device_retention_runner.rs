//! Tests for [`DeviceRetentionRunner`] (issue #1181).
//!
//! Mirrors `db_maintenance_runner`'s shape: the runner's job is purely
//! *cadence* — once per calendar day, responsive to cancellation — so these
//! drive the day-rollover gate and the failure tolerance. What the prune
//! actually deletes is covered in `device::tests::discovery`.

use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use async_trait::async_trait;
use uuid::Uuid;
use wardnet_common::auth::AuthContext;
use wardnet_common::device::{Device, DeviceType};

use crate::device::discovery::{DeviceDiscoveryService, ObservationResult};
use crate::device::packet_capture::ObservedDevice;
use crate::device::retention_runner::{DeviceRetentionRunner, prune};
use crate::error::AppError;

// ── Mock discovery service ───────────────────────────────────────────────────

/// A `DeviceDiscoveryService` that counts prune calls, records the auth context
/// each ran under, and can be made to fail. Every other method is
/// `unimplemented!()` — the runner must touch nothing else.
struct MockDiscovery {
    fail: bool,
    calls: Mutex<u32>,
    /// Whether every prune so far saw an admin context. The runner is
    /// responsible for supplying one; the service's own `require_admin` would
    /// otherwise reject it.
    always_admin: Mutex<bool>,
}

impl MockDiscovery {
    fn ok() -> Arc<Self> {
        Arc::new(Self {
            fail: false,
            calls: Mutex::new(0),
            always_admin: Mutex::new(true),
        })
    }

    fn err() -> Arc<Self> {
        Arc::new(Self {
            fail: true,
            calls: Mutex::new(0),
            always_admin: Mutex::new(true),
        })
    }

    fn call_count(&self) -> u32 {
        *self.calls.lock().unwrap()
    }

    fn always_admin(&self) -> bool {
        *self.always_admin.lock().unwrap()
    }
}

#[async_trait]
impl DeviceDiscoveryService for MockDiscovery {
    async fn prune_unmanaged_devices(&self) -> Result<u64, AppError> {
        *self.calls.lock().unwrap() += 1;
        let is_admin = crate::auth_context::try_current()
            .unwrap_or(AuthContext::Anonymous)
            .is_admin();
        if !is_admin {
            *self.always_admin.lock().unwrap() = false;
        }
        if self.fail {
            return Err(AppError::Internal(anyhow::anyhow!("synthetic prune error")));
        }
        Ok(3)
    }

    async fn restore_devices(&self) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn rebuild_trusted_subnets(&self) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn process_observation(
        &self,
        _obs: &ObservedDevice,
    ) -> Result<ObservationResult, AppError> {
        unimplemented!()
    }
    async fn flush_last_seen(&self) -> Result<u64, AppError> {
        unimplemented!()
    }
    async fn scan_departures(&self, _timeout_secs: u64) -> Result<Vec<Uuid>, AppError> {
        unimplemented!()
    }
    async fn resolve_hostname(&self, _mac: &str, _ip: &str) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn get_all_devices(&self) -> Result<Vec<Device>, AppError> {
        unimplemented!()
    }
    async fn get_device_by_id(&self, _id: Uuid) -> Result<Device, AppError> {
        unimplemented!()
    }
    async fn update_device(
        &self,
        _id: Uuid,
        _name: Option<&str>,
        _device_type: Option<DeviceType>,
    ) -> Result<Device, AppError> {
        unimplemented!()
    }
    async fn process_peer_observation(
        &self,
        _device_id: Uuid,
        _ip: &str,
    ) -> Result<ObservationResult, AppError> {
        unimplemented!()
    }
    async fn touch_peer_presence(&self, _device_id: Uuid) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn mark_peer_gone(
        &self,
        _device_id: Uuid,
        _timeout: Duration,
    ) -> Result<Option<Uuid>, AppError> {
        unimplemented!()
    }
}

fn admin_ctx() -> AuthContext {
    AuthContext::Admin {
        admin_id: Uuid::nil(),
    }
}

// ── One pass ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn prune_runs_under_an_admin_context() {
    // The service method is `require_admin`-guarded like every other, so the
    // runner has to supply the context — a background task has no session.
    let discovery = MockDiscovery::ok();
    prune(discovery.as_ref(), &admin_ctx()).await;

    assert_eq!(discovery.call_count(), 1);
    assert!(discovery.always_admin());
}

#[tokio::test]
async fn prune_swallows_errors_rather_than_panicking() {
    // A prune that cannot run is a table that keeps growing for another day,
    // not a reason to tear down the runner.
    let discovery = MockDiscovery::err();
    prune(discovery.as_ref(), &admin_ctx()).await;

    assert_eq!(discovery.call_count(), 1);
}

// ── Cadence ──────────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn runner_shuts_down_cleanly() {
    let discovery = MockDiscovery::ok();
    let runner = DeviceRetentionRunner::start(discovery, &tracing::Span::none());
    runner.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn runner_fires_on_day_rollover() {
    let discovery = MockDiscovery::ok();
    let yesterday = chrono::Utc::now().date_naive() - chrono::Days::new(1);

    let runner = DeviceRetentionRunner::start_with_interval_and_day(
        discovery.clone(),
        Duration::from_millis(20),
        yesterday,
        &tracing::Span::none(),
    );

    tokio::time::sleep(Duration::from_millis(100)).await;
    runner.shutdown().await;

    assert!(
        discovery.call_count() >= 1,
        "expected a prune on day rollover, call_count={}",
        discovery.call_count()
    );
    assert!(discovery.always_admin());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn runner_does_not_refire_within_the_same_day() {
    // Several ticks inside one calendar day must produce zero prunes — the
    // hourly tick exists to stay responsive to cancellation, not to prune
    // hourly.
    let discovery = MockDiscovery::ok();

    let runner = DeviceRetentionRunner::start_with_interval(
        discovery.clone(),
        Duration::from_millis(20),
        &tracing::Span::none(),
    );

    tokio::time::sleep(Duration::from_millis(100)).await;
    runner.shutdown().await;

    assert_eq!(
        discovery.call_count(),
        0,
        "prune must not fire when the day has not rolled over"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn runner_survives_a_failing_prune() {
    // The day-rollover gate has already advanced when the prune runs, so a
    // failure must neither panic the task nor stop later ticks — the retry
    // lands on the next calendar day.
    let discovery = MockDiscovery::err();
    let yesterday = chrono::Utc::now().date_naive() - chrono::Days::new(1);

    let runner = DeviceRetentionRunner::start_with_interval_and_day(
        discovery.clone(),
        Duration::from_millis(20),
        yesterday,
        &tracing::Span::none(),
    );

    tokio::time::sleep(Duration::from_millis(100)).await;
    // A panicked task would make this hang or the join fail.
    runner.shutdown().await;

    assert!(discovery.call_count() >= 1);
}
