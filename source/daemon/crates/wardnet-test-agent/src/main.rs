//! `wardnet-test-agent` -- a lightweight HTTP server that exposes kernel
//! networking state and container operations as a REST API.
//!
//! Designed to run on a Raspberry Pi (or any Linux test host) so that
//! integration / system tests can be driven remotely from a developer machine.

mod container;
mod fixtures;
mod kernel;
mod models;

use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::sync::Arc;

use axum::routing::{get, post};
use axum::{Json, Router};
use clap::Parser;
use models::HealthResponse;
use tracing::info;
use tracing_subscriber::EnvFilter;

/// Shared state available to all axum handlers.
struct AppState {
    /// Directory containing generated fixture files (`WireGuard` configs, keys).
    fixtures_dir: PathBuf,
}

/// A test-only HTTP agent that exposes kernel state and container operations.
#[derive(Debug, Parser)]
#[command(name = "wardnet-test-agent", version, about)]
struct Cli {
    /// Port to listen on.
    #[arg(short, long, default_value_t = 3001)]
    port: u16,

    /// Host to bind to.
    #[arg(long, default_value = "0.0.0.0")]
    host: IpAddr,

    /// Path to the generated fixtures directory (`WireGuard` configs, keys).
    #[arg(long, default_value = "/tmp/wardnet-system-test/fixtures/generated")]
    fixtures_dir: PathBuf,
}

/// `GET /health` -- simple liveness check.
async fn health() -> Json<HealthResponse> {
    Json(HealthResponse { status: "ok" })
}

/// Builds the [`Router`] with all endpoint registrations.
fn build_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/ip-rules", get(kernel::get_ip_rules))
        .route("/nft-rules", get(kernel::get_nft_rules))
        .route("/wg/{interface}", get(kernel::get_wg_show))
        .route("/link/{interface}", get(kernel::get_link_show))
        .route("/container/exec", post(container::post_container_exec))
        .route("/fixtures/{name}", get(fixtures::get_fixture))
        .with_state(state)
}

#[tokio::main]
async fn main() {
    // Initialize tracing (respects RUST_LOG, defaults to info).
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();
    let addr = SocketAddr::from((cli.host, cli.port));

    let state = Arc::new(AppState {
        fixtures_dir: cli.fixtures_dir,
    });

    info!(%addr, "starting wardnet-test-agent");

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("failed to bind TCP listener");

    axum::serve(listener, build_router(state))
        .await
        .expect("server error");
}
