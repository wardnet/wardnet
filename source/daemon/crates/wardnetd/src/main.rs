use std::ffi::OsStr;
use std::future::IntoFuture;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

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

use wardnet_common::auth::AuthContext;
use wardnet_common::config::{ApplicationConfiguration, LogFormat, LogRotation, OtelConfig};
use wardnetd::device_detector::DeviceDetector;
use wardnetd::firewall_netlink::NetlinkFirewallManager;
use wardnetd::garp_pnet::PnetGarpOps;
use wardnetd::health_runner::HealthMonitorRunner;
use wardnetd::heartbeat::HeartbeatRunner;
use wardnetd::hostname_resolver::SystemHostnameResolver;
use wardnetd::mdns_advertiser::MdnsAdvertiser;
use wardnetd::metrics_collector::MetricsCollector;
use wardnetd::packet_capture_pnet::PnetCapture;
use wardnetd::policy_router_netlink::NetlinkPolicyRouter;
use wardnetd::profiling::ProfilingAgent;
use wardnetd::route_monitor::RouteMonitor;
use wardnetd::routing_listener::RoutingListener;
use wardnetd::system::{
    LinuxWatchdog, PnetNetworkProbe, ProcNetNetworkInspector, SystemctlPowerOps,
};
use wardnetd::tls_server;
use wardnetd::tunnel_exit_probe::ReqwestTunnelExitProbe;
use wardnetd::tunnel_idle::IdleTunnelWatcher;
use wardnetd::tunnel_interface_wireguard::WireGuardTunnelInterface;
use wardnetd::tunnel_latency_prober::SurgePingTunnelLatencyProber;
use wardnetd::tunnel_monitor::TunnelMonitor;
use wardnetd::tunnel_throughput_tester::HttpThroughputTester;
use wardnetd::watchdog::{HardwareWatchdogRunner, Notifier, SdNotifier, SoftWatchdogRunner};
use wardnetd_api::state::AppState;
use wardnetd_services::HealthMonitor;
use wardnetd_services::TlsRenewalRunner;
use wardnetd_services::db_maintenance_runner::DbMaintenanceRunner;
use wardnetd_services::ddns::runner::DdnsUpdateRunner;
use wardnetd_services::dhcp::runner::DhcpRunner;
use wardnetd_services::dns::DnsCaptureRunner;
use wardnetd_services::dns::dhcp_lan_runner::DhcpLanRunner;
use wardnetd_services::dns::query_log_runner::DnsQueryLogRunner;
use wardnetd_services::dns::runner::DnsRunner;
use wardnetd_services::dns_filter::blocklist_downloader::{BlocklistFetcher, HttpBlocklistFetcher};
use wardnetd_services::dns_filter::runner::DnsFilterRunner;
use wardnetd_services::health::checks::{
    DbHealthCheck, DhcpServerHealthCheck, DnsServerHealthCheck, LivenessHealthCheck,
};
use wardnetd_services::logging::{
    ErrorNotifierService, LogService, LogServiceImpl, LogStreamService,
};
use wardnetd_services::secret_store::build_secret_store;
use wardnetd_services::system::WatchdogOps;
use wardnetd_services::update::{
    EMBEDDED_PUBLIC_KEY, FsBinaryApplier, HttpsManifestSource, Sha256MinisignVerifier, UpdateRunner,
};
use wardnetd_services::{Backends, UpdateBackends, auth_context, init_services_with_factory};

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
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let config = ApplicationConfiguration::load(&cli.config)?;

    // Build the log service BEFORE init_tracing so its tracing layers can
    // be attached to the subscriber.
    let log_stream = Arc::new(LogStreamService::new(config.logging.broadcast_capacity));
    let error_notifier = Arc::new(ErrorNotifierService::new(config.logging.max_recent_errors));
    let log_service: Arc<dyn LogService> = Arc::new(LogServiceImpl::new(
        log_stream,
        error_notifier,
        config.logging.path.clone(),
    ));

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
        log_service.as_ref(),
    );

    // Use `.instrument()` instead of `span.enter()` so the span is correctly
    // propagated across `.await` points in the tokio multi-threaded runtime.
    // The `Full` (console) formatter prints span fields on every line, and the
    // JSON formatter includes them via `with_current_span` / `with_span_list`.
    let result = run(config, cli.config.clone(), log_service)
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
    config: ApplicationConfiguration,
    config_path: PathBuf,
    log_service: Arc<dyn LogService>,
) -> anyhow::Result<()> {
    let started_at = Instant::now();

    // Install the aws-lc-rs crypto provider before anything builds a rustls
    // `ServerConfig` (the `:443` listener does). Both ring and aws-lc-rs are in
    // the tree, so rustls 0.23 can't auto-pick — without this it panics at the
    // first handshake. Must run on every code path that reaches TLS setup.
    tls_server::install_crypto_provider();

    // Detect wardnet's own LAN IP for DHCP gateway advertisement.
    //
    // After a cold boot, `network-online.target` is reached before the
    // LAN interface has actually finished applying its IPv4. Single-shot
    // detection here used to land in that gap, lock in `0.0.0.0` for the
    // process lifetime, and poison every DHCP response. Poll until the
    // address is up or the deadline expires.
    let lan_ip = wardnetd::packet_capture_pnet::wait_for_interface_ipv4(
        &config.network.lan_interface,
        Duration::from_secs(30),
        Duration::from_millis(500),
    )
    .await
    .unwrap_or(std::net::Ipv4Addr::UNSPECIFIED);
    tracing::info!(
        lan_ip = %lan_ip,
        interface = %config.network.lan_interface,
        "detected LAN IP for DHCP gateway: lan_ip={lan_ip}, interface={iface}",
        lan_ip = lan_ip,
        iface = config.network.lan_interface,
    );

    // Build real network backends (Linux kernel interfaces).
    let executor = Arc::new(wardnetd::command::ShellCommandExecutor);
    let packet_capture = Arc::new(PnetCapture);
    let blocklist_fetcher: Arc<dyn BlocklistFetcher> = Arc::new(HttpBlocklistFetcher::new());

    // Auto-update backends. The short arch name is resolved from the compile-
    // time target triple injected by build.rs (`WARDNET_TARGET`); falling back
    // to the runtime `ARCH` keeps dev builds working even if the env var is
    // absent.
    let target_triple = option_env!("WARDNET_TARGET").unwrap_or("");
    let arch =
        wardnetd_services::update::short_arch(target_triple).unwrap_or(std::env::consts::ARCH);
    let update_backends = UpdateBackends {
        release_source: Arc::new(
            HttpsManifestSource::new(
                &config.update.manifest_base_url,
                arch,
                Duration::from_secs(config.update.http_timeout_secs),
            )
            .expect("failed to build release manifest source"),
        ),
        verifier: Arc::new(Sha256MinisignVerifier::new(EMBEDDED_PUBLIC_KEY)),
        applier: Arc::new(
            FsBinaryApplier::new(
                config.update.live_binary_path.clone(),
                config.update.staging_dir.clone(),
            )
            .with_postupgrade(
                std::path::PathBuf::from(wardnetd_services::update::POSTUPGRADE_DIR),
                EMBEDDED_PUBLIC_KEY,
            ),
        ),
    };

    let host_id = sysinfo::System::host_name().unwrap_or_else(|| "wardnet".to_owned());

    // One shutdown token shared by three consumers:
    //   - `shutdown_signal` below (waits on it alongside SIGINT/SIGTERM)
    //   - `SystemService::request_restart` (cancels it to trigger restart)
    //   - cleanup logic after `axum::serve` returns
    // Cancelling it from any source drives the same graceful-shutdown path.
    let shutdown_token = tokio_util::sync::CancellationToken::new();

    // Build the repository factory up front so `PnetGarpOps` can hold a
    // `SystemConfigRepository` handle (it reads `dhcp_router_ip` /
    // `garp_router_mac` at broadcast time). Admin bootstrap is run
    // explicitly here because we're calling `init_services_with_factory`
    // below — that helper deliberately skips admin bootstrap.
    let repo_factory = wardnetd_data::create_repository_factory(&config).await?;
    let admin_credentials = config
        .admin
        .as_ref()
        .map(|a| (a.username.as_str(), a.password.as_str()));
    wardnetd_data::bootstrap::bootstrap_admin(&repo_factory.admin(), admin_credentials).await?;
    let system_config_repo = repo_factory.system_config();

    // Hold a clone of `garp_ops` outside `Backends` so the shutdown
    // block (after `init_services_with_factory` has consumed
    // `Backends`) can still call `broadcast_farewell`. Mirrors how
    // `shutdown_token` is held.
    let garp_ops: Arc<dyn wardnetd_services::garp::GarpOps> = Arc::new(PnetGarpOps::new(
        config.network.lan_interface.clone(),
        system_config_repo.clone(),
    ));

    // Hardware watchdog backend. Opened here (like `garp_ops`) so a clone can
    // be held for `HardwareWatchdogRunner` after `Backends` is consumed. When
    // `watchdog.enabled = false`, or on a board without `/dev/watchdog`, this
    // is an unavailable no-op and the daemon runs without the hardware
    // backstop. The pet is ungated by health — see issue #214.
    let watchdog_ops: Arc<dyn WatchdogOps> = if config.watchdog.enabled {
        Arc::new(LinuxWatchdog::open(
            config.watchdog.device_path.clone(),
            config.watchdog.hardware_timeout_secs,
        ))
    } else {
        tracing::info!("hardware watchdog disabled by config (watchdog.enabled = false)");
        Arc::new(LinuxWatchdog::disabled())
    };

    // Secret store is hoisted out of the `Backends` literal because the `:443`
    // TLS config is seeded from the stored cert (read through the SecretStore
    // abstraction) *before* services are wired — the `CertActivator` must exist
    // by the time `init_services_with_factory` builds the TLS service.
    let secret_store = build_secret_store(config.secret_store.as_ref());

    // Build the `:443` serving identity: seed from the stored real cert + its
    // recorded domain if one exists (provisioned), else from a placeholder
    // self-signed cert (unprovisioned). A SecretStore/config error at boot is
    // non-fatal — fall back to the placeholder so `:7411` and `:443` still come up.
    let stored_cert = match wardnetd_services::tls::load_stored_cert(secret_store.as_ref()).await {
        Ok(seed) => seed,
        Err(e) => {
            tracing::warn!(error = %e, "failed to read stored TLS cert; starting with placeholder");
            None
        }
    };
    let stored_cert_domain =
        match wardnetd_services::tls::load_stored_cert_domain(system_config_repo.as_ref()).await {
            Ok(domain) => domain,
            Err(e) => {
                tracing::warn!(error = %e, "failed to read stored TLS cert domain");
                None
            }
        };
    let (rustls_config, serving_control) =
        tls_server::build_serving_control(stored_cert, stored_cert_domain)
            .await
            .map_err(|e| anyhow::anyhow!("failed to build :443 TLS config: {e}"))?;
    let cert_activator: Arc<dyn wardnetd_services::CertActivator> = serving_control.clone();

    let backends = Backends {
        tunnel_interface: Arc::new(WireGuardTunnelInterface),
        tunnel_exit_probe: Arc::new(ReqwestTunnelExitProbe::new(
            config.tunnel.test_probe_url.clone(),
        )),
        tunnel_latency_prober: Arc::new(SurgePingTunnelLatencyProber::new(
            config
                .tunnel
                .latency_probe_target
                .parse()
                .expect("tunnel.latency_probe_target must be a valid IP address"),
        )),
        tunnel_throughput_tester: Arc::new(HttpThroughputTester::new(
            config.tunnel.speed_test_url.clone(),
        )),
        policy_router: Arc::new(
            NetlinkPolicyRouter::new(executor.clone())
                .expect("failed to initialise netlink policy router"),
        ),
        firewall: Arc::new(NetlinkFirewallManager::new()),
        packet_capture: packet_capture.clone(),
        hostname_resolver: Arc::new(SystemHostnameResolver),
        secret_store: secret_store.clone(),
        blocklist_fetcher: blocklist_fetcher.clone(),
        update: update_backends,
        config_path: config_path.clone(),
        host_id,
        shutdown_token: shutdown_token.clone(),
        power_ops: Arc::new(SystemctlPowerOps::new(executor.clone())),
        network_inspector: Arc::new(ProcNetNetworkInspector::new(
            config.network.lan_interface.clone(),
            lan_ip,
        )),
        network_probe: Arc::new(PnetNetworkProbe::new(config.network.lan_interface.clone())),
        garp_ops: garp_ops.clone(),
        cert_activator: cert_activator.clone(),
        watchdog_ops: watchdog_ops.clone(),
    };

    // Wire services. Admin bootstrap already ran above; this helper
    // seeds the setup-wizard / default-policy keys and classifies the
    // previous shutdown.
    let services = init_services_with_factory(
        repo_factory.as_ref(),
        backends,
        &config,
        lan_ip,
        started_at,
        log_service.clone(),
    )
    .await?;

    // Restore state from the database.
    services
        .tunnel
        .restore_tunnels()
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    services
        .discovery
        .restore_devices()
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    // Capture the root span so background tasks can create child spans that
    // inherit the `wardnetd{version=...}` context.
    let root_span = tracing::Span::current();

    // Reconcile routing state with kernel on startup.
    auth_context::with_context(
        AuthContext::Admin {
            admin_id: uuid::Uuid::nil(),
        },
        services.routing.reconcile(),
    )
    .await
    .map_err(|e| anyhow::anyhow!("{e}"))?;

    // Seed the convenience `wardnet.lan -> <lan_ip>` system record so LAN clients
    // can reach the Pi by a friendly name (and `http://wardnet` works via the
    // search-domain hop + `:80` redirect). Cert-independent, so seeded at boot;
    // idempotent (`source = system` upsert) and non-fatal.
    {
        use wardnet_common::api::UpsertRecordRequest;
        use wardnet_common::dns::{DnsRecordSource, DnsRecordType};
        // The seeded `.lan` zone id (see `20260414000000_dns.sql`).
        const LAN_ZONE_ID: uuid::Uuid = uuid::uuid!("00000000-0000-0000-0000-000000000010");
        let req = UpsertRecordRequest {
            zone_id: Some(LAN_ZONE_ID),
            domain: "wardnet.lan".to_owned(),
            record_type: DnsRecordType::A,
            value: lan_ip.to_string(),
            ttl: 300,
            enabled: true,
            source: DnsRecordSource::System,
        };
        match auth_context::with_context(
            AuthContext::Admin {
                admin_id: uuid::Uuid::nil(),
            },
            services.dns_local.upsert_record(req),
        )
        .await
        {
            Ok(_) => tracing::info!(
                lan_ip = %lan_ip,
                "seeded wardnet.lan -> {lan_ip} system record",
                lan_ip = lan_ip,
            ),
            Err(e) => {
                tracing::warn!(error = %e, "failed to seed wardnet.lan system record; continuing bootstrap");
            }
        }
    }

    // Broadcast the GARP claim *before* any user-facing runner starts,
    // so managed clients see "dhcp_router_ip is at the Pi's MAC again"
    // by the time DHCP/DNS bind their sockets. ~1.1s of bounded boot
    // latency (2 pulses × 500ms gap) buys deterministic acceptance —
    // see issue #213, decision 5. Failures swallowed: the GARP path
    // must never block bootstrap.
    if let Err(e) = auth_context::with_context(
        AuthContext::Admin {
            admin_id: uuid::Uuid::nil(),
        },
        garp_ops.broadcast_claim(),
    )
    .await
    {
        tracing::warn!(error = %e, "GARP claim failed; continuing bootstrap");
    }

    // Start background monitors.
    let monitor = TunnelMonitor::start(
        services.tunnel.clone(),
        config.tunnel.stats_interval_secs,
        config.tunnel.health_check_interval_secs,
        config.tunnel.latency_probe_interval_secs,
        &root_span,
    );
    let idle_watcher = IdleTunnelWatcher::start(
        services.event_publisher.clone(),
        services.tunnel.clone(),
        services.routing.clone(),
        config.tunnel.idle_timeout_secs,
        &root_span,
    );
    let routing_listener = RoutingListener::start(
        &services.event_publisher,
        services.routing.clone(),
        &root_span,
    );
    let route_monitor = RouteMonitor::start(services.event_publisher.clone(), &root_span)
        .map_err(|e| anyhow::anyhow!("failed to start route monitor: {e}"))?;

    // Heartbeat: refreshes `last_seen_at` in `system_config` every
    // 60 seconds so an unclean shutdown's reported timestamp is
    // accurate to within the heartbeat window.
    let heartbeat_runner = HeartbeatRunner::start(services.system.clone(), &root_span);

    // mDNS advertiser: publishes `<hostname>.local.` (default
    // `wardnet.local.`) so the setup wizard is reachable without
    // knowing the LAN IP. Failure here is non-fatal — the wizard
    // remains reachable by IP.
    let mdns_advertiser = if config.mdns.enabled {
        match MdnsAdvertiser::start(
            &config.mdns,
            &config.network.lan_interface,
            lan_ip,
            config.server.port,
            env!("WARDNET_VERSION"),
            &root_span,
        ) {
            Ok(advertiser) => Some(advertiser),
            Err(e) => {
                tracing::warn!(error = %e, "failed to start mDNS advertiser; continuing without it: {e}");
                None
            }
        }
    } else {
        tracing::info!("mDNS advertiser disabled");
        None
    };

    // Start device detector (conditionally).
    let device_detector = if config.detection.enabled {
        Some(DeviceDetector::start(
            packet_capture.clone(),
            services.discovery.clone(),
            system_config_repo.clone(),
            services.event_publisher.as_ref(),
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
            services.system.clone(),
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

    // Build and start DHCP server and runner.
    // Load initial DHCP config under admin context.
    // gateway_ip is injected by DhcpServiceImpl from the detected LAN IP.
    let admin_ctx = AuthContext::Admin {
        admin_id: uuid::Uuid::nil(),
    };
    let initial_dhcp_config = auth_context::with_context(
        admin_ctx.clone(),
        services.dhcp.get_dhcp_config(),
    )
    .await
    .unwrap_or_else(|e| {
        tracing::warn!(error = %e, "failed to load initial DHCP config, using defaults: error={e}");
        {
            // Derive default pool range from the detected LAN IP subnet.
            let octets = lan_ip.octets();
            wardnet_common::dhcp::DhcpConfig {
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
    let dhcp_server: Arc<dyn wardnetd_services::dhcp::server::DhcpServer> = Arc::new(
        wardnetd::dhcp::server::UdpDhcpServer::new(services.dhcp.clone(), initial_dhcp_config),
    );
    let dhcp_runner = DhcpRunner::start(
        services.dhcp.clone(),
        dhcp_server.clone(),
        services.event_publisher.as_ref(),
        &root_span,
    );

    // Build and start DNS server, the slim DnsRunner (server lifecycle
    // only), and the new DnsFilterRunner (blocklist cron + filter rebuilds
    // driven by `WardnetEvent::DnsFilterChanged` / `DeviceIpChanged`).
    let dns_server: Arc<dyn wardnetd_services::dns::server::DnsServer> = Arc::new(
        wardnetd::dns::server::UdpDnsServer::new(
            wardnet_common::dns::DnsConfig::default(),
            services.dns_filter.clone(),
            services.routing.dns_upstream_snapshot(),
            services.tunnel_repo.clone(),
            services.event_publisher.clone(),
        )
        .with_log_sink(services.dns_log_sink.clone()),
    );
    let dns_runner = DnsRunner::start(
        services.dns.clone(),
        dns_server.clone(),
        services.dns_local.clone(),
        services.event_publisher.as_ref(),
        &root_span,
    );
    let dns_filter_runner = DnsFilterRunner::start(
        services.dns_filter.clone(),
        services.event_publisher.as_ref(),
        &root_span,
        Duration::from_mins(1),
    );

    // Auto-register `{hostname}.lan` A records as DHCP leases are assigned
    // or renewed. Goes through the auth-gated local-DNS service so the
    // authoritative view rebuild (and cache eviction) fire automatically.
    let dhcp_lan_runner = DhcpLanRunner::start(
        services.dns_local.clone(),
        services.dhcp.clone(),
        services.event_publisher.as_ref(),
        &root_span,
    );

    // Drain the DNS query log persistence channel into SQLite and trim the
    // table once a day. The receiver is taken out of `Services` exactly
    // once at startup; the sink stays in `Services` so the API layer can
    // subscribe to live-stream events.
    let dns_log_persist_rx = services
        .dns_log_persist_rx
        .lock()
        .expect("dns log persist rx lock poisoned")
        .take()
        .expect("dns log persist rx taken twice");
    let dns_query_log_runner = DnsQueryLogRunner::start(
        services.dns.clone(),
        services.dns_log_sink.clone(),
        dns_log_persist_rx,
        &root_span,
    );

    let dns_capture_rx = services
        .dns_capture_rx
        .lock()
        .expect("dns capture rx lock poisoned")
        .take()
        .expect("dns capture rx taken twice");
    let dns_capture_runner = DnsCaptureRunner::start(
        dns_capture_rx,
        services.device.clone(),
        services.dns_events_repo.clone(),
        services.event_publisher.clone(),
        &root_span,
    );

    // Return freed SQLite pages to the filesystem once per day — fires
    // regardless of any per-feature flag so it covers all tables.
    let db_maintenance_runner =
        DbMaintenanceRunner::start(services.maintenance.clone(), &root_span);

    // Start the auto-update poller. An initial check runs immediately; then
    // every `check_interval_secs` with ±10% jitter.
    let update_runner = UpdateRunner::start(
        services.update.clone(),
        Duration::from_secs(config.update.check_interval_secs),
        &root_span,
    );

    // Backup cleanup runner — trims `.bak-<ts>` siblings older than
    // 24 h. Hourly sweep, same root span as the other background tasks
    // so every log line carries the daemon version.
    let backup_cleanup_runner = wardnetd_services::backup::BackupCleanupRunner::start(
        services.backup.clone(),
        Duration::from_hours(1),
        wardnetd_services::backup::DEFAULT_SNAPSHOT_RETENTION,
        &root_span,
    );

    let stats_flush_runner = wardnetd_services::StatsFlushRunner::start(
        services.stats_buffer.clone(),
        services.stats.clone(),
        &root_span,
    );

    // DDNS update runner — keeps the public A record current. Inert (no network
    // calls) until a DDNS provider is configured by the setup wizard.
    let ddns_update_runner = DdnsUpdateRunner::start(services.ddns.clone(), &root_span);

    // TLS renewal runner — issues the cert once DDNS is configured and renews it
    // before expiry, hot-swapping the live `:443` cert. Inert (no ACME calls)
    // while there is no active FQDN.
    let tls_renewal_runner = TlsRenewalRunner::start(services.tls.clone(), &root_span);

    // Health monitor (issue #214): register the four startup probes — DB
    // connectivity, liveness (a fresh snapshot containing it proves the
    // refresh loop schedules), and the two always-on core LAN services (DNS,
    // DHCP). Refresh once synchronously so the first published snapshot
    // reflects real checks before READY=1 is sent and the soft watchdog reads
    // it.
    let mut health_monitor = HealthMonitor::new(
        config.health.failure_threshold,
        Duration::from_secs(config.health.check_timeout_secs),
    );
    health_monitor.register(Arc::new(LivenessHealthCheck));
    health_monitor.register(Arc::new(DbHealthCheck::new(repo_factory.maintenance())));
    health_monitor.register(Arc::new(DnsServerHealthCheck::new(
        services.dns.clone(),
        dns_server.clone(),
    )));
    health_monitor.register(Arc::new(DhcpServerHealthCheck::new(
        services.dhcp.clone(),
        dhcp_server.clone(),
    )));
    let health_monitor = Arc::new(health_monitor);
    health_monitor.refresh().await;

    // Health refresh loop.
    let health_runner = HealthMonitorRunner::start_with_interval(
        health_monitor.clone(),
        Duration::from_secs(config.health.refresh_interval_secs),
        &root_span,
    );

    // Soft watchdog: health-gated sd_notify(WATCHDOG=1). Tick at WATCHDOG_USEC/2
    // when systemd supervises us, else fall back to the health interval. A
    // snapshot older than 2× the refresh interval is treated as stale (the
    // refresh loop stalled) and withholds the ping.
    let notifier: Arc<dyn Notifier> = Arc::new(SdNotifier);
    let soft_watchdog_runner = if config.watchdog.soft_enabled {
        let tick = SdNotifier::recommended_interval()
            .unwrap_or_else(|| Duration::from_secs(config.health.refresh_interval_secs));
        let staleness =
            Duration::from_secs(config.health.refresh_interval_secs.saturating_mul(2).max(1));
        Some(SoftWatchdogRunner::start(
            health_monitor.clone(),
            notifier.clone(),
            tick,
            staleness,
            &root_span,
        ))
    } else {
        tracing::info!("soft watchdog disabled by config (watchdog.soft_enabled = false)");
        None
    };

    // Hard watchdog: ungated /dev/watchdog pet. No-op when the device is
    // unavailable. Never consults health — this is the total-freeze backstop.
    let hardware_watchdog_runner = HardwareWatchdogRunner::start(
        watchdog_ops.clone(),
        Duration::from_secs(config.watchdog.pet_interval_secs),
        &root_span,
    );

    let state = AppState::new(
        services.auth.clone(),
        services.backup.clone(),
        services.device.clone(),
        services.dhcp.clone(),
        services.dns.clone(),
        services.dns_filter.clone(),
        services.dns_local.clone(),
        services.ddns.clone(),
        services.tls.clone(),
        services.discovery.clone(),
        log_service.clone(),
        services.vpn_provider.clone(),
        services.routing.clone(),
        services.system.clone(),
        services.tunnel.clone(),
        services.update.clone(),
        dhcp_server,
        dns_server,
        services.event_publisher.clone(),
        services.jobs.clone(),
        services.stats.clone(),
        services.rule_request.clone(),
    )
    .with_health_monitor(health_monitor.clone());

    let app = wardnetd_api::api::router(state);

    let addr: SocketAddr = format!("{}:{}", config.server.host, config.server.port).parse()?;
    let https_addr: SocketAddr =
        format!("{}:{}", config.server.host, config.server.https_port).parse()?;
    let redirect_addr: SocketAddr = format!(
        "{}:{}",
        config.server.host, config.server.http_redirect_port
    )
    .parse()?;

    println!(
        "\n  Wardnet daemon v{}\n  Listening on http://{} (plain) and https://{} (TLS)\n  Database: {}\n",
        env!("WARDNET_VERSION"),
        addr,
        https_addr,
        config.database.connection_string,
    );

    let _pidfile = match wardnetd::pidfile::PidfileGuard::write(&config.pidfile_path) {
        Ok(guard) => {
            tracing::info!(
                path = %config.pidfile_path.display(),
                "pidfile written: path={}",
                config.pidfile_path.display()
            );
            Some(guard)
        }
        Err(e) => {
            tracing::warn!(
                error = %e,
                path = %config.pidfile_path.display(),
                "failed to write pidfile at {}: {e}",
                config.pidfile_path.display()
            );
            None
        }
    };

    let listener = TcpListener::bind(addr).await?;

    // Three listeners share `shutdown_token`:
    //   - `:7411` plain HTTP — the pre-provisioning admin surface, never guarded.
    //   - `:443` HTTPS — always bound (placeholder cert until issuance), with a
    //     503 guard until TLS is provisioned. Spawned.
    //   - `:80` — 308-redirects to HTTPS. Spawned.
    // The `:443`/`:80` tasks drain on cancellation; we join them after the
    // blocking `:7411` serve returns, before the teardown sequence.
    let serving: Arc<dyn tls_server::ServingIdentity> = serving_control.clone();
    let https_app = tls_server::guarded_https_app(app.clone(), serving.clone());

    // Bind the `:443` and `:80` listeners **synchronously** before signalling
    // readiness, so `READY=1` can never precede a listener bind (issue #214).
    // Both are non-fatal: a bind failure (e.g. port already in use) is logged
    // and that listener is skipped — the daemon still serves the primary
    // `:7411` surface.
    let https_handle = match std::net::TcpListener::bind(https_addr) {
        Ok(std_listener) => match std_listener.set_nonblocking(true) {
            Ok(()) => Some(tls_server::spawn_https_listener(
                std_listener,
                https_app,
                rustls_config,
                &shutdown_token,
                &root_span,
            )),
            Err(e) => {
                tracing::error!(error = %e, %https_addr, "failed to set :443 listener non-blocking; skipping HTTPS: {e}");
                None
            }
        },
        Err(e) => {
            tracing::error!(error = %e, %https_addr, "failed to bind :443 listener; skipping HTTPS: {e}");
            None
        }
    };
    let http_redirect_handle = match TcpListener::bind(redirect_addr).await {
        Ok(redirect_listener) => Some(tls_server::spawn_http_redirect_listener(
            redirect_listener,
            config.server.https_port,
            serving,
            &shutdown_token,
            &root_span,
        )),
        Err(e) => {
            tracing::error!(error = %e, %redirect_addr, "failed to bind :80 redirect listener; skipping redirect: {e}");
            None
        }
    };

    // Every listener that could bind is now bound (`:7411`, plus `:443`/`:80`
    // when available). Tell systemd we're ready: with `Type=notify` it only
    // marks the unit `active (running)` once READY=1 arrives, so this gates the
    // watchdog supervision and `systemctl start`'s return. No-op outside systemd.
    notifier.notify_ready();

    let api_span = tracing::info_span!(parent: &root_span, "api_server");
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal(shutdown_token.clone()))
    .into_future()
    .instrument(api_span)
    .await?;

    // `:7411`'s graceful-shutdown future returns on SIGINT/SIGTERM *without*
    // cancelling the token, so cancel it explicitly to drive the spawned `:443`
    // and `:80` listeners (and any token-watching tasks) into shutdown, then
    // wait for them to drain before tearing the rest down.
    shutdown_token.cancel();
    if let Some(handle) = https_handle {
        let _ = handle.await;
    }
    if let Some(handle) = http_redirect_handle {
        let _ = handle.await;
    }

    tracing::info!("server stopped, broadcasting GARP farewell before tearing down");
    // Farewell goes out *before* anything else in the shutdown sequence
    // so managed clients fall back to the upstream router while the rest
    // of the daemon is still alive on the LAN. Failures swallowed: a
    // GARP error must not block routing/DHCP/DNS cleanup. See issue
    // #213, decision 3.
    if let Err(e) = auth_context::with_context(
        AuthContext::Admin {
            admin_id: uuid::Uuid::nil(),
        },
        garp_ops.broadcast_farewell(),
    )
    .await
    {
        tracing::warn!(error = %e, "GARP farewell failed; continuing shutdown");
    }

    tracing::info!("shutting down background tasks");
    // Hardware watchdog first: disarm (magic close) before the rest of the
    // teardown runs, so a slow shutdown can't trip a reboot. Then the soft
    // watchdog and the health loop.
    hardware_watchdog_runner.shutdown().await;
    if let Some(runner) = soft_watchdog_runner {
        runner.shutdown().await;
    }
    health_runner.shutdown().await;
    routing_listener.shutdown().await;
    route_monitor.shutdown().await;
    idle_watcher.shutdown().await;
    monitor.shutdown().await;
    dhcp_runner.shutdown().await;
    dns_runner.shutdown().await;
    dns_filter_runner.shutdown().await;
    dhcp_lan_runner.shutdown().await;
    dns_query_log_runner.shutdown().await;
    dns_capture_runner.shutdown().await;
    db_maintenance_runner.shutdown().await;
    update_runner.shutdown().await;
    backup_cleanup_runner.shutdown().await;
    stats_flush_runner.shutdown().await;
    ddns_update_runner.shutdown().await;
    tls_renewal_runner.shutdown().await;
    heartbeat_runner.shutdown().await;
    if let Some(advertiser) = mdns_advertiser {
        advertiser.shutdown().await;
    }
    if let Some(detector) = device_detector {
        detector.shutdown().await;
    }
    if let Some(collector) = metrics_collector {
        collector.shutdown().await;
    }
    if let Some(agent) = profiling_agent {
        agent.shutdown();
    }

    // Mark this shutdown as graceful so the next boot's
    // `detect_previous_shutdown` classifies the run as `Graceful` and
    // the dashboard does not surface an unclean-shutdown banner.
    // Wrapped in `auth_context::with_context` because the call lives
    // outside the HTTP middleware chain. Failure here is logged but
    // never escalated — the worst-case is a benign false-positive
    // banner on the next boot.
    let admin_ctx = AuthContext::Admin {
        admin_id: uuid::Uuid::nil(),
    };
    if let Err(e) =
        auth_context::with_context(admin_ctx, services.system.record_graceful_shutdown()).await
    {
        tracing::warn!(error = %e, "failed to record graceful shutdown marker: {e}");
    }

    Ok(())
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
        .with_interval(Duration::from_secs(config.interval_secs))
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
    config: &ApplicationConfiguration,
    cli_log_level: Option<&str>,
    cli_log_path: Option<&Path>,
    verbose: bool,
    log_service: &dyn LogService,
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
    // Log service layers (BoxedLayer = Box<dyn Layer<Registry>>) must be applied
    // directly on the Registry, so they come first before any other layers.
    let log_layers = log_service.tracing_layers();
    let registry = tracing_subscriber::registry()
        .with(log_layers)
        .with(filter)
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

    log_service.start_all();

    let otel_meter_provider = init_otel_meter(&config.otel);

    TracingGuards {
        _log_guard: guard,
        otel_tracer_provider,
        otel_logger_provider,
        otel_meter_provider,
    }
}

async fn shutdown_signal(restart_token: tokio_util::sync::CancellationToken) {
    let ctrl_c = tokio::signal::ctrl_c();

    #[cfg(unix)]
    {
        let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler");
        tokio::select! {
            _ = ctrl_c => {},
            _ = sigterm.recv() => {},
            () = restart_token.cancelled() => {},
        }
    }

    #[cfg(not(unix))]
    {
        tokio::select! {
            _ = ctrl_c => {},
            () = restart_token.cancelled() => {},
        }
    }

    tracing::info!("shutdown signal received");
}
