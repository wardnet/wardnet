//! HTTP server mode: exposes kernel state, pidfile checks, container ops,
//! and fixture lookups as a REST API.

pub mod handlers;
pub mod models;

use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::sync::Arc;

use axum::routing::{get, post};
use axum::{Json, Router};
use clap::Args;
use tracing::info;

use models::HealthResponse;

/// Shared state available to all axum handlers.
pub struct AppState {
    /// Directory containing generated fixture files (`WireGuard` configs, keys).
    pub fixtures_dir: PathBuf,
    /// Path to the daemon's pidfile (matches `wardnet-common::config::default_pidfile_path`).
    pub pidfile_path: PathBuf,
}

/// Arguments for the `server` subcommand.
#[derive(Debug, Args)]
pub struct ServerArgs {
    /// Port to listen on.
    #[arg(short, long, default_value_t = 3001)]
    pub port: u16,

    /// Host to bind to.
    #[arg(long, default_value = "0.0.0.0")]
    pub host: IpAddr,

    /// Path to the generated fixtures directory (`WireGuard` configs, keys).
    #[arg(long, default_value = "/tmp/wardnet-system-test/fixtures/generated")]
    pub fixtures_dir: PathBuf,

    /// Path to the daemon's pidfile.
    #[arg(long, default_value = "/run/wardnetd/wardnetd.pid")]
    pub pidfile: PathBuf,
}

/// `GET /health` -- simple liveness check.
async fn health() -> Json<HealthResponse> {
    Json(HealthResponse { status: "ok" })
}

/// Builds the [`Router`] with all endpoint registrations.
fn build_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/pid", get(handlers::pid::get_pid))
        .route("/ip-rules", get(handlers::kernel::get_ip_rules))
        .route("/nft-rules", get(handlers::kernel::get_nft_rules))
        .route("/wg/{interface}", get(handlers::kernel::get_wg_show))
        .route("/link/{interface}", get(handlers::kernel::get_link_show))
        .route(
            "/container/exec",
            post(handlers::container::post_container_exec),
        )
        .route(
            "/process/signal",
            post(handlers::process::post_process_signal),
        )
        .route("/fixtures/{name}", get(handlers::fixtures::get_fixture))
        .route("/postupgrade/state", get(handlers::postupgrade::get_state))
        .route(
            "/power/systemctl-invocations",
            get(handlers::power::get_systemctl_invocations),
        )
        .route("/power/polkit-rule", get(handlers::power::get_polkit_rule))
        .route("/power/pkcheck", get(handlers::power::get_pkcheck))
        // Auto-update binary-swap e2e (issue #319). These observe and
        // manipulate state outside the daemon's API — the live binary,
        // its `.old` sibling, the staging dir — and stay reachable when
        // `wardnetd.service` itself refuses to start.
        .route("/update/binaries", get(handlers::update::get_binaries))
        .route(
            "/update/stage-bad-signature",
            post(handlers::update::post_stage_bad_signature),
        )
        .route(
            "/update/clear-staged",
            post(handlers::update::post_clear_staged),
        )
        .route(
            "/update/restore-original",
            post(handlers::update::post_restore_original),
        )
        .route(
            "/systemd/restart",
            post(handlers::update::post_systemd_restart),
        )
        .route("/systemd/status", get(handlers::update::get_systemd_status))
        .with_state(state)
}

/// Runs the test agent in server mode.
pub async fn run(args: ServerArgs) {
    let addr = SocketAddr::from((args.host, args.port));

    let state = Arc::new(AppState {
        fixtures_dir: args.fixtures_dir,
        pidfile_path: args.pidfile,
    });

    info!(%addr, "starting wardnet-test-agent");

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("failed to bind TCP listener");

    axum::serve(listener, build_router(state))
        .await
        .expect("server error");
}
