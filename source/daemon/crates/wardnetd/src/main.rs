use std::ffi::OsStr;
use std::future::IntoFuture;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use clap::Parser;
use opentelemetry::trace::TracerProvider as _;
use opentelemetry::{Key, KeyValue, Value};
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::logs::SdkLoggerProvider;
use opentelemetry_sdk::trace::SdkTracerProvider;
use tokio::net::TcpListener;
use tracing::Instrument;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use wardnetd::config::{Config, LogFormat, LogRotation, OtelConfig};
use wardnetd::device_detector::DeviceDetector;
use wardnetd::event::{BroadcastEventBus, EventPublisher};
use wardnetd::hostname_resolver::HostnameResolver;
use wardnetd::hostname_resolver_noop::NoopHostnameResolver;
use wardnetd::keys::FileKeyStore;
use wardnetd::packet_capture::PacketCapture;
use wardnetd::packet_capture_noop::NoopPacketCapture;
use wardnetd::packet_capture_pnet::PnetCapture;
use wardnetd::repository::{
    SqliteAdminRepository, SqliteApiKeyRepository, SqliteDeviceRepository, SqliteSessionRepository,
    SqliteSystemConfigRepository, SqliteTunnelRepository,
};
use wardnetd::service::{
    AuthServiceImpl, DeviceDiscoveryServiceImpl, DeviceServiceImpl, ProviderServiceImpl,
    SystemServiceImpl, TunnelServiceImpl,
};
use wardnetd::vpn_provider_registry::VpnProviderRegistry;
use wardnetd::state::AppState;
use wardnetd::tunnel_idle::IdleTunnelWatcher;
use wardnetd::tunnel_monitor::TunnelMonitor;
use wardnetd::wireguard::WireGuardOps;
use wardnetd::wireguard_noop::NoopWireGuard;
use wardnetd::wireguard_real::RealWireGuard;
use wardnetd::{api, db};

/// Wardnet daemon — self-hosted network privacy gateway.
#[derive(Parser)]
#[command(name = "wardnetd", version = env!("WARDNET_VERSION"))]
struct Cli {
    /// Path to configuration file.
    #[arg(long, short, default_value = "/etc/wardnet/wardnet.toml")]
    config: PathBuf,

    /// Enable verbose (debug-level) logging and also output to the console.
    /// Overrides both the config file and `--log-level`.
    #[arg(long, short)]
    verbose: bool,

    /// Override the log level from the config file.
    #[arg(long)]
    log_level: Option<String>,

    /// Override the log file path from the config file.
    #[arg(long)]
    log_path: Option<PathBuf>,

    /// Run in foreground mode (default). Use with systemd or as a regular process.
    #[arg(long)]
    foreground: bool,

    /// Use a no-op `WireGuard` backend instead of real kernel interfaces.
    #[arg(long)]
    mock_network: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let config = Config::load(&cli.config)?;

    let TracingGuards {
        _log_guard,
        otel_tracer_provider,
        otel_logger_provider,
    } = init_tracing(
        &config,
        cli.log_level.as_deref(),
        cli.log_path.as_deref(),
        cli.verbose,
    );

    // Use `.instrument()` instead of `span.enter()` so the span is correctly
    // propagated across `.await` points in the tokio multi-threaded runtime.
    // The `Full` (console) formatter prints span fields on every line, and the
    // JSON formatter includes them via `with_current_span` / `with_span_list`.
    let result = run(config, cli.mock_network)
        .instrument(tracing::info_span!(
            "wardnetd",
            version = env!("WARDNET_VERSION")
        ))
        .await;

    // Flush remaining spans and logs to the OTel collector before exiting.
    if let Some(provider) = otel_tracer_provider
        && let Err(e) = provider.shutdown()
    {
        eprintln!("failed to shut down OTel tracer provider: {e}");
    }
    if let Some(provider) = otel_logger_provider
        && let Err(e) = provider.shutdown()
    {
        eprintln!("failed to shut down OTel logger provider: {e}");
    }

    result
}

/// Inner entry-point that runs inside the root `wardnetd{version=...}` span.
#[allow(clippy::too_many_lines)]
async fn run(config: Config, mock_network: bool) -> anyhow::Result<()> {
    let started_at = Instant::now();

    let pool = db::init_pool(&config.database.path).await?;

    // Create repository instances (concrete SQLite implementations).
    let admin_repo: Arc<dyn wardnetd::repository::AdminRepository> =
        Arc::new(SqliteAdminRepository::new(pool.clone()));
    let session_repo = Arc::new(SqliteSessionRepository::new(pool.clone()));
    let api_key_repo = Arc::new(SqliteApiKeyRepository::new(pool.clone()));
    let device_repo = Arc::new(SqliteDeviceRepository::new(pool.clone()));
    let system_config_repo = Arc::new(SqliteSystemConfigRepository::new(pool.clone()));
    let tunnel_repo = Arc::new(SqliteTunnelRepository::new(pool));

    // Create event publisher.
    let event_publisher: Arc<dyn EventPublisher> = Arc::new(BroadcastEventBus::new(256));

    let (wireguard, packet_capture, hostname_resolver) = network_backends(mock_network);

    // Create key store.
    let key_store = Arc::new(FileKeyStore::new(config.tunnel.keys_dir.clone()));

    // Create service instances, injecting repository traits.
    // `system_config_repo` is shared between AuthServiceImpl and SystemServiceImpl,
    // hence the clone — same pattern as `device_repo` which is shared across services.
    let auth_service = Arc::new(AuthServiceImpl::new(
        admin_repo,
        session_repo,
        api_key_repo,
        system_config_repo.clone(),
        config.auth.session_expiry_hours,
    ));
    let device_service = Arc::new(DeviceServiceImpl::new(device_repo.clone()));
    let system_service = Arc::new(SystemServiceImpl::new(system_config_repo, started_at));
    let tunnel_service: Arc<dyn wardnetd::service::TunnelService> =
        Arc::new(TunnelServiceImpl::new(
            tunnel_repo.clone(),
            wireguard.clone(),
            key_store,
            event_publisher.clone(),
        ));

    // VPN provider registry and service.
    let registry = Arc::new(VpnProviderRegistry::new(&config));
    let provider_service: Arc<dyn wardnetd::service::ProviderService> =
        Arc::new(ProviderServiceImpl::new(
            registry,
            tunnel_service.clone(),
        ));

    // Create device discovery service.
    let discovery_service: Arc<dyn wardnetd::service::DeviceDiscoveryService> = Arc::new(
        DeviceDiscoveryServiceImpl::new(device_repo, event_publisher.clone(), hostname_resolver),
    );

    // Restore state from the database.
    tunnel_service
        .restore_tunnels()
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    discovery_service
        .restore_devices()
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    // Capture the root span so background tasks can create child spans that
    // inherit the `wardnetd{version=...}` context.
    let root_span = tracing::Span::current();

    // Start background monitors.
    let monitor = TunnelMonitor::start(
        tunnel_repo,
        wireguard,
        event_publisher.clone(),
        config.tunnel.stats_interval_secs,
        config.tunnel.health_check_interval_secs,
        &root_span,
    );
    let idle_watcher = IdleTunnelWatcher::start(
        event_publisher.clone(),
        tunnel_service.clone(),
        config.tunnel.idle_timeout_secs,
        &root_span,
    );

    // Start device detector (conditionally).
    let device_detector = if config.detection.enabled {
        Some(DeviceDetector::start(
            packet_capture,
            discovery_service.clone(),
            &config.detection,
            config.network.lan_interface.clone(),
            &root_span,
        ))
    } else {
        tracing::info!("device detection disabled");
        None
    };

    let state = AppState::new(
        auth_service,
        device_service,
        discovery_service,
        provider_service,
        system_service,
        tunnel_service,
        event_publisher,
        config.clone(),
        started_at,
    );

    let app = api::router(state);

    let addr: SocketAddr = format!("{}:{}", config.server.host, config.server.port).parse()?;

    println!(
        "\n  Wardnet daemon v{}\n  Listening on http://{}\n  Database: {}\n",
        env!("WARDNET_VERSION"),
        addr,
        config.database.path.display()
    );

    let listener = TcpListener::bind(addr).await?;

    let api_span = tracing::info_span!(parent: &root_span, "api_server");
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .into_future()
    .instrument(api_span)
    .await?;

    tracing::info!("server stopped, shutting down background tasks");
    monitor.shutdown().await;
    idle_watcher.shutdown().await;
    if let Some(detector) = device_detector {
        detector.shutdown().await;
    }

    Ok(())
}

/// Select real or no-op network backends based on the `--mock-network` flag.
fn network_backends(
    mock: bool,
) -> (
    Arc<dyn WireGuardOps>,
    Arc<dyn PacketCapture>,
    Arc<dyn HostnameResolver>,
) {
    if mock {
        tracing::info!("using no-op network backends (--mock-network)");
        (
            Arc::new(NoopWireGuard),
            Arc::new(NoopPacketCapture),
            Arc::new(NoopHostnameResolver),
        )
    } else {
        (
            Arc::new(RealWireGuard),
            Arc::new(PnetCapture),
            Arc::new(wardnetd::hostname_resolver::SystemHostnameResolver),
        )
    }
}

/// Handles returned from [`init_tracing`] that must be held for the lifetime
/// of the program. Dropping them flushes pending data to their respective sinks.
struct TracingGuards {
    /// Flushes the non-blocking file writer on drop.
    _log_guard: WorkerGuard,
    /// When `Some`, the OTLP tracer provider that must be shut down on exit.
    otel_tracer_provider: Option<SdkTracerProvider>,
    /// When `Some`, the OTLP logger provider that must be shut down on exit.
    otel_logger_provider: Option<SdkLoggerProvider>,
}

/// Build the `OTel` resource with service name and version attributes.
fn otel_resource(config: &OtelConfig) -> opentelemetry_sdk::Resource {
    opentelemetry_sdk::Resource::builder()
        .with_service_name(config.service_name.clone())
        .with_attributes([KeyValue::new(
            Key::new("service.version"),
            Value::from(env!("WARDNET_VERSION")),
        )])
        .build()
}

/// Build an OTLP tracer provider if `OTel` is enabled.
fn init_otel_tracer(config: &OtelConfig) -> Option<SdkTracerProvider> {
    if !config.enabled {
        return None;
    }

    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .with_endpoint(&config.endpoint)
        .build()
        .expect("failed to create OTLP span exporter");

    let provider = SdkTracerProvider::builder()
        .with_batch_exporter(exporter)
        .with_resource(otel_resource(config))
        .build();

    opentelemetry::global::set_tracer_provider(provider.clone());

    Some(provider)
}

/// Build an OTLP logger provider if `OTel` is enabled.
fn init_otel_logger(config: &OtelConfig) -> Option<SdkLoggerProvider> {
    if !config.enabled {
        return None;
    }

    let log_exporter = opentelemetry_otlp::LogExporter::builder()
        .with_tonic()
        .with_endpoint(&config.endpoint)
        .build()
        .expect("failed to create OTLP log exporter");

    let provider = SdkLoggerProvider::builder()
        .with_batch_exporter(log_exporter)
        .with_resource(otel_resource(config))
        .build();

    Some(provider)
}

/// Initialize the tracing subscriber with file-based logging, optional
/// console output, and optional OpenTelemetry OTLP export.
///
/// Logs are always written to the configured file path using a rolling
/// appender. Console output is only enabled when `verbose` is true.
/// When the `[otel]` config section is enabled, traces and logs are
/// additionally exported via OTLP gRPC.
///
/// Level priority (highest to lowest):
/// 1. `RUST_LOG` environment variable
/// 2. `--verbose` flag (forces "debug")
/// 3. `--log-level` CLI argument
/// 4. Config file `logging.level`
///
/// Returns [`TracingGuards`] that **must** be held for the lifetime of the
/// program. Dropping them flushes and shuts down the non-blocking writer
/// and `OTel` providers.
fn init_tracing(
    config: &Config,
    cli_log_level: Option<&str>,
    cli_log_path: Option<&Path>,
    verbose: bool,
) -> TracingGuards {
    // Determine effective filter. Priority:
    // 1. RUST_LOG env var (full control, overrides everything)
    // 2. --verbose flag (sets wardnet crates to debug, deps stay at warn)
    // 3. Config file logging.level + logging.filters
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        if verbose {
            // Override wardnet level to debug but respect per-crate filters.
            let mut cfg = config.logging.clone();
            "debug".clone_into(&mut cfg.level);
            EnvFilter::new(cfg.to_filter_string())
        } else if let Some(cli_level) = cli_log_level {
            let mut cfg = config.logging.clone();
            cli_level.clone_into(&mut cfg.level);
            EnvFilter::new(cfg.to_filter_string())
        } else {
            EnvFilter::new(config.logging.to_filter_string())
        }
    });

    // Determine effective log path.
    let log_path = cli_log_path.unwrap_or(&config.logging.path);

    // Create log directory if it doesn't exist.
    if let Some(parent) = log_path.parent() {
        std::fs::create_dir_all(parent).ok();
    }

    let dir = log_path.parent().unwrap_or(Path::new("."));
    let filename = log_path
        .file_name()
        .unwrap_or(OsStr::new("wardnetd.log"))
        .to_str()
        .expect("log file name is not valid UTF-8");

    let rotation = match config.logging.rotation {
        LogRotation::Hourly => tracing_appender::rolling::Rotation::HOURLY,
        LogRotation::Daily => tracing_appender::rolling::Rotation::DAILY,
        LogRotation::Never => tracing_appender::rolling::Rotation::NEVER,
    };

    // Build the rolling file appender with retention limits.
    let file_appender = tracing_appender::rolling::RollingFileAppender::builder()
        .rotation(rotation)
        .filename_prefix(filename)
        .max_log_files(config.logging.max_log_files)
        .build(dir)
        .expect("failed to create log file appender");

    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    // --- OpenTelemetry layers (no-op `None` when disabled) ---
    let otel_tracer_provider = init_otel_tracer(&config.otel);
    let otel_trace_layer = otel_tracer_provider.as_ref().map(|p: &SdkTracerProvider| {
        tracing_opentelemetry::layer().with_tracer(p.tracer("wardnetd"))
    });

    let otel_logger_provider = init_otel_logger(&config.otel);
    let otel_log_layer = otel_logger_provider
        .as_ref()
        .map(opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge::new);

    // Build the subscriber. JSON format is applied only to the file layer;
    // the console layer (if enabled) always uses human-readable output.
    // Optional OTel layers are added regardless of format — `Option<L>`
    // implements `Layer` as a no-op when `None`.
    let registry = tracing_subscriber::registry()
        .with(filter)
        .with(otel_trace_layer)
        .with(otel_log_layer);

    match config.logging.format {
        LogFormat::Json => {
            let file_layer = tracing_subscriber::fmt::layer()
                .json()
                .with_current_span(true)
                .with_span_list(true)
                .with_writer(non_blocking)
                .with_ansi(false);

            if verbose {
                let console_layer = tracing_subscriber::fmt::layer().with_writer(std::io::stderr);
                registry.with(file_layer).with(console_layer).init();
            } else {
                registry.with(file_layer).init();
            }
        }
        LogFormat::Console => {
            let file_layer = tracing_subscriber::fmt::layer()
                .with_writer(non_blocking)
                .with_ansi(false);

            if verbose {
                let console_layer = tracing_subscriber::fmt::layer().with_writer(std::io::stderr);
                registry.with(file_layer).with(console_layer).init();
            } else {
                registry.with(file_layer).init();
            }
        }
    }

    TracingGuards {
        _log_guard: guard,
        otel_tracer_provider,
        otel_logger_provider,
    }
}

async fn shutdown_signal() {
    let ctrl_c = tokio::signal::ctrl_c();

    #[cfg(unix)]
    {
        let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler");
        tokio::select! {
            _ = ctrl_c => {},
            _ = sigterm.recv() => {},
        }
    }

    #[cfg(not(unix))]
    {
        ctrl_c.await.ok();
    }

    tracing::info!("shutdown signal received");
}
