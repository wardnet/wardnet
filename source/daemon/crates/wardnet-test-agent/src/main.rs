//! `wardnet-test-agent` -- a tool for driving Wardnet end-to-end tests.
//!
//! Two modes:
//!
//! * `server` -- HTTP server that exposes kernel networking state, the daemon
//!   pidfile, container exec, and fixture lookups as a REST API. Runs alongside
//!   `wardnetd` inside the test container.
//! * `client` -- CLI that performs probe operations (routes, interfaces, DNS
//!   resolve, ping, DHCP renew) inside test client containers and prints typed
//!   JSON to stdout for the test runner to consume.

mod client;
mod server;

use clap::{Parser, Subcommand};
use tracing_subscriber::EnvFilter;

#[derive(Debug, Parser)]
#[command(name = "wardnet-test-agent", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Run as an HTTP server (daemon-side test agent).
    Server(server::ServerArgs),
    /// Run a single probe and print JSON to stdout (client-side test agent).
    Client(client::ClientArgs),
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();

    match cli.command {
        Command::Server(args) => server::run(args).await,
        Command::Client(args) => client::run(args).await,
    }
}
