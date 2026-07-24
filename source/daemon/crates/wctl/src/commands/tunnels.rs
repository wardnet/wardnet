//! `wctl tunnels list | show | add | remove | test`.

use wardnet_common::api::{
    CreateTunnelRequest, CreateTunnelResponse, DeleteTunnelResponse, ListTunnelsResponse,
    TunnelDetailResponse, TunnelTestResponse,
};
use wardnet_common::tunnel::{Tunnel, TunnelStatus};

use crate::client::Client;
use crate::commands::parse_uuid;
use crate::error::CliError;
use crate::output;

/// `GET /api/tunnels`.
///
/// # Errors
/// Propagates any [`CliError`] from the request.
pub async fn list(client: &Client, json: bool) -> Result<(), CliError> {
    let resp: ListTunnelsResponse = client.get("/api/tunnels").await?;
    if json {
        return output::print_json(&resp);
    }

    if resp.tunnels.is_empty() {
        println!("no tunnels");
        return Ok(());
    }

    println!(
        "{:<36}  {:<16}  {:<7}  {:<12}  {:<12}  RX/TX",
        "ID", "LABEL", "COUNTRY", "INTERFACE", "STATUS"
    );
    for t in &resp.tunnels {
        println!(
            "{:<36}  {:<16}  {:<7}  {:<12}  {:<12}  {} / {}",
            t.id,
            t.label,
            t.country_code,
            t.interface_name,
            status(t.status),
            output::bytes(t.bytes_rx),
            output::bytes(t.bytes_tx),
        );
    }
    Ok(())
}

/// `GET /api/tunnels/{id}`.
///
/// # Errors
/// Propagates any [`CliError`] from the request, including
/// [`CliError::Usage`] when `id` is not a UUID.
pub async fn show(client: &Client, json: bool, id: &str) -> Result<(), CliError> {
    let id = parse_uuid("tunnel", id)?;
    let resp: TunnelDetailResponse = client.get(&format!("/api/tunnels/{id}")).await?;
    if json {
        return output::print_json(&resp);
    }
    print_detail(&resp.tunnel);
    Ok(())
}

/// `POST /api/tunnels`, importing a `WireGuard` `.conf` from `conf_path`.
///
/// # Errors
/// [`CliError::Io`] if the config file cannot be read, or any [`CliError`]
/// from the request.
pub async fn add(
    client: &Client,
    json: bool,
    label: &str,
    country: &str,
    conf_path: &str,
) -> Result<(), CliError> {
    let config = std::fs::read_to_string(conf_path)
        .map_err(|e| CliError::Io(format!("cannot read WireGuard config {conf_path}: {e}")))?;
    let req = CreateTunnelRequest {
        label: label.to_owned(),
        country_code: country.to_owned(),
        provider: None,
        config,
        server_selector: None,
        resolved_server_name: None,
    };
    let resp: CreateTunnelResponse = client.post_json("/api/tunnels", &req).await?;
    if json {
        return output::print_json(&resp);
    }
    println!("{}", resp.message);
    println!(
        "created tunnel {} ({}) on interface {}",
        resp.tunnel.id, resp.tunnel.label, resp.tunnel.interface_name
    );
    Ok(())
}

/// `DELETE /api/tunnels/{id}`.
///
/// # Errors
/// Propagates any [`CliError`] from the request, including
/// [`CliError::Usage`] when `id` is not a UUID.
pub async fn remove(client: &Client, json: bool, id: &str) -> Result<(), CliError> {
    let id = parse_uuid("tunnel", id)?;
    let resp: DeleteTunnelResponse = client.delete(&format!("/api/tunnels/{id}")).await?;
    if json {
        return output::print_json(&resp);
    }
    println!("{}", resp.message);
    Ok(())
}

/// `POST /api/tunnels/{id}/test` — probe exit IP, country, and latency.
///
/// # Errors
/// Propagates any [`CliError`] from the request, including
/// [`CliError::Usage`] when `id` is not a UUID.
pub async fn test(client: &Client, json: bool, id: &str) -> Result<(), CliError> {
    let id = parse_uuid("tunnel", id)?;
    let resp: TunnelTestResponse = client
        .post_empty(&format!("/api/tunnels/{id}/test"))
        .await?;
    if json {
        return output::print_json(&resp);
    }
    let r = &resp.result;
    println!("exit ip:  {}", r.exit_ip);
    println!("country:  {}", r.country_code);
    println!("latency:  {} ms", r.latency_ms);
    Ok(())
}

fn print_detail(t: &Tunnel) {
    println!("id:           {}", t.id);
    println!("label:        {}", t.label);
    println!("country:      {}", t.country_code);
    println!("provider:     {}", output::opt(t.provider.as_deref()));
    println!("interface:    {}", t.interface_name);
    println!("endpoint:     {}", t.endpoint);
    println!("status:       {}", status(t.status));
    println!("last handshake: {}", output::timestamp(t.last_handshake));
    println!(
        "traffic:      {} rx / {} tx",
        output::bytes(t.bytes_rx),
        output::bytes(t.bytes_tx)
    );
    println!("dns override: {}", t.override_default_dns);
    println!("created:      {}", t.created_at.to_rfc3339());
}

fn status(status: TunnelStatus) -> &'static str {
    match status {
        TunnelStatus::Up => "up",
        TunnelStatus::Down => "down",
        TunnelStatus::Connecting => "connecting",
        TunnelStatus::Reconnecting => "reconnecting",
    }
}
