use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use async_trait::async_trait;
use uuid::Uuid;
use wardnet_common::auth::AuthContext;

use crate::auth_context;
use crate::error::AppError;
use crate::system::SystemPowerOps;
use crate::{SystemService, SystemServiceImpl};
use wardnet_common::tunnel::{Tunnel, TunnelConfig};
use wardnetd_data::repository::tunnel::TunnelRow;
use wardnetd_data::repository::{SystemConfigRepository, TunnelRepository};

// -- Mock repositories ----------------------------------------------------

/// Mock system config repository returning fixed counts.
struct MockSystemConfigRepo {
    devices: i64,
    tunnels: i64,
    db_size: u64,
}

#[async_trait]
impl SystemConfigRepository for MockSystemConfigRepo {
    async fn get(&self, _key: &str) -> anyhow::Result<Option<String>> {
        Ok(None)
    }
    async fn set(&self, _key: &str, _value: &str) -> anyhow::Result<()> {
        Ok(())
    }
    async fn device_count(&self) -> anyhow::Result<i64> {
        Ok(self.devices)
    }
    async fn tunnel_count(&self) -> anyhow::Result<i64> {
        Ok(self.tunnels)
    }
    async fn db_size_bytes(&self) -> anyhow::Result<u64> {
        Ok(self.db_size)
    }
}

/// Mock tunnel repository returning a fixed active count.
struct MockTunnelRepo {
    active: i64,
}

#[async_trait]
impl TunnelRepository for MockTunnelRepo {
    async fn find_all(&self) -> anyhow::Result<Vec<Tunnel>> {
        Ok(vec![])
    }
    async fn find_by_id(&self, _id: &str) -> anyhow::Result<Option<Tunnel>> {
        Ok(None)
    }
    async fn find_config_by_id(&self, _id: &str) -> anyhow::Result<Option<TunnelConfig>> {
        Ok(None)
    }
    async fn insert(&self, _row: &TunnelRow) -> anyhow::Result<()> {
        Ok(())
    }
    async fn update_status(&self, _id: &str, _status: &str) -> anyhow::Result<()> {
        Ok(())
    }
    async fn update_stats(
        &self,
        _id: &str,
        _tx: i64,
        _rx: i64,
        _hs: Option<&str>,
    ) -> anyhow::Result<()> {
        Ok(())
    }
    async fn delete(&self, _id: &str) -> anyhow::Result<()> {
        Ok(())
    }
    async fn next_interface_index(&self) -> anyhow::Result<i64> {
        Ok(0)
    }
    async fn count(&self) -> anyhow::Result<i64> {
        Ok(0)
    }
    async fn count_active(&self) -> anyhow::Result<i64> {
        Ok(self.active)
    }
}

// -- Helpers --------------------------------------------------------------

fn admin_ctx() -> AuthContext {
    AuthContext::Admin {
        admin_id: Uuid::new_v4(),
    }
}

/// Counts calls to `reboot()` / `poweroff()` so the request_*
/// service tests can assert the spawned task fired exactly once
/// after the grace window.
#[derive(Default)]
struct RecordingPowerOps {
    reboot_calls: AtomicUsize,
    poweroff_calls: AtomicUsize,
}

#[async_trait]
impl SystemPowerOps for RecordingPowerOps {
    async fn reboot(&self) -> Result<(), AppError> {
        self.reboot_calls.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    async fn poweroff(&self) -> Result<(), AppError> {
        self.poweroff_calls.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

fn build_service(
    devices: i64,
    tunnels: i64,
    active_tunnels: i64,
    db_size: u64,
) -> SystemServiceImpl {
    build_service_with_power_ops(
        devices,
        tunnels,
        active_tunnels,
        db_size,
        Arc::new(RecordingPowerOps::default()),
    )
}

fn build_service_with_power_ops(
    devices: i64,
    tunnels: i64,
    active_tunnels: i64,
    db_size: u64,
    power_ops: Arc<dyn SystemPowerOps>,
) -> SystemServiceImpl {
    SystemServiceImpl::new(
        Arc::new(MockSystemConfigRepo {
            devices,
            tunnels,
            db_size,
        }),
        Arc::new(MockTunnelRepo {
            active: active_tunnels,
        }),
        Instant::now(),
        tokio_util::sync::CancellationToken::new(),
        power_ops,
    )
}

// -- Tests ----------------------------------------------------------------

#[tokio::test]
async fn status_returns_correct_values() {
    let svc = build_service(5, 2, 1, 8192);

    let resp = auth_context::with_context(admin_ctx(), svc.status())
        .await
        .unwrap();
    assert_eq!(resp.version, env!("WARDNET_VERSION"));
    assert_eq!(resp.release_version, env!("WARDNET_RELEASE_VERSION"));
    assert_eq!(resp.device_count, 5);
    assert_eq!(resp.tunnel_count, 2);
    assert_eq!(resp.tunnel_active_count, 1);
    assert_eq!(resp.db_size_bytes, 8192);
    assert!(resp.uptime_seconds < 2); // just created
    assert!(resp.cpu_usage_percent >= 0.0);
    assert!(resp.memory_total_bytes > 0);
    assert!(resp.memory_used_bytes <= resp.memory_total_bytes);
}

#[tokio::test]
async fn status_version_comes_from_compile_time_env() {
    let svc = build_service(0, 0, 0, 0);

    let resp = auth_context::with_context(admin_ctx(), svc.status())
        .await
        .unwrap();
    // Both versions are compile-time constants, never "unknown".
    assert_eq!(resp.version, env!("WARDNET_VERSION"));
    assert_eq!(resp.release_version, env!("WARDNET_RELEASE_VERSION"));
}

#[tokio::test]
async fn status_anonymous_forbidden() {
    let svc = build_service(0, 0, 0, 0);
    let result = auth_context::with_context(AuthContext::Anonymous, svc.status()).await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}

#[tokio::test]
async fn status_device_forbidden() {
    let svc = build_service(0, 0, 0, 0);
    let ctx = AuthContext::Device {
        mac: "AA:BB:CC:DD:EE:01".to_owned(),
    };
    let result = auth_context::with_context(ctx, svc.status()).await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}

// -- request_reboot / request_shutdown ------------------------------------

#[tokio::test(start_paused = true)]
async fn request_reboot_invokes_power_ops_after_grace() {
    let ops = Arc::new(RecordingPowerOps::default());
    let svc = build_service_with_power_ops(0, 0, 0, 0, ops.clone());

    auth_context::with_context(admin_ctx(), svc.request_reboot())
        .await
        .unwrap();

    // Yield so the spawned task gets polled and registers its sleep
    // with the paused timer driver. Without this the timer driver
    // never sees the sleep and `advance` has nothing to wake.
    tokio::task::yield_now().await;
    assert_eq!(ops.reboot_calls.load(Ordering::SeqCst), 0);

    // Advance past the 500 ms grace window, then drive the runtime
    // a few more times so the woken task runs to completion.
    tokio::time::advance(std::time::Duration::from_millis(750)).await;
    for _ in 0..20 {
        tokio::task::yield_now().await;
        tokio::time::advance(std::time::Duration::from_millis(10)).await;
    }
    assert_eq!(ops.reboot_calls.load(Ordering::SeqCst), 1);
    assert_eq!(ops.poweroff_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test(start_paused = true)]
async fn request_shutdown_invokes_power_ops_after_grace() {
    let ops = Arc::new(RecordingPowerOps::default());
    let svc = build_service_with_power_ops(0, 0, 0, 0, ops.clone());

    auth_context::with_context(admin_ctx(), svc.request_shutdown())
        .await
        .unwrap();

    tokio::task::yield_now().await;
    assert_eq!(ops.poweroff_calls.load(Ordering::SeqCst), 0);

    tokio::time::advance(std::time::Duration::from_millis(750)).await;
    for _ in 0..20 {
        tokio::task::yield_now().await;
        tokio::time::advance(std::time::Duration::from_millis(10)).await;
    }
    assert_eq!(ops.poweroff_calls.load(Ordering::SeqCst), 1);
    assert_eq!(ops.reboot_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn request_reboot_returns_immediately() {
    // Without `start_paused`, real time progresses; the test just
    // confirms the call returns long before the 500 ms grace would
    // have elapsed (which means the response can flush before the
    // power op fires).
    let ops = Arc::new(RecordingPowerOps::default());
    let svc = build_service_with_power_ops(0, 0, 0, 0, ops);

    let started = std::time::Instant::now();
    auth_context::with_context(admin_ctx(), svc.request_reboot())
        .await
        .unwrap();
    assert!(
        started.elapsed() < std::time::Duration::from_millis(100),
        "request_reboot should return promptly, took {:?}",
        started.elapsed()
    );
}

#[tokio::test]
async fn request_reboot_anonymous_forbidden() {
    let ops = Arc::new(RecordingPowerOps::default());
    let svc = build_service_with_power_ops(0, 0, 0, 0, ops.clone());
    let result = auth_context::with_context(AuthContext::Anonymous, svc.request_reboot()).await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
    assert_eq!(ops.reboot_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn request_shutdown_anonymous_forbidden() {
    let ops = Arc::new(RecordingPowerOps::default());
    let svc = build_service_with_power_ops(0, 0, 0, 0, ops.clone());
    let result = auth_context::with_context(AuthContext::Anonymous, svc.request_shutdown()).await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
    assert_eq!(ops.poweroff_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn request_reboot_device_forbidden() {
    let ops = Arc::new(RecordingPowerOps::default());
    let svc = build_service_with_power_ops(0, 0, 0, 0, ops.clone());
    let ctx = AuthContext::Device {
        mac: "AA:BB:CC:DD:EE:01".to_owned(),
    };
    let result = auth_context::with_context(ctx, svc.request_reboot()).await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
    assert_eq!(ops.reboot_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn request_shutdown_device_forbidden() {
    let ops = Arc::new(RecordingPowerOps::default());
    let svc = build_service_with_power_ops(0, 0, 0, 0, ops.clone());
    let ctx = AuthContext::Device {
        mac: "AA:BB:CC:DD:EE:01".to_owned(),
    };
    let result = auth_context::with_context(ctx, svc.request_shutdown()).await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
    assert_eq!(ops.poweroff_calls.load(Ordering::SeqCst), 0);
}
