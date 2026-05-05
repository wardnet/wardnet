//! Tests for [`TunnelMetricsRunner`].
//!
//! The runner is a thin loop around `run_metrics_maintenance`, so the
//! tests just verify it calls the service on every tick and that
//! `shutdown` actually stops the loop. The rollup state machine itself
//! is covered by the service tests.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use uuid::Uuid;
use wardnet_common::api::{
    CreateTunnelRequest, CreateTunnelResponse, DeleteTunnelResponse, ListTunnelsResponse,
    TunnelDevicesResponse, TunnelMetricsRange, TunnelMetricsResponse,
};
use wardnet_common::tunnel::Tunnel;

use crate::error::AppError;
use crate::tunnel::metrics_runner::{DEFAULT_ROLLUP_INTERVAL, TunnelMetricsRunner};
use crate::tunnel::service::TunnelService;

/// Counts `run_metrics_maintenance` calls; every other method panics so
/// tests fail loudly if the runner starts calling something it shouldn't.
struct CountingTunnelService {
    maintenance_calls: AtomicU32,
}

impl CountingTunnelService {
    fn new() -> Self {
        Self {
            maintenance_calls: AtomicU32::new(0),
        }
    }
}

#[async_trait]
impl TunnelService for CountingTunnelService {
    async fn import_tunnel(
        &self,
        _r: CreateTunnelRequest,
    ) -> Result<CreateTunnelResponse, AppError> {
        unimplemented!()
    }
    async fn list_tunnels(&self) -> Result<ListTunnelsResponse, AppError> {
        unimplemented!()
    }
    async fn get_tunnel(&self, _id: Uuid) -> Result<Tunnel, AppError> {
        unimplemented!()
    }
    async fn get_metrics(
        &self,
        _id: Uuid,
        _range: TunnelMetricsRange,
    ) -> Result<TunnelMetricsResponse, AppError> {
        unimplemented!()
    }
    async fn list_tunnel_devices(&self, _id: Uuid) -> Result<TunnelDevicesResponse, AppError> {
        unimplemented!()
    }
    async fn bring_up(&self, _id: Uuid) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn tear_down(&self, _id: Uuid, _r: &str) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn bring_up_internal(&self, _id: Uuid) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn tear_down_internal(&self, _id: Uuid, _r: &str) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn delete_tunnel(&self, _id: Uuid) -> Result<DeleteTunnelResponse, AppError> {
        unimplemented!()
    }
    async fn restore_tunnels(&self) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn collect_stats(&self) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn run_health_check(&self) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn run_metrics_maintenance(&self) -> Result<(), AppError> {
        self.maintenance_calls.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

#[tokio::test]
async fn runner_fires_maintenance_on_startup() {
    let svc = Arc::new(CountingTunnelService::new());
    let parent = tracing::info_span!("wardnetd-test");

    let runner = TunnelMetricsRunner::start(
        svc.clone() as Arc<dyn TunnelService>,
        Duration::from_hours(1),
        &parent,
    );

    // Let the initial tick complete. The runner calls maintenance once
    // immediately on start, then waits for the next interval.
    for _ in 0..50 {
        if svc.maintenance_calls.load(Ordering::SeqCst) > 0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(svc.maintenance_calls.load(Ordering::SeqCst), 1);

    runner.shutdown().await;
}

#[tokio::test]
async fn runner_shuts_down_cleanly_without_tick() {
    // Even if the operator shuts down immediately after start, before
    // the initial tick finishes, the runner must not hang.
    let svc = Arc::new(CountingTunnelService::new());
    let parent = tracing::info_span!("wardnetd-test");

    let runner = TunnelMetricsRunner::start(
        svc.clone() as Arc<dyn TunnelService>,
        Duration::from_hours(1),
        &parent,
    );

    runner.shutdown().await;
}

#[tokio::test]
async fn runner_swallows_maintenance_errors() {
    // `perform_tick` logs and discards errors so a single failing tick
    // doesn't kill the loop. Verify that even if maintenance returns
    // `Err`, the runner stays alive and shutdown still works.
    struct FailingService;

    #[async_trait]
    impl TunnelService for FailingService {
        async fn import_tunnel(
            &self,
            _r: CreateTunnelRequest,
        ) -> Result<CreateTunnelResponse, AppError> {
            unimplemented!()
        }
        async fn list_tunnels(&self) -> Result<ListTunnelsResponse, AppError> {
            unimplemented!()
        }
        async fn get_tunnel(&self, _id: Uuid) -> Result<Tunnel, AppError> {
            unimplemented!()
        }
        async fn get_metrics(
            &self,
            _id: Uuid,
            _range: TunnelMetricsRange,
        ) -> Result<TunnelMetricsResponse, AppError> {
            unimplemented!()
        }
        async fn list_tunnel_devices(&self, _id: Uuid) -> Result<TunnelDevicesResponse, AppError> {
            unimplemented!()
        }
        async fn bring_up(&self, _id: Uuid) -> Result<(), AppError> {
            unimplemented!()
        }
        async fn tear_down(&self, _id: Uuid, _r: &str) -> Result<(), AppError> {
            unimplemented!()
        }
        async fn bring_up_internal(&self, _id: Uuid) -> Result<(), AppError> {
            unimplemented!()
        }
        async fn tear_down_internal(&self, _id: Uuid, _r: &str) -> Result<(), AppError> {
            unimplemented!()
        }
        async fn delete_tunnel(&self, _id: Uuid) -> Result<DeleteTunnelResponse, AppError> {
            unimplemented!()
        }
        async fn restore_tunnels(&self) -> Result<(), AppError> {
            unimplemented!()
        }
        async fn collect_stats(&self) -> Result<(), AppError> {
            unimplemented!()
        }
        async fn run_health_check(&self) -> Result<(), AppError> {
            unimplemented!()
        }
        async fn run_metrics_maintenance(&self) -> Result<(), AppError> {
            Err(AppError::Internal(anyhow::anyhow!(
                "simulated rollup failure"
            )))
        }
    }

    let svc = Arc::new(FailingService);
    let parent = tracing::info_span!("wardnetd-test");

    let runner = TunnelMetricsRunner::start(
        svc as Arc<dyn TunnelService>,
        Duration::from_hours(1),
        &parent,
    );

    // Give the runner enough time to execute the failing initial tick;
    // it should log and continue rather than panic.
    tokio::time::sleep(Duration::from_millis(50)).await;
    runner.shutdown().await;
}

#[test]
fn default_rollup_interval_is_one_hour() {
    assert_eq!(DEFAULT_ROLLUP_INTERVAL, Duration::from_hours(1));
}
