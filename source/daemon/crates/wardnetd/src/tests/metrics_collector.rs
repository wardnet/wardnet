use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use wardnet_common::api::SystemStatusResponse;

use crate::metrics_collector::MetricsCollector;
use wardnet_common::config::OtelMetricsConfig;
use wardnetd_services::error::AppError;
use wardnetd_services::system::SystemService;

/// Mock system service that returns fixed values.
struct MockSystemService;

#[async_trait]
impl SystemService for MockSystemService {
    fn version(&self) -> &'static str {
        "0.0.0-test"
    }
    fn uptime(&self) -> std::time::Duration {
        std::time::Duration::from_secs(42)
    }
    async fn status(&self) -> Result<SystemStatusResponse, AppError> {
        Ok(SystemStatusResponse {
            version: "0.0.0-test".to_owned(),
            uptime_seconds: 42,
            device_count: 5,
            tunnel_count: 2,
            tunnel_active_count: 1,
            db_size_bytes: 8192,
            cpu_usage_percent: 10.0,
            memory_used_bytes: 500_000_000,
            memory_total_bytes: 1_000_000_000,
        })
    }
}

/// Mock system service that always returns an error.
struct FailingSystemService;

#[async_trait]
impl SystemService for FailingSystemService {
    fn version(&self) -> &'static str {
        "0.0.0-test"
    }
    fn uptime(&self) -> std::time::Duration {
        std::time::Duration::from_secs(0)
    }
    async fn status(&self) -> Result<SystemStatusResponse, AppError> {
        Err(AppError::Internal(anyhow::anyhow!(
            "simulated service failure"
        )))
    }
}

#[tokio::test(start_paused = true)]
async fn collector_starts_and_shuts_down_cleanly() {
    let svc: Arc<dyn SystemService> = Arc::new(MockSystemService);
    let config = OtelMetricsConfig::default();

    // Use the global noop meter provider (no exporter configured).
    let meter = opentelemetry::global::meter_provider().meter("test");
    let started_at = Instant::now();
    let parent_span = tracing::info_span!("test");

    let collector = MetricsCollector::start(svc, &config, meter, started_at, 1, &parent_span);

    // Advance time past two ticks to ensure the collection loop body executes
    // at least once. The interval is 1 second so 2 seconds guarantees it.
    tokio::time::advance(Duration::from_secs(2)).await;

    // Yield to let the spawned task process the tick.
    tokio::task::yield_now().await;

    collector.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn collector_with_all_metrics_disabled() {
    let svc: Arc<dyn SystemService> = Arc::new(MockSystemService);
    let mut config = OtelMetricsConfig::default();
    config.enabled_metrics.system_cpu_utilization = false;
    config.enabled_metrics.system_memory_usage = false;
    config.enabled_metrics.system_temperature = false;
    config.enabled_metrics.system_network_io = false;
    config.enabled_metrics.wardnet_device_count = false;
    config.enabled_metrics.wardnet_tunnel_count = false;
    config.enabled_metrics.wardnet_tunnel_active_count = false;
    config.enabled_metrics.wardnet_uptime_seconds = false;
    config.enabled_metrics.wardnet_db_size_bytes = false;

    let meter = opentelemetry::global::meter_provider().meter("test");
    let started_at = Instant::now();
    let parent_span = tracing::info_span!("test");

    let collector = MetricsCollector::start(svc, &config, meter, started_at, 1, &parent_span);

    // Even with all metrics disabled the loop body should execute without
    // panicking. Advance past a tick to exercise that path.
    tokio::time::advance(Duration::from_secs(2)).await;
    tokio::task::yield_now().await;

    collector.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn collector_exercises_full_tick_with_all_metrics_enabled() {
    let svc: Arc<dyn SystemService> = Arc::new(MockSystemService);
    let config = OtelMetricsConfig::default();

    let meter = opentelemetry::global::meter_provider().meter("test-full-tick");
    let started_at = Instant::now();
    let parent_span = tracing::info_span!("test");

    let collector = MetricsCollector::start(svc, &config, meter, started_at, 1, &parent_span);

    // Advance past several ticks to exercise the collection loop body
    // multiple times, covering system metrics (CPU, memory, network) and
    // application metrics via `SystemService::status()` inside `auth_context`.
    tokio::time::advance(Duration::from_secs(3)).await;
    tokio::task::yield_now().await;

    collector.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn collector_handles_service_error_gracefully() {
    let svc: Arc<dyn SystemService> = Arc::new(FailingSystemService);
    let config = OtelMetricsConfig::default();

    let meter = opentelemetry::global::meter_provider().meter("test-failing");
    let started_at = Instant::now();
    let parent_span = tracing::info_span!("test");

    let collector = MetricsCollector::start(svc, &config, meter, started_at, 1, &parent_span);

    // The loop should log the error from `system_service.status()` and
    // continue without panicking. Advance past a tick to trigger it.
    tokio::time::advance(Duration::from_secs(2)).await;
    tokio::task::yield_now().await;

    collector.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn collector_only_app_metrics_enabled() {
    let svc: Arc<dyn SystemService> = Arc::new(MockSystemService);
    let mut config = OtelMetricsConfig::default();
    // Disable all system metrics, keep only application metrics.
    config.enabled_metrics.system_cpu_utilization = false;
    config.enabled_metrics.system_memory_usage = false;
    config.enabled_metrics.system_temperature = false;
    config.enabled_metrics.system_network_io = false;

    let meter = opentelemetry::global::meter_provider().meter("test-app-only");
    let started_at = Instant::now();
    let parent_span = tracing::info_span!("test");

    let collector = MetricsCollector::start(svc, &config, meter, started_at, 1, &parent_span);

    // This exercises the `needs_app_metrics` branch (which wraps the
    // `system_service.status()` call in `auth_context::with_context`).
    tokio::time::advance(Duration::from_secs(2)).await;
    tokio::task::yield_now().await;

    collector.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn collector_only_system_metrics_enabled() {
    let svc: Arc<dyn SystemService> = Arc::new(MockSystemService);
    let mut config = OtelMetricsConfig::default();
    // Disable all application metrics, keep only system metrics.
    config.enabled_metrics.wardnet_device_count = false;
    config.enabled_metrics.wardnet_tunnel_count = false;
    config.enabled_metrics.wardnet_tunnel_active_count = false;
    config.enabled_metrics.wardnet_uptime_seconds = false;
    config.enabled_metrics.wardnet_db_size_bytes = false;

    let meter = opentelemetry::global::meter_provider().meter("test-system-only");
    let started_at = Instant::now();
    let parent_span = tracing::info_span!("test");

    let collector = MetricsCollector::start(svc, &config, meter, started_at, 1, &parent_span);

    // With no app metrics, `needs_app_metrics` is false and the
    // `auth_context::with_context` call is skipped entirely.
    tokio::time::advance(Duration::from_secs(2)).await;
    tokio::task::yield_now().await;

    collector.shutdown().await;
}
