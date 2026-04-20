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
use std::time::Instant;

use clap::Parser;
use tokio::net::TcpListener;
use tracing::Instrument;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use wardnet_common::config::ApplicationConfiguration;
use wardnetd_api::state::AppState;
use wardnetd_data::create_repository_factory;
use wardnetd_mock::backends::noop_device::{NoopHostnameResolver, NoopPacketCapture};
use wardnetd_mock::backends::noop_dhcp::NoopDhcpServer;
use wardnetd_mock::backends::noop_dns::NoopDnsServer;
use wardnetd_mock::backends::noop_keys::InMemoryKeyStore;
use wardnetd_mock::backends::noop_routing::{NoopFirewallManager, NoopPolicyRouter};
use wardnetd_mock::backends::noop_tunnel::NoopTunnelInterface;
use wardnetd_mock::events::FakeEventEmitter;
use wardnetd_mock::seed;
use wardnetd_services::dns::blocklist_downloader::HttpBlocklistFetcher;
use wardnetd_services::logging::{
    ErrorNotifierService, LogService, LogServiceImpl, LogStreamService,
};
use wardnetd_services::{Backends, init_services_with_factory};

/// Wardnet mock daemon — local HTTP API for web-ui development.
#[derive(Parser, Debug)]
#[command(
    name = "wardnetd-mock",
    about = "Runs the Wardnet API on loopback with no-op network backends and seeded demo data."
)]
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
    let log_stream = Arc::new(LogStreamService::new(config.logging.broadcast_capacity));
    let error_notifier = Arc::new(ErrorNotifierService::new(config.logging.max_recent_errors));
    let log_service: Arc<dyn LogService> = Arc::new(LogServiceImpl::new(
        log_stream,
        error_notifier,
        config.logging.path.clone(),
    ));

    init_tracing(cli.verbose, log_service.as_ref());

    run(cli, config, log_service).await
}

async fn run(
    cli: Cli,
    config: ApplicationConfiguration,
    log_service: Arc<dyn LogService>,
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

    // No-op backends are used only for subsystems that require Linux kernel
    // APIs unavailable on macOS (WireGuard, netlink routing, nftables, pnet
    // capture). Anything that works cross-platform — like HTTP blocklist
    // fetches — uses the real implementation so dev testing exercises the
    // actual code path.
    let backends = Backends {
        tunnel_interface: Arc::new(NoopTunnelInterface),
        policy_router: Arc::new(NoopPolicyRouter),
        firewall: Arc::new(NoopFirewallManager),
        packet_capture: Arc::new(NoopPacketCapture),
        hostname_resolver: Arc::new(NoopHostnameResolver),
        key_store: Arc::new(InMemoryKeyStore::default()),
        blocklist_fetcher: Arc::new(HttpBlocklistFetcher::new()),
    };

    // A synthetic LAN IP that looks plausible in UI copy.
    let lan_ip = std::net::Ipv4Addr::new(192, 168, 1, 1);

    let services = init_services_with_factory(
        factory.as_ref(),
        backends,
        &config,
        lan_ip,
        started_at,
        log_service.clone(),
    );

    // No-op DHCP and DNS servers — services and handlers treat them
    // opaquely so the UI gets consistent start/stop semantics.
    let dhcp_server: Arc<dyn wardnetd_services::dhcp::server::DhcpServer> =
        Arc::new(NoopDhcpServer::default());
    let dns_server: Arc<dyn wardnetd_services::dns::server::DnsServer> =
        Arc::new(NoopDnsServer::default());

    let state = AppState::new(
        services.auth.clone(),
        services.device.clone(),
        services.dhcp.clone(),
        services.dns.clone(),
        services.discovery.clone(),
        log_service.clone(),
        services.vpn_provider.clone(),
        services.routing.clone(),
        services.system.clone(),
        services.tunnel.clone(),
        dhcp_server,
        dns_server,
        services.event_publisher.clone(),
        services.jobs.clone(),
    );

    // Start the fake event emitter (unless disabled).
    let emitter = if cli.no_events {
        tracing::info!("--no-events set, skipping fake event emitter");
        None
    } else {
        Some(FakeEventEmitter::start(
            services.event_publisher.clone(),
            tunnel_ids_for_events,
        ))
    };

    let app = wardnetd_api::api::router(state);
    let addr: SocketAddr = format!("{}:{}", config.server.host, config.server.port).parse()?;

    println!(
        "\n  wardnetd-mock\n  Listening on http://{}\n  Database: {}\n  (Setup wizard runs on every launch — no admin is seeded.)\n",
        addr, config.database.connection_string,
    );

    let listener = TcpListener::bind(addr).await?;
    let api_span = tracing::info_span!("mock_api_server");
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .into_future()
    .instrument(api_span)
    .await?;

    tracing::info!("mock server stopped, shutting down emitter");
    if let Some(emitter) = emitter {
        emitter.shutdown().await;
    }

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

async fn shutdown_signal() {
    let ctrl_c = tokio::signal::ctrl_c();

    #[cfg(unix)]
    {
        let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler");
        tokio::select! {
            _ = ctrl_c => {}
            _ = sigterm.recv() => {}
        }
    }

    #[cfg(not(unix))]
    {
        ctrl_c.await.ok();
    }

    tracing::info!("mock shutdown signal received");
}
