//! Client mode: probe operations the test runner drives against
//! containers attached to the daemon's simulated LAN. Two transports
//! over the same handlers:
//!
//! * One-shot CLI subcommands (`routes`, `interfaces`, `dns-resolve`,
//!   `ping`, `dhcp-renew`) print typed JSON to stdout and exit. Useful
//!   for ad-hoc probes via `docker exec` and as a debugging entrypoint.
//! * `serve` keeps the same handlers behind a long-running HTTP server.
//!   Test client containers run `client serve --port 3001` as their
//!   entrypoint so specs can `fetch("http://test_debian:3001/...")`
//!   without mounting the container-runtime socket into the runner.

mod dhcp;
mod dns;
mod interfaces;
mod models;
mod ping;
mod routes;
mod serve;

use clap::{Args, Subcommand};
use serde::Serialize;
use tracing::warn;

use models::ClientError;

#[derive(Debug, Args)]
pub struct ClientArgs {
    #[command(subcommand)]
    command: ClientCommand,

    /// Pretty-print JSON output (default: compact).
    #[arg(long, global = true)]
    pretty: bool,
}

#[derive(Debug, Subcommand)]
enum ClientCommand {
    /// Print the kernel routing table as JSON.
    Routes(routes::RoutesArgs),
    /// Print interfaces and their addresses as JSON.
    Interfaces(interfaces::InterfacesArgs),
    /// Resolve a DNS name and print results.
    DnsResolve(dns::DnsResolveArgs),
    /// Send ICMP echo and report success / RTT.
    Ping(ping::PingArgs),
    /// Renew the DHCP lease on an interface.
    DhcpRenew(dhcp::DhcpRenewArgs),
    /// Run the probe handlers behind an HTTP server (long-running).
    Serve(serve::ServeArgs),
}

/// Runs the requested client subcommand. One-shot subcommands print
/// JSON to stdout and exit; `serve` blocks indefinitely on the HTTP
/// listener and returns only on a fatal error.
pub async fn run(args: ClientArgs) -> ! {
    let pretty = args.pretty;
    let result: Result<Box<dyn ErasedSerialize>, ClientError> = match args.command {
        ClientCommand::Routes(a) => routes::run(a).await.map(boxed),
        ClientCommand::Interfaces(a) => interfaces::run(a).await.map(boxed),
        ClientCommand::DnsResolve(a) => dns::run(a).await.map(boxed),
        ClientCommand::Ping(a) => ping::run(a).await.map(boxed),
        ClientCommand::DhcpRenew(a) => dhcp::run(a).await.map(boxed),
        ClientCommand::Serve(a) => {
            serve::run(a).await;
            std::process::exit(0);
        }
    };

    let code = emit(result.as_deref(), pretty);
    std::process::exit(code);
}

/// Erased trait so different subcommand response types share an exit path.
trait ErasedSerialize {
    fn to_json(&self, pretty: bool) -> Result<String, serde_json::Error>;
}

impl<T: Serialize> ErasedSerialize for T {
    fn to_json(&self, pretty: bool) -> Result<String, serde_json::Error> {
        if pretty {
            serde_json::to_string_pretty(self)
        } else {
            serde_json::to_string(self)
        }
    }
}

fn boxed<T: Serialize + 'static>(value: T) -> Box<dyn ErasedSerialize> {
    Box::new(value)
}

fn emit(result: Result<&dyn ErasedSerialize, &ClientError>, pretty: bool) -> i32 {
    match result {
        Ok(value) => match value.to_json(pretty) {
            Ok(s) => {
                println!("{s}");
                0
            }
            Err(e) => {
                warn!(error = %e, "failed to serialize response");
                println!("{{\"error\":\"failed to serialize response: {e}\"}}");
                1
            }
        },
        Err(err) => {
            let json = err.to_json(pretty).unwrap_or_else(|_| {
                String::from("{\"error\":\"failed to serialize error response\"}")
            });
            println!("{json}");
            1
        }
    }
}
