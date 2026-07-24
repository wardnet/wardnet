//! `wctl status` — one-shot system health snapshot.

use wardnet_common::api::{LastShutdownState, SystemStatusResponse};

use crate::client::Client;
use crate::error::CliError;
use crate::output;

/// Fetch and render `GET /api/system/status`.
///
/// # Errors
/// Propagates any [`CliError`] from the request.
pub async fn run(client: &Client, json: bool) -> Result<(), CliError> {
    let status: SystemStatusResponse = client.get("/api/system/status").await?;
    if json {
        return output::print_json(&status);
    }

    println!("version:       {}", status.release_version);
    println!("build:         {}", status.version);
    println!("uptime:        {}", output::duration(status.uptime_seconds));
    println!("devices:       {}", status.device_count);
    println!(
        "tunnels:       {} ({} active)",
        status.tunnel_count, status.tunnel_active_count
    );
    println!("cpu:           {:.1}%", status.cpu_usage_percent);
    println!(
        "memory:        {} / {}",
        output::bytes(status.memory_used_bytes),
        output::bytes(status.memory_total_bytes)
    );
    println!(
        "disk free:     {} / {}",
        output::bytes(status.disk_free_bytes),
        output::bytes(status.disk_total_bytes)
    );
    println!("database:      {}", output::bytes(status.db_size_bytes));
    println!(
        "last shutdown: {} ({})",
        shutdown_state(status.last_shutdown.state),
        output::timestamp(status.last_shutdown.at)
    );
    Ok(())
}

fn shutdown_state(state: LastShutdownState) -> &'static str {
    match state {
        LastShutdownState::Unknown => "unknown",
        LastShutdownState::Graceful => "graceful",
        LastShutdownState::Unclean => "unclean",
    }
}
