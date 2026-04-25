//! Client mode: performs a single probe operation and prints typed JSON to
//! stdout. Designed for test runners (Vitest) that drive client containers
//! through `docker exec wardnet-test-agent client <subcommand> ...`.
//!
//! All subcommands write JSON to stdout. On error, a `{ "error": "..." }`
//! object is printed and the process exits with code 1. Logs go to stderr.

mod dhcp;
mod dns;
mod interfaces;
mod models;
mod ping;
mod routes;

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
}

/// Runs the requested client subcommand and exits the process.
pub async fn run(args: ClientArgs) -> ! {
    let result: Result<Box<dyn ErasedSerialize>, ClientError> = match args.command {
        ClientCommand::Routes(a) => routes::run(a).await.map(boxed),
        ClientCommand::Interfaces(a) => interfaces::run(a).await.map(boxed),
        ClientCommand::DnsResolve(a) => dns::run(a).await.map(boxed),
        ClientCommand::Ping(a) => ping::run(a).await.map(boxed),
        ClientCommand::DhcpRenew(a) => dhcp::run(a).await.map(boxed),
    };

    let code = emit(result.as_deref(), args.pretty);
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
