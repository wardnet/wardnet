//! `client serve` -- exposes the client probe operations as an HTTP API.
//!
//! Long-running counterpart to the one-shot CLI subcommands. Test
//! containers (`test_debian`, `test_ubuntu`) launch
//! `wardnet-test-agent client serve --port 3001` as their entrypoint;
//! specs reach the probes via plain HTTP (`<container>:3001/...`)
//! instead of going through `docker exec`. Same handlers either way --
//! this module is a thin axum wrapper that constructs `*Args` and
//! forwards to the existing `run()` functions.

use std::net::{IpAddr, SocketAddr};

use axum::Json;
use axum::extract::Query;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Router, serve};
use clap::Args;
use serde::Deserialize;
use tracing::info;

use super::dhcp::{self, DhcpClient, DhcpRenewArgs};
use super::dns::{self, DnsResolveArgs};
use super::interfaces::{self, InterfacesArgs};
use super::models::ClientError;
use super::ping::{self, PingArgs};
use super::routes::{self, RoutesArgs};

/// Arguments for the `client serve` subcommand.
#[derive(Debug, Args)]
pub struct ServeArgs {
    /// Port to listen on.
    #[arg(short, long, default_value_t = 3001)]
    pub port: u16,

    /// Host to bind to.
    #[arg(long, default_value = "0.0.0.0")]
    pub host: IpAddr,
}

pub async fn run(args: ServeArgs) {
    let addr = SocketAddr::from((args.host, args.port));

    let router = Router::new()
        .route("/health", get(health))
        .route("/interfaces", get(get_interfaces))
        .route("/routes", get(get_routes))
        .route("/dns/resolve", get(get_dns_resolve))
        .route("/ping", post(post_ping))
        .route("/dhcp/renew", post(post_dhcp_renew));

    info!(%addr, "starting wardnet-test-agent client serve");

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("failed to bind TCP listener");

    serve(listener, router).await.expect("server error");
}

async fn health() -> &'static str {
    "ok"
}

#[derive(Debug, Deserialize)]
struct InterfacesQuery {
    name: Option<String>,
}

async fn get_interfaces(Query(q): Query<InterfacesQuery>) -> impl IntoResponse {
    into_response(interfaces::run(InterfacesArgs { name: q.name }).await)
}

#[derive(Debug, Deserialize)]
struct RoutesQuery {
    table: Option<String>,
}

async fn get_routes(Query(q): Query<RoutesQuery>) -> impl IntoResponse {
    into_response(routes::run(RoutesArgs { table: q.table }).await)
}

#[derive(Debug, Deserialize)]
struct DnsResolveQuery {
    name: String,
    server: Option<String>,
    record: Option<String>,
    timeout: Option<u32>,
}

async fn get_dns_resolve(Query(q): Query<DnsResolveQuery>) -> impl IntoResponse {
    let args = DnsResolveArgs {
        name: q.name,
        server: q.server,
        record: q.record.unwrap_or_else(|| "A".to_owned()),
        timeout: q.timeout.unwrap_or(3),
    };
    into_response(dns::run(args).await)
}

#[derive(Debug, Deserialize)]
struct PingRequest {
    target: String,
    count: Option<u32>,
    timeout: Option<u32>,
    interface: Option<String>,
}

async fn post_ping(Json(req): Json<PingRequest>) -> impl IntoResponse {
    let args = PingArgs {
        target: req.target,
        count: req.count.unwrap_or(3),
        timeout: req.timeout.unwrap_or(2),
        interface: req.interface,
    };
    into_response(ping::run(args).await)
}

#[derive(Debug, Deserialize)]
struct DhcpRenewRequest {
    interface: String,
    client: Option<DhcpClientName>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum DhcpClientName {
    Dhclient,
    Dhcpcd,
}

impl From<DhcpClientName> for DhcpClient {
    fn from(value: DhcpClientName) -> Self {
        match value {
            DhcpClientName::Dhclient => DhcpClient::Dhclient,
            DhcpClientName::Dhcpcd => DhcpClient::Dhcpcd,
        }
    }
}

async fn post_dhcp_renew(Json(req): Json<DhcpRenewRequest>) -> impl IntoResponse {
    let args = DhcpRenewArgs {
        interface: req.interface,
        client: req.client.map_or(DhcpClient::Dhclient, Into::into),
    };
    into_response(dhcp::run(args).await)
}

/// Maps a probe `Result` to an axum response. Errors surface as 500
/// with the same `{ "error": "..." }` envelope the CLI mode prints, so
/// callers see one shape regardless of transport.
fn into_response<T: serde::Serialize>(
    result: Result<T, ClientError>,
) -> (StatusCode, Json<serde_json::Value>) {
    match result {
        Ok(value) => (
            StatusCode::OK,
            Json(serde_json::to_value(value).unwrap_or(serde_json::Value::Null)),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::to_value(err).unwrap_or(serde_json::Value::Null)),
        ),
    }
}
