use std::sync::Arc;
use std::time::{Duration, Instant};

use opentelemetry::KeyValue;
use opentelemetry::metrics::{Gauge, Meter};
use sysinfo::{Components, Networks, System};
use tokio::time::interval;
use tokio_util::sync::CancellationToken;
use tracing::Instrument;
use wardnet_common::auth::AuthContext;

use wardnet_common::config::OtelMetricsConfig;
use wardnetd_services::auth_context;
use wardnetd_services::system::SystemService;

/// Background task that periodically collects system and application metrics
/// and records them via OpenTelemetry instruments.
pub struct MetricsCollector {
    cancel: CancellationToken,
    handle: tokio::task::JoinHandle<()>,
}

impl MetricsCollector {
    /// Start the metrics collection loop in a background task.
    ///
    /// The collector records system-level gauges (CPU, memory, temperature,
    /// network I/O) and wardnet application metrics (device count, tunnel
    /// count, uptime, database size) at the configured interval.
    pub fn start(
        system_service: Arc<dyn SystemService>,
        config: &OtelMetricsConfig,
        meter: Meter,
        started_at: Instant,
        interval_secs: u64,
        parent: &tracing::Span,
    ) -> Self {
        let cancel = CancellationToken::new();
        let span = tracing::info_span!(parent: parent, "metrics_collector");
        let config = config.clone();

        let handle = tokio::spawn(
            collection_loop(
                system_service,
                config,
                meter,
                started_at,
                interval_secs,
                cancel.clone(),
            )
            .instrument(span),
        );

        Self { cancel, handle }
    }

    /// Cancel the background task and wait for it to finish.
    pub async fn shutdown(self) {
        self.cancel.cancel();
        let _ = self.handle.await;
        tracing::info!("metrics collector shut down");
    }
}

/// Instruments created for enabled metrics.
struct Instruments {
    cpu_utilization: Option<Gauge<f64>>,
    memory_usage: Option<Gauge<u64>>,
    temperature: Option<Gauge<f64>>,
    network_io: Option<Gauge<u64>>,
    device_count: Option<Gauge<u64>>,
    tunnel_count: Option<Gauge<u64>>,
    tunnel_active_count: Option<Gauge<u64>>,
    uptime_seconds: Option<Gauge<u64>>,
    db_size_bytes: Option<Gauge<u64>>,
}

impl Instruments {
    fn new(meter: &Meter, config: &OtelMetricsConfig) -> Self {
        let em = &config.enabled_metrics;
        Self {
            cpu_utilization: em.system_cpu_utilization.then(|| {
                meter
                    .f64_gauge("system.cpu.utilization")
                    .with_description("CPU utilization as a fraction (0.0 to 1.0)")
                    .with_unit("1")
                    .build()
            }),
            memory_usage: em.system_memory_usage.then(|| {
                meter
                    .u64_gauge("system.memory.usage")
                    .with_description("Used memory in bytes")
                    .with_unit("By")
                    .build()
            }),
            temperature: em.system_temperature.then(|| {
                meter
                    .f64_gauge("system.temperature")
                    .with_description("System temperature in degrees Celsius")
                    .with_unit("Cel")
                    .build()
            }),
            network_io: em.system_network_io.then(|| {
                meter
                    .u64_gauge("system.network.io")
                    .with_description("Cumulative network I/O in bytes")
                    .with_unit("By")
                    .build()
            }),
            device_count: em.wardnet_device_count.then(|| {
                meter
                    .u64_gauge("wardnet.device_count")
                    .with_description("Total number of known devices")
                    .build()
            }),
            tunnel_count: em.wardnet_tunnel_count.then(|| {
                meter
                    .u64_gauge("wardnet.tunnel_count")
                    .with_description("Total number of configured tunnels")
                    .build()
            }),
            tunnel_active_count: em.wardnet_tunnel_active_count.then(|| {
                meter
                    .u64_gauge("wardnet.tunnel_active_count")
                    .with_description("Number of tunnels currently in up status")
                    .build()
            }),
            uptime_seconds: em.wardnet_uptime_seconds.then(|| {
                meter
                    .u64_gauge("wardnet.uptime_seconds")
                    .with_description("Daemon uptime in seconds")
                    .with_unit("s")
                    .build()
            }),
            db_size_bytes: em.wardnet_db_size_bytes.then(|| {
                meter
                    .u64_gauge("wardnet.db_size_bytes")
                    .with_description("SQLite database file size in bytes")
                    .with_unit("By")
                    .build()
            }),
        }
    }
}

/// Main collection loop that runs until cancelled.
async fn collection_loop(
    system_service: Arc<dyn SystemService>,
    config: OtelMetricsConfig,
    meter: Meter,
    started_at: Instant,
    interval_secs: u64,
    cancel: CancellationToken,
) {
    let instruments = Instruments::new(&meter, &config);

    let mut sys = System::new_all();
    let mut components = Components::new_with_refreshed_list();
    let mut networks = Networks::new_with_refreshed_list();

    let has_temp_sensors = !components.list().is_empty();
    if !has_temp_sensors && instruments.temperature.is_some() {
        tracing::info!("no temperature sensors found, skipping system.temperature metric");
    }

    let mut tick = interval(Duration::from_secs(interval_secs));

    loop {
        tokio::select! {
            () = cancel.cancelled() => break,
            _ = tick.tick() => {}
        }

        // -- System metrics --
        sys.refresh_cpu_all();
        sys.refresh_memory();

        if let Some(gauge) = &instruments.cpu_utilization {
            let utilization = f64::from(sys.global_cpu_usage()) / 100.0;
            gauge.record(utilization, &[]);
        }

        if let Some(gauge) = &instruments.memory_usage {
            gauge.record(sys.used_memory(), &[]);
        }

        if let Some(gauge) = &instruments.temperature
            && has_temp_sensors
        {
            components.refresh(true);
            if let Some(comp) = components.first()
                && let Some(temp) = comp.temperature()
            {
                gauge.record(f64::from(temp), &[]);
            }
        }

        if let Some(gauge) = &instruments.network_io {
            networks.refresh(true);
            for (name, data) in &networks {
                gauge.record(
                    data.total_received(),
                    &[
                        KeyValue::new("interface", name.clone()),
                        KeyValue::new("direction", "rx"),
                    ],
                );
                gauge.record(
                    data.total_transmitted(),
                    &[
                        KeyValue::new("interface", name.clone()),
                        KeyValue::new("direction", "tx"),
                    ],
                );
            }
        }

        // -- Wardnet application metrics --
        if let Some(gauge) = &instruments.uptime_seconds {
            gauge.record(started_at.elapsed().as_secs(), &[]);
        }

        // Call system_service.status() inside an admin context since it
        // requires admin privileges.
        let needs_app_metrics = instruments.device_count.is_some()
            || instruments.tunnel_count.is_some()
            || instruments.tunnel_active_count.is_some()
            || instruments.db_size_bytes.is_some();

        if needs_app_metrics {
            let admin_ctx = AuthContext::Admin {
                admin_id: uuid::Uuid::nil(),
            };
            match auth_context::with_context(admin_ctx, system_service.status()).await {
                Ok(status) => {
                    if let Some(gauge) = &instruments.device_count {
                        gauge.record(status.device_count, &[]);
                    }
                    if let Some(gauge) = &instruments.tunnel_count {
                        gauge.record(status.tunnel_count, &[]);
                    }
                    if let Some(gauge) = &instruments.tunnel_active_count {
                        gauge.record(status.tunnel_active_count, &[]);
                    }
                    if let Some(gauge) = &instruments.db_size_bytes {
                        gauge.record(status.db_size_bytes, &[]);
                    }
                }
                Err(e) => {
                    tracing::error!(error = %e, "metrics collector: failed to query system status");
                }
            }
        }
    }
}
