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
use opentelemetry_sdk::metrics::SdkMeterProvider;
use opentelemetry_sdk::trace::SdkTracerProvider;
use tokio::net::TcpListener;
use tracing::Instrument;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use wardnet_types::auth::AuthContext;
use wardnetd::config::{Config, LogFormat, LogRotation, OtelConfig};
use wardnetd::device_detector::DeviceDetector;
use wardnetd::dhcp::runner::DhcpRunner;
use wardnetd::dhcp::server::{NoopDhcpServer, UdpDhcpServer};
use wardnetd::dns::blocklist_downloader::HttpBlocklistFetcher;
use wardnetd::dns::filter::DnsFilter;
use wardnetd::dns::runner::DnsRunner;
use wardnetd::dns::server::{NoopDnsServer, UdpDnsServer};
use wardnetd::event::{BroadcastEventBus, EventPublisher};
use wardnetd::firewall::FirewallManager;
use wardnetd::firewall_nftables::NftablesFirewallManager;
use wardnetd::hostname_resolver::HostnameResolver;
use wardnetd::keys::FileKeyStore;
use wardnetd::metrics_collector::MetricsCollector;
use wardnetd::noop_hostname_resolver::NoopHostnameResolver;
use wardnetd::noop_network::{NoopFirewallManager, NoopPolicyRouter, NoopTunnelInterface};
use wardnetd::noop_packet_capture::NoopPacketCapture;
use wardnetd::packet_capture::PacketCapture;
use wardnetd::packet_capture_pnet::PnetCapture;
use wardnetd::policy_router::PolicyRouter;
use wardnetd::policy_router_netlink::NetlinkPolicyRouter;
use wardnetd::profiling::ProfilingAgent;
use wardnetd::repository::{
    SqliteAdminRepository, SqliteApiKeyRepository, SqliteDeviceRepository, SqliteDhcpRepository,
    SqliteSessionRepository, SqliteSystemConfigRepository, SqliteTunnelRepository,
};
use wardnetd::routing_listener::RoutingListener;
use wardnetd::service::{
    AuthServiceImpl, DeviceDiscoveryServiceImpl, DeviceServiceImpl, DhcpServiceImpl,
    DnsServiceImpl, ProviderServiceImpl, RoutingServiceImpl, SystemServiceImpl, TunnelServiceImpl,
};
use wardnetd::state::AppState;
use wardnetd::tunnel_idle::IdleTunnelWatcher;
use wardnetd::tunnel_interface::TunnelInterface;
use wardnetd::tunnel_interface_wireguard::WireGuardTunnelInterface;
use wardnetd::tunnel_monitor::TunnelMonitor;
use wardnetd::vpn_provider_registry::VpnProviderRegistry;
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

    /// Use no-op network backends instead of real kernel interfaces.
    #[arg(long)]
    mock_network: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let config = Config::load(&cli.config)?;

    let log_broadcaster = wardnetd::log_broadcast::LogBroadcaster::new(1024);
    let recent_errors = wardnetd::recent_errors::RecentErrors::new();

    let TracingGuards {
        _log_guard,
        otel_tracer_provider,
        otel_logger_provider,
        otel_meter_provider,
    } = init_tracing(
        &config,
        cli.log_level.as_deref(),
        cli.log_path.as_deref(),
        cli.verbose,
        &log_broadcaster,
        &recent_errors,
    );

    // Use `.instrument()` instead of `span.enter()` so the span is correctly
    // propagated across `.await` points in the tokio multi-threaded runtime.
    // The `Full` (console) formatter prints span fields on every line, and the
    // JSON formatter includes them via `with_current_span` / `with_span_list`.
    let result = run(config, cli.mock_network, log_broadcaster, recent_errors)
        .instrument(tracing::info_span!(
            "wardnetd",
            version = env!("WARDNET_VERSION")
        ))
        .await;

    // Flush remaining spans and logs to the OTel collector before exiting.
    // Meter provider must be shut down before tracer/logger providers.
    if let Some(provider) = otel_meter_provider
        && let Err(e) = provider.shutdown()
    {
        eprintln!("failed to shut down OTel meter provider: {e}");
    }
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
async fn run(
    config: Config,
    mock_network: bool,
    log_broadcaster: wardnetd::log_broadcast::LogBroadcaster,
    recent_errors: wardnetd::recent_errors::RecentErrors,
) -> anyhow::Result<()> {
    let started_at = Instant::now();

    let pool = db::init_pool(&config.database.path).await?;

    // Create repository instances (concrete SQLite implementations).
    let admin_repo: Arc<dyn wardnetd::repository::AdminRepository> =
        Arc::new(SqliteAdminRepository::new(pool.clone()));
    let session_repo = Arc::new(SqliteSessionRepository::new(pool.clone()));
    let api_key_repo = Arc::new(SqliteApiKeyRepository::new(pool.clone()));
    let device_repo = Arc::new(SqliteDeviceRepository::new(pool.clone()));
    let system_config_repo = Arc::new(SqliteSystemConfigRepository::new(pool.clone()));
    let dhcp_repo = Arc::new(SqliteDhcpRepository::new(pool.clone()));
    let dns_repo = Arc::new(wardnetd::repository::SqliteDnsRepository::new(pool.clone()));
    let tunnel_repo = Arc::new(SqliteTunnelRepository::new(pool));

    // Create event publisher.
    let event_publisher: Arc<dyn EventPublisher> = Arc::new(BroadcastEventBus::new(256));

    let backends = network_backends(mock_network);

    // Detect wardnet's own LAN IP for DHCP gateway advertisement.
    let lan_ip = wardnetd::packet_capture_pnet::get_interface_ipv4(&config.network.lan_interface)
        .unwrap_or_else(|e| {
            tracing::warn!(error = %e, "failed to detect LAN IP, using 0.0.0.0");
            std::net::Ipv4Addr::UNSPECIFIED
        });
    tracing::info!(
        lan_ip = %lan_ip,
        interface = %config.network.lan_interface,
        "detected LAN IP for DHCP gateway"
    );

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
    let device_service = Arc::new(DeviceServiceImpl::new(
        device_repo.clone(),
        event_publisher.clone(),
    ));
    let dhcp_service: Arc<dyn wardnetd::service::DhcpService> = Arc::new(DhcpServiceImpl::new(
        dhcp_repo,
        system_config_repo.clone(),
        lan_ip,
    ));
    let dns_service: Arc<dyn wardnetd::service::DnsService> = Arc::new(DnsServiceImpl::new(
        system_config_repo.clone(),
        dns_repo.clone(),
        event_publisher.clone(),
    ));
    let system_service = Arc::new(SystemServiceImpl::new(
        system_config_repo,
        tunnel_repo.clone(),
        started_at,
    ));
    let tunnel_service: Arc<dyn wardnetd::service::TunnelService> =
        Arc::new(TunnelServiceImpl::new(
            tunnel_repo.clone(),
            device_repo.clone(),
            backends.tunnel_interface.clone(),
            key_store,
            event_publisher.clone(),
        ));

    // VPN provider registry and service.
    let registry = Arc::new(VpnProviderRegistry::new(&config));
    let provider_service: Arc<dyn wardnetd::service::ProviderService> =
        Arc::new(ProviderServiceImpl::new(registry, tunnel_service.clone()));

    // Compute LAN subnet for device detection filtering.
    let lan_subnet = ipnetwork::Ipv4Network::new(lan_ip, 24).unwrap_or_else(|_| {
        tracing::warn!("failed to create LAN subnet, using /24 default");
        ipnetwork::Ipv4Network::new(lan_ip, 24).expect("valid /24")
    });
    tracing::info!(subnet = %lan_subnet, "LAN subnet for device detection");

    // Create device discovery service.
    let discovery_service: Arc<dyn wardnetd::service::DeviceDiscoveryService> =
        Arc::new(DeviceDiscoveryServiceImpl::new(
            device_repo.clone(),
            event_publisher.clone(),
            backends.hostname_resolver.clone(),
            lan_subnet,
        ));

    // Create routing service (policy routing engine).
    let routing_service: Arc<dyn wardnetd::service::RoutingService> =
        Arc::new(RoutingServiceImpl::new(
            device_repo.clone(),
            tunnel_repo.clone(),
            tunnel_service.clone(),
            backends.policy_router.clone(),
            backends.firewall.clone(),
            config.network.default_policy.clone(),
            config.network.lan_interface.clone(),
        ));

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

    // Reconcile routing state with kernel on startup.
    wardnetd::auth_context::with_context(
        AuthContext::Admin {
            admin_id: uuid::Uuid::nil(),
        },
        routing_service.reconcile(),
    )
    .await
    .map_err(|e| anyhow::anyhow!("{e}"))?;

    // Start background monitors.
    let monitor = TunnelMonitor::start(
        tunnel_repo.clone(),
        backends.tunnel_interface.clone(),
        event_publisher.clone(),
        config.tunnel.stats_interval_secs,
        config.tunnel.health_check_interval_secs,
        &root_span,
    );
    let idle_watcher = IdleTunnelWatcher::start(
        event_publisher.clone(),
        tunnel_service.clone(),
        routing_service.clone(),
        config.tunnel.idle_timeout_secs,
        &root_span,
    );
    let routing_listener = RoutingListener::start(
        &event_publisher,
        routing_service.clone(),
        device_repo.clone(),
        &root_span,
    );
    let route_monitor = if mock_network {
        None
    } else {
        Some(
            wardnetd::route_monitor::RouteMonitor::start(event_publisher.clone(), &root_span)
                .map_err(|e| anyhow::anyhow!("failed to start route monitor: {e}"))?,
        )
    };

    // Start device detector (conditionally).
    let device_detector = if config.detection.enabled {
        Some(DeviceDetector::start(
            backends.packet_capture.clone(),
            discovery_service.clone(),
            &config.detection,
            config.network.lan_interface.clone(),
            &root_span,
        ))
    } else {
        tracing::info!("device detection disabled");
        None
    };

    // Start metrics collector (conditionally).
    let metrics_collector = if config.otel.enabled && config.otel.metrics.enabled {
        let meter = opentelemetry::global::meter("wardnetd");
        Some(MetricsCollector::start(
            system_service.clone(),
            &config.otel.metrics,
            meter,
            started_at,
            config.otel.interval_secs,
            &root_span,
        ))
    } else {
        tracing::info!("OTel metrics collection disabled");
        None
    };

    // Start Pyroscope profiling agent (conditionally).
    let profiling_agent = ProfilingAgent::start(&config.pyroscope);

    // Start DHCP server and runner.
    let dhcp_server: Arc<dyn wardnetd::dhcp::server::DhcpServer> = if mock_network {
        Arc::new(NoopDhcpServer)
    } else {
        // Load initial DHCP config under admin context.
        // gateway_ip is injected by DhcpServiceImpl from the detected LAN IP.
        let admin_ctx = wardnet_types::auth::AuthContext::Admin {
            admin_id: uuid::Uuid::nil(),
        };
        let initial_config = wardnetd::auth_context::with_context(
            admin_ctx,
            dhcp_service.get_dhcp_config(),
        )
        .await
        .unwrap_or_else(|e| {
            tracing::warn!(error = %e, "failed to load initial DHCP config, using defaults");
            {
                // Derive default pool range from the detected LAN IP subnet.
                let octets = lan_ip.octets();
                wardnet_types::dhcp::DhcpConfig {
                    enabled: false,
                    gateway_ip: lan_ip,
                    pool_start: std::net::Ipv4Addr::new(octets[0], octets[1], octets[2], 100),
                    pool_end: std::net::Ipv4Addr::new(octets[0], octets[1], octets[2], 250),
                    subnet_mask: "255.255.255.0".parse().expect("valid IP"),
                    upstream_dns: vec![
                        "1.1.1.1".parse().expect("valid IP"),
                        "8.8.8.8".parse().expect("valid IP"),
                    ],
                    lease_duration_secs: 600,
                    router_ip: None,
                }
            }
        });
        Arc::new(UdpDhcpServer::new(dhcp_service.clone(), initial_config))
    };
    let dhcp_runner = DhcpRunner::start(
        dhcp_service.clone(),
        dhcp_server.clone(),
        event_publisher.as_ref(),
        &root_span,
    );

    // Start DNS server and runner.
    let dns_filter = Arc::new(tokio::sync::RwLock::new(DnsFilter::empty()));
    let blocklist_fetcher: Arc<dyn wardnetd::dns::blocklist_downloader::BlocklistFetcher> =
        Arc::new(HttpBlocklistFetcher::new());
    let dns_server: Arc<dyn wardnetd::dns::server::DnsServer> = if mock_network {
        Arc::new(NoopDnsServer)
    } else {
        Arc::new(UdpDnsServer::new(
            wardnet_types::dns::DnsConfig::default(),
            Arc::clone(&dns_filter),
        ))
    };
    let dns_runner = DnsRunner::start(
        dns_service.clone(),
        dns_server.clone(),
        dns_repo.clone(),
        dns_filter,
        blocklist_fetcher,
        event_publisher.as_ref(),
        &root_span,
    );

    let state = AppState::new(
        auth_service.clone(),
        device_service.clone(),
        dhcp_service,
        dns_service,
        discovery_service.clone(),
        provider_service.clone(),
        routing_service.clone(),
        system_service.clone(),
        tunnel_service.clone(),
        dhcp_server,
        dns_server,
        event_publisher.clone(),
        log_broadcaster,
        recent_errors,
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
    routing_listener.shutdown().await;
    if let Some(rm) = route_monitor {
        rm.shutdown().await;
    }
    idle_watcher.shutdown().await;
    monitor.shutdown().await;
    dhcp_runner.shutdown().await;
    dns_runner.shutdown().await;
    if let Some(detector) = device_detector {
        detector.shutdown().await;
    }
    if let Some(collector) = metrics_collector {
        collector.shutdown().await;
    }
    if let Some(agent) = profiling_agent {
        agent.shutdown();
    }

    Ok(())
}

/// Network backends selected at startup based on the `--mock-network` flag.
struct NetworkBackends {
    tunnel_interface: Arc<dyn TunnelInterface>,
    packet_capture: Arc<dyn PacketCapture>,
    hostname_resolver: Arc<dyn HostnameResolver>,
    policy_router: Arc<dyn PolicyRouter>,
    firewall: Arc<dyn FirewallManager>,
}

/// Select real or no-op network backends based on the `--mock-network` flag.
fn network_backends(mock: bool) -> NetworkBackends {
    if mock {
        tracing::info!("using no-op network backends (--mock-network)");
        NetworkBackends {
            tunnel_interface: Arc::new(NoopTunnelInterface),
            packet_capture: Arc::new(NoopPacketCapture),
            hostname_resolver: Arc::new(NoopHostnameResolver),
            policy_router: Arc::new(NoopPolicyRouter),
            firewall: Arc::new(NoopFirewallManager),
        }
    } else {
        let executor = Arc::new(wardnetd::command::ShellCommandExecutor);
        NetworkBackends {
            tunnel_interface: Arc::new(WireGuardTunnelInterface),
            packet_capture: Arc::new(PnetCapture),
            hostname_resolver: Arc::new(wardnetd::hostname_resolver::SystemHostnameResolver),
            policy_router: Arc::new(
                NetlinkPolicyRouter::new(executor.clone())
                    .expect("failed to initialise netlink policy router"),
            ),
            firewall: Arc::new(NftablesFirewallManager::new(executor.clone())),
        }
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
    /// When `Some`, the OTLP meter provider that must be shut down on exit.
    otel_meter_provider: Option<SdkMeterProvider>,
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
    if !config.enabled || !config.traces.enabled {
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
    if !config.enabled || !config.logs.enabled {
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

/// Build an OTLP meter provider if `OTel` metrics are enabled.
fn init_otel_meter(config: &OtelConfig) -> Option<SdkMeterProvider> {
    if !config.enabled || !config.metrics.enabled {
        return None;
    }

    let metric_exporter = opentelemetry_otlp::MetricExporter::builder()
        .with_tonic()
        .with_endpoint(&config.endpoint)
        .build()
        .expect("failed to create OTLP metric exporter");

    let reader = opentelemetry_sdk::metrics::PeriodicReader::builder(metric_exporter)
        .with_interval(std::time::Duration::from_secs(config.interval_secs))
        .build();

    let provider = SdkMeterProvider::builder()
        .with_reader(reader)
        .with_resource(otel_resource(config))
        .build();

    opentelemetry::global::set_meter_provider(provider.clone());

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
    log_broadcaster: &wardnetd::log_broadcast::LogBroadcaster,
    recent_errors: &wardnetd::recent_errors::RecentErrors,
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
    // Broadcast layer sends log events to WebSocket clients in real time.
    let broadcast_layer = wardnetd::log_broadcast::BroadcastLayer::new(log_broadcaster.clone());
    // Recent errors layer keeps the last 15 WARN/ERROR entries in memory.
    let recent_errors_layer =
        wardnetd::recent_errors::RecentErrorsLayer::new(recent_errors.clone());

    let registry = tracing_subscriber::registry()
        .with(filter)
        .with(broadcast_layer)
        .with(recent_errors_layer)
        .with(otel_trace_layer)
        .with(otel_log_layer);

    // File output is always JSON for machine-readable log parsing (API, web UI).
    // Console output follows the config format setting for human readability.
    let file_layer = tracing_subscriber::fmt::layer()
        .json()
        .with_current_span(true)
        .with_span_list(true)
        .with_writer(non_blocking)
        .with_ansi(false);

    if verbose {
        match config.logging.format {
            LogFormat::Json => {
                let console_layer = tracing_subscriber::fmt::layer()
                    .json()
                    .with_writer(std::io::stderr);
                registry.with(file_layer).with(console_layer).init();
            }
            LogFormat::Console => {
                let console_layer = tracing_subscriber::fmt::layer().with_writer(std::io::stderr);
                registry.with(file_layer).with(console_layer).init();
            }
        }
    } else {
        registry.with(file_layer).init();
    }

    let otel_meter_provider = init_otel_meter(&config.otel);

    TracingGuards {
        _log_guard: guard,
        otel_tracer_provider,
        otel_logger_provider,
        otel_meter_provider,
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
