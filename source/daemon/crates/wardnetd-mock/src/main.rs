//! Mock daemon binary entry point.
//!
//! Builds `ApplicationConfiguration` from CLI args + defaults, initialises a
//! (normally in-memory) `SQLite` pool, seeds demo data, wires the services
//! against no-op backends, and serves the HTTP API on the requested loopback
//! address.

use std::future::IntoFuture;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use clap::Parser;
use tokio::net::TcpListener;
use tracing::Instrument;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use wardnet_common::config::ApplicationConfiguration;
use wardnetd_api::state::AppState;
use wardnetd_data::create_repository_factory;
use wardnetd_mock::backends::noop_cert_activator::NoopCertActivator;
use wardnetd_mock::backends::noop_device::{
    NoopDeviceProber, NoopHostnameResolver, NoopPacketCapture,
};
use wardnetd_mock::backends::noop_dhcp::NoopDhcpServer;
use wardnetd_mock::backends::noop_dns::NoopDnsServer;
use wardnetd_mock::backends::noop_exit_probe::NoopExitProbe;
use wardnetd_mock::backends::noop_garp::NoopGarpOps;
use wardnetd_mock::backends::noop_inbound_wg::NoopInboundWgInterface;
use wardnetd_mock::backends::noop_latency_prober::NoopLatencyProber;
use wardnetd_mock::backends::noop_network_inspector::NoopNetworkInspector;
use wardnetd_mock::backends::noop_network_probe::NoopNetworkProbe;
use wardnetd_mock::backends::noop_power_ops::NoopSystemPowerOps;
use wardnetd_mock::backends::noop_private_dns::MockPrivateDnsService;
use wardnetd_mock::backends::noop_remote_access::{
    MockDdnsService, MockRemoteAccessState, MockTlsService,
};
use wardnetd_mock::backends::noop_routing::{NoopFirewallManager, NoopPolicyRouter};
use wardnetd_mock::backends::noop_throughput_tester::NoopThroughputTester;
use wardnetd_mock::backends::noop_tunnel::NoopTunnelInterface;
use wardnetd_mock::backends::noop_watchdog::NoopWatchdog;
use wardnetd_mock::events::FakeEventEmitter;
use wardnetd_mock::seed;
use wardnetd_services::db_maintenance_runner::DbMaintenanceRunner;
use wardnetd_services::device::DeviceRetentionRunner;
use wardnetd_services::diagnostics::DiagnosticStore;
use wardnetd_services::diagnostics::listener::DiagnosticsListener;
use wardnetd_services::dns::DnsCaptureRunner;
use wardnetd_services::dns::query_log_runner::DnsQueryLogRunner;
use wardnetd_services::dns_filter::blocklist_downloader::HttpBlocklistFetcher;
use wardnetd_services::health::checks::{DbHealthCheck, LivenessHealthCheck};
use wardnetd_services::logging::{LogService, LogServiceImpl, LogStreamService};
use wardnetd_services::secret_store::FileSecretStore;
use wardnetd_services::stats::flush_runner::{DEFAULT_FLUSH_INTERVAL, StatsFlushRunner};
use wardnetd_services::update::{
    EMBEDDED_PUBLIC_KEY, FsBinaryApplier, HttpsManifestSource, Sha256MinisignVerifier,
};
use wardnetd_services::{Backends, HealthMonitor, UpdateBackends, init_services_with_factory};

/// Wardnet mock daemon — local HTTP API for web-ui development.
#[derive(Parser, Debug)]
#[command(
    name = "wardnetd-mock",
    about = "Runs the Wardnet API on loopback with no-op network backends and seeded demo data."
)]
// Independent on/off CLI flags are the natural shape for a clap struct.
#[allow(clippy::struct_excessive_bools)]
struct Cli {
    /// `SQLite` connection string. Use `:memory:` (default) for an ephemeral
    /// database or a file path for on-disk persistence between runs.
    #[arg(long, default_value = ":memory:")]
    database: String,

    /// Loopback host to bind. Never accept non-loopback values — this is a
    /// dev tool, not a daemon.
    #[arg(long, default_value = "127.0.0.1")]
    host: String,

    /// TCP port to listen on.
    #[arg(long, default_value_t = 7411)]
    port: u16,

    /// Skip demo data seeding. Useful when `--database` points at an on-disk
    /// file that has already been populated.
    #[arg(long)]
    no_seed: bool,

    /// Disable the periodic fake event emitter.
    #[arg(long)]
    no_events: bool,

    /// Deliver Web Push notifications for real (through the browser vendors'
    /// push services) instead of the default no-op sender. Lets local dev
    /// exercise the full notification flow — subscribe in the admin PWA, then
    /// e.g. change a device's own routing in the user PWA to trigger an admin
    /// push. Requires internet access.
    #[arg(long)]
    real_push: bool,

    /// Enable debug-level logging for all wardnet crates.
    #[arg(long, short)]
    verbose: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Build a default configuration and override the pieces the mock needs.
    let mut config = ApplicationConfiguration::default();
    config.server.host.clone_from(&cli.host);
    config.server.port = cli.port;
    config.database.connection_string.clone_from(&cli.database);
    "mock0".clone_into(&mut config.network.lan_interface);
    // Explicitly clear any admin bootstrap — the setup wizard must run.
    config.admin = None;
    // Keep logging entirely to stderr; don't let the daemon's file appender kick in.
    config.logging.path = PathBuf::from("/tmp/wardnetd-mock.log");
    // Disable device detection so no background task pokes the fake capture.
    config.detection.enabled = false;
    // Disable OTel / Pyroscope — mock is a dev aid, not observable infra.
    config.otel.enabled = false;
    config.pyroscope.enabled = false;

    // Build the log service BEFORE init_tracing so its tracing layers are
    // attached to the subscriber. This is what feeds the /system/logs/stream
    // websocket the web UI subscribes to.
    let log_stream = Arc::new(
        LogStreamService::new(config.logging.broadcast_capacity)
            .with_suppressed_targets(config.logging.ui_suppressed_targets.clone()),
    );
    // Recent-diagnostics buffer: read handle for the log service, write handle
    // for the diagnostics listener wired up in `run`.
    let diagnostics = Arc::new(DiagnosticStore::new(config.logging.max_recent_errors));
    let log_service: Arc<dyn LogService> = Arc::new(LogServiceImpl::new(
        log_stream,
        diagnostics.clone(),
        config.logging.path.clone(),
    ));

    init_tracing(cli.verbose, log_service.as_ref());

    run(cli, config, log_service, diagnostics).await
}

#[allow(clippy::too_many_lines)]
async fn run(
    cli: Cli,
    config: ApplicationConfiguration,
    log_service: Arc<dyn LogService>,
    diagnostics: Arc<DiagnosticStore>,
) -> anyhow::Result<()> {
    let started_at = Instant::now();

    // Build the repository factory *first* so we can seed data before
    // services wake up and start observing events.
    let factory = create_repository_factory(&config).await?;

    if cli.no_seed {
        tracing::info!("--no-seed set, skipping demo data population");
    } else {
        tracing::info!("seeding demo data...");
        let seeded = seed::populate(factory.as_ref()).await?;
        tracing::info!(
            devices = seeded.device_ids.len(),
            tunnels = seeded.tunnel_ids.len(),
            "demo data seeded: devices={d}, tunnels={t}",
            d = seeded.device_ids.len(),
            t = seeded.tunnel_ids.len(),
        );
    }

    // Seed IDs are only needed by the event emitter; re-read tunnels from the
    // DB so --no-seed + on-disk runs still get realistic data.
    let tunnel_ids_for_events = {
        let tunnels = factory.tunnel().find_all().await?;
        tunnels.into_iter().map(|t| t.id).collect::<Vec<_>>()
    };

    // Re-read devices so the fake-DNS emitter can attribute events to real
    // seeded clients (lights up the top-clients table + filter dropdown) and
    // so the per-device capture pipeline can pick them up. `capture_target` is
    // whichever device has DNS capture enabled (the seed turns it on for the
    // 127.0.0.1 localhost device that the user PWA resolves `/devices/me` to) —
    // keying off the flag keeps the emitter and the seed in sync.
    let (dns_clients, capture_target) = {
        let devices = factory.device().find_all().await?;
        let clients = devices
            .iter()
            .filter(|d| !d.last_ip.is_empty())
            .map(|d| (d.id.to_string(), d.last_ip.clone()))
            .collect::<Vec<(String, String)>>();
        let target = devices
            .iter()
            .find(|d| d.dns_capture_enabled)
            .map(|d| (d.id.to_string(), d.last_ip.clone()));
        if target.is_none() {
            tracing::warn!(
                "no DNS-capture-enabled device found — the user PWA DNS-events \
                 stream will stay empty. On a --no-seed run against an older DB, \
                 delete the DB to re-seed or enable capture on a device."
            );
        }
        (clients, target)
    };

    // No-op backends are used only for subsystems that require Linux kernel
    // APIs unavailable on macOS (WireGuard, netlink routing, nftables, pnet
    // capture). Anything that works cross-platform — like HTTP blocklist
    // fetches and the auto-update pipeline — uses the real implementation so
    // dev testing exercises the actual code path. The auto-update applier is
    // pointed at `/tmp/wardnet-mock/...` so a manually triggered install
    // stages and renames files in a throwaway directory instead of clobbering
    // the dev system binary.
    let mock_update_dir = std::env::temp_dir().join("wardnet-mock");
    let update_backends = UpdateBackends {
        release_source: Arc::new(
            HttpsManifestSource::new(
                &config.update.manifest_base_url,
                wardnetd_services::update::short_arch(std::env::consts::ARCH).unwrap_or("aarch64"),
                Duration::from_secs(config.update.http_timeout_secs),
            )
            .expect("failed to build mock release source"),
        ),
        verifier: Arc::new(Sha256MinisignVerifier::new(EMBEDDED_PUBLIC_KEY)),
        applier: Arc::new(
            FsBinaryApplier::new(
                mock_update_dir.join("wardnetd"),
                mock_update_dir.join("staging"),
            )
            // Mock postupgrade dir lives under the same temp tree so
            // tearing down the mock leaves no /var/lib state behind.
            .with_postupgrade(mock_update_dir.join("postupgrade"), EMBEDDED_PUBLIC_KEY),
        ),
    };
    // Real file-backed secret store rooted under the OS temp directory so
    // the mock exercises the exact same save/load code path as production.
    let mock_secrets_root = std::env::temp_dir().join("wardnet-mock").join("secrets");
    tokio::fs::create_dir_all(&mock_secrets_root)
        .await
        .expect("failed to create mock secret store root");
    let secret_store: Arc<dyn wardnetd_services::secret_store::SecretStore> =
        Arc::new(FileSecretStore::new(mock_secrets_root));

    // Synthetic config path — the mock uses CLI flags rather than a
    // wardnet.toml, so we write an empty placeholder that the backup
    // service can still read on export.
    let mock_config_path = std::env::temp_dir()
        .join("wardnet-mock")
        .join("wardnet.toml");
    if !mock_config_path.exists() {
        if let Some(parent) = mock_config_path.parent() {
            let _ = tokio::fs::create_dir_all(parent).await;
        }
        let _ = tokio::fs::write(&mock_config_path, b"# mock wardnet.toml\n").await;
    }

    // Token is wired through the mock too so the Settings restart
    // button behaves identically in dev — it cancels the token,
    // `shutdown_signal` wakes up, and the mock exits. The operator
    // reruns `make run-dev` to bring it back.
    let shutdown_token = tokio_util::sync::CancellationToken::new();

    // A synthetic LAN IP that looks plausible in UI copy. Declared
    // before `Backends` so the noop network inspector can claim it.
    let lan_ip = std::net::Ipv4Addr::new(192, 168, 1, 1);

    // Web Push: no-op by default (the mock must not reach the network
    // unasked); `--real-push` swaps in the daemon's real sender so local dev
    // can receive actual notifications.
    let web_push_sender: Arc<dyn wardnetd_services::push::sender::WebPushSender> = if cli.real_push
    {
        tracing::info!("--real-push set: delivering Web Push via the real sender");
        Arc::new(wardnetd_services::push::sender::ReqwestWebPushSender::new(
            reqwest::Client::new(),
            wardnetd_services::push::VAPID_CONTACT.to_owned(),
        ))
    } else {
        Arc::new(wardnetd_mock::backends::noop_web_push::NoopWebPushSender)
    };

    let backends = Backends {
        tunnel_interface: Arc::new(NoopTunnelInterface),
        inbound_wg_interface: Arc::new(NoopInboundWgInterface),
        tunnel_exit_probe: Arc::new(NoopExitProbe::new(factory.tunnel())),
        tunnel_latency_prober: Arc::new(NoopLatencyProber::new()),
        tunnel_throughput_tester: Arc::new(NoopThroughputTester::new()),
        policy_router: Arc::new(NoopPolicyRouter),
        firewall: Arc::new(NoopFirewallManager),
        packet_capture: Arc::new(NoopPacketCapture),
        hostname_resolver: Arc::new(NoopHostnameResolver),
        device_prober: Arc::new(NoopDeviceProber),
        secret_store,
        web_push_sender,
        blocklist_fetcher: Arc::new(HttpBlocklistFetcher::new()),
        update: update_backends,
        config_path: mock_config_path,
        host_id: "wardnetd-mock".to_owned(),
        shutdown_token: shutdown_token.clone(),
        power_ops: Arc::new(NoopSystemPowerOps),
        network_inspector: Arc::new(NoopNetworkInspector {
            interface: config.network.lan_interface.clone(),
            ip: lan_ip,
        }),
        network_probe: Arc::new(NoopNetworkProbe),
        garp_ops: Arc::new(NoopGarpOps),
        cert_activator: Arc::new(NoopCertActivator),
        watchdog_ops: Arc::new(NoopWatchdog),
    };

    let services = init_services_with_factory(
        factory.as_ref(),
        backends,
        &config,
        lan_ip,
        started_at,
        log_service.clone(),
    )
    .await?;

    // Personal VPN (inbound WireGuard) and the premium app surfaces (user PWA +
    // admin mobile app) are Premium capabilities gated on `services.entitlement`
    // (the one shared handle the DDNS service owns and inbound-WG reads). The
    // mock always stands in for a wardnet-subscribed box, so mark it entitled
    // up front — in particular before the inbound-WG reconcile below, whose
    // Premium gate would otherwise disable an enabled-across-restart server.
    services.entitlement.set_premium(true);

    // Startup reconcile of the inbound-WireGuard server, mirroring the real
    // daemon (`wardnetd` main): stands the interface back up if enabled and,
    // crucially for the mock's persistent secret store, re-caches the server
    // public key into `system_config` so an enabled-across-restart server never
    // reads back with a null public key.
    if let Err(e) = services.inbound_wg.reconcile().await {
        tracing::warn!(error = %e, "inbound wireguard reconcile failed on startup: {e}");
    }

    // No-op DHCP and DNS servers — services and handlers treat them
    // opaquely so the UI gets consistent start/stop semantics.
    let dhcp_server: Arc<dyn wardnetd_services::dhcp::server::DhcpServer> =
        Arc::new(NoopDhcpServer::default());
    let dns_server: Arc<dyn wardnetd_services::dns::server::DnsServer> =
        Arc::new(NoopDnsServer::default());

    // Remote access (DDNS + TLS): swap in the stateful in-memory mock so the UI's
    // HTTPS/DDNS flow — registration, the issuing→issued progress, the resolution
    // check, and teardown — works fully offline (no real bridge / Cloudflare /
    // Let's Encrypt). The real services from `init_services_with_factory` would
    // reach out to live upstreams; we discard them here. See
    // `backends::noop_remote_access`.
    let remote_access_state = MockRemoteAccessState::new();
    // Both of these are process-local in-memory state (NOT persisted in the
    // DB), so they must be re-established on every boot — including a
    // `--no-seed` resume of an on-disk DB, where the demo data already exists
    // but this state was lost with the previous process. Gating them on seeding
    // meant a kept-DB restart silently dropped premium + the demo DDNS host.
    //
    // Demo DDNS: an already-issued host so remote-access QR codes / `.conf`
    // downloads have a reachable-looking Endpoint out of the box.
    remote_access_state.configure_demo();
    let mock_ddns: Arc<dyn wardnetd_services::ddns::DdnsService> =
        Arc::new(MockDdnsService::new(remote_access_state.clone()));
    let mock_tls: Arc<dyn wardnetd_services::tls::TlsService> =
        Arc::new(MockTlsService::new(remote_access_state.clone()));

    // Health monitor (issue #214): the mock registers only the
    // backend-independent probes (liveness + database). Its noop DNS/DHCP
    // servers never bind a socket, so `is_running()` is false and including
    // them would make `/health` report 503. Refresh once so the snapshot has
    // both components UP; the mock runs no watchdog/refresh runners — those are
    // production-only (no systemd, no /dev/watchdog in dev).
    let mut health_monitor =
        HealthMonitor::new(config.health.failure_threshold, Duration::from_secs(2));
    health_monitor.register(Arc::new(LivenessHealthCheck));
    health_monitor.register(Arc::new(DbHealthCheck::new(factory.maintenance())));
    let health_monitor = Arc::new(health_monitor);
    health_monitor.refresh().await;

    let state = AppState::new(
        services.auth.clone(),
        services.backup.clone(),
        services.device.clone(),
        services.dhcp.clone(),
        services.dns.clone(),
        services.dns_filter.clone(),
        services.dns_local.clone(),
        mock_ddns,
        mock_tls,
        services.discovery.clone(),
        log_service.clone(),
        services.vpn_provider.clone(),
        services.routing.clone(),
        services.network_zone.clone(),
        services.system.clone(),
        services.tunnel.clone(),
        services.update.clone(),
        dhcp_server,
        dns_server,
        services.event_publisher.clone(),
        services.jobs.clone(),
        services.stats.clone(),
        services.rule_request.clone(),
        services.zone_exception.clone(),
    )
    .with_push_service(services.push.clone())
    .with_device_identification_service(services.device_identification.clone())
    .with_routing_profile_service(services.routing_profile.clone())
    .with_inbound_wg_service(services.inbound_wg.clone())
    // Private DNS reaches the live DDNS/TLS/secret store, which the mock stands
    // in for offline; swap in the stateful in-memory fake, mirroring DDNS/TLS.
    .with_private_dns_service(Arc::new(MockPrivateDnsService::default()))
    .with_entitlement(services.entitlement.clone())
    .with_health_monitor(health_monitor);

    // Drain the DNS query log persistence channel into SQLite so the
    // fake live-stream events also populate the historical view.
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
        &tracing::Span::current(),
    );
    // Drain the capture channel into `dns_events` for capture-enabled devices
    // and publish `DnsEventInserted` — this is what feeds the user PWA's
    // `/devices/me/dns-events/stream` SSE during local dev.
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
        &tracing::Span::current(),
    );
    let db_maintenance_runner =
        DbMaintenanceRunner::start(services.maintenance.clone(), &tracing::Span::current());

    // Device retention (#1181). Runs here too so the mock daemon exercises the
    // same day-rollover path as production; with a fresh mock DB there is never
    // anything 30 days stale to delete, so it is a no-op in practice.
    let device_retention_runner =
        DeviceRetentionRunner::start(services.discovery.clone(), &tracing::Span::current());

    // Drain the in-memory stats buffer into stats_intraday so the fake DNS
    // queries emitted by FakeEventEmitter show up in the live stats charts.
    // Use a 5-minute maintenance interval (vs 1 h in production) and no
    // startup grace so the hourly rollup runs quickly for local dev.
    let stats_flush_runner = StatsFlushRunner::start_with_intervals(
        services.stats_buffer.clone(),
        services.stats.clone(),
        DEFAULT_FLUSH_INTERVAL,
        Duration::from_mins(5),
        Duration::ZERO,
        &tracing::Span::current(),
    );

    // Start the fake event emitter (unless disabled).
    let emitter = if cli.no_events {
        tracing::info!("--no-events set, skipping fake event emitter");
        None
    } else {
        Some(FakeEventEmitter::start(
            services.event_publisher.clone(),
            tunnel_ids_for_events,
            services.dns_log_sink.clone(),
            dns_clients,
            capture_target,
        ))
    };

    // Forward domain events to the push service, exactly like the real
    // daemon. Always on: even with the no-op sender this persists the
    // admin notification feed, so the System screen fills up during dev;
    // with `--real-push` it also delivers.
    let push_listener = wardnetd_services::push::listener::PushNotificationListener::start(
        &services.event_publisher,
        services.push.clone(),
        &tracing::Span::current(),
    );

    // Surface error-flavoured fake events in the dashboard's recent-errors
    // panel, exactly like the real daemon.
    let _diagnostics_listener = DiagnosticsListener::start(
        &services.event_publisher,
        diagnostics,
        &tracing::Span::current(),
    );

    let app = wardnetd_api::api::router(state);
    let addr: SocketAddr = format!("{}:{}", config.server.host, config.server.port).parse()?;

    println!(
        "\n  wardnetd-mock\n  Listening on http://{}\n  Database: {}\n  (Setup wizard runs on every launch - no admin is seeded.)\n",
        addr, config.database.connection_string,
    );

    let listener = TcpListener::bind(addr).await?;
    let api_span = tracing::info_span!("mock_api_server");
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal(shutdown_token.clone()))
    .into_future()
    .instrument(api_span)
    .await?;

    tracing::info!("mock server stopped, shutting down emitter");
    if let Some(emitter) = emitter {
        emitter.shutdown().await;
    }
    push_listener.shutdown().await;
    dns_query_log_runner.shutdown().await;
    dns_capture_runner.shutdown().await;
    db_maintenance_runner.shutdown().await;
    device_retention_runner.shutdown().await;
    stats_flush_runner.shutdown().await;

    Ok(())
}

/// Initialise the tracing subscriber for the mock.
///
/// Attaches the `LogService` layers first (so `/system/logs/stream` actually
/// receives events), then the filter, then a stderr formatter for local
/// terminal output. `start_all` kicks off the log service background tasks.
fn init_tracing(verbose: bool, log_service: &dyn LogService) {
    let default = if verbose {
        "debug,wardnetd_mock=debug,wardnet_common=debug,wardnetd_services=debug,wardnetd_data=debug,wardnetd_api=debug"
    } else {
        "info,wardnetd_mock=info"
    };
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default));

    let stderr_layer = tracing_subscriber::fmt::layer()
        .with_writer(std::io::stderr)
        .with_target(true);

    // LogService layers must be applied directly on the Registry, so they
    // come first before filter/formatter layers (mirrors wardnetd main).
    tracing_subscriber::registry()
        .with(log_service.tracing_layers())
        .with(filter)
        .with(stderr_layer)
        .init();

    log_service.start_all();
}

async fn shutdown_signal(restart_token: tokio_util::sync::CancellationToken) {
    let ctrl_c = tokio::signal::ctrl_c();

    #[cfg(unix)]
    {
        let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler");
        tokio::select! {
            _ = ctrl_c => {}
            _ = sigterm.recv() => {}
            () = restart_token.cancelled() => {}
        }
    }

    #[cfg(not(unix))]
    {
        tokio::select! {
            _ = ctrl_c => {}
            () = restart_token.cancelled() => {}
        }
    }

    tracing::info!("mock shutdown signal received");
}
