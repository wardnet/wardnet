//! `wctl devices list | show | set-rule`.

use wardnet_common::api::{DeviceDetailResponse, DeviceWithStatus, ListDevicesResponse};
use wardnet_common::device::{Device, DeviceType, DhcpStatus};
use wardnet_common::routing::RoutingTarget;

use crate::client::Client;
use crate::commands::{parse_routing_target, parse_uuid};
use crate::error::CliError;
use crate::output;

/// `GET /api/devices`.
///
/// # Errors
/// Propagates any [`CliError`] from the request.
pub async fn list(client: &Client, json: bool) -> Result<(), CliError> {
    let resp: ListDevicesResponse = client.get("/api/devices").await?;
    if json {
        return output::print_json(&resp);
    }

    if resp.devices.is_empty() {
        println!("no devices");
        return Ok(());
    }

    println!(
        "{:<36}  {:<18}  {:<15}  {:<13}  RULE",
        "ID", "NAME", "IP", "TYPE"
    );
    for d in &resp.devices {
        println!(
            "{:<36}  {:<18}  {:<15}  {:<13}  {}",
            d.device.id,
            display_name(&d.device),
            d.device.last_ip,
            device_type(d.device.device_type),
            rule(d.current_rule.as_ref()),
        );
    }
    Ok(())
}

/// `GET /api/devices/{id}`.
///
/// # Errors
/// Propagates any [`CliError`] from the request, including
/// [`CliError::Usage`] when `id` is not a UUID.
pub async fn show(client: &Client, json: bool, id: &str) -> Result<(), CliError> {
    let id = parse_uuid("device", id)?;
    let resp: DeviceDetailResponse = client.get(&format!("/api/devices/{id}")).await?;
    if json {
        return output::print_json(&resp);
    }
    print_detail(&resp.device, resp.current_rule.as_ref());
    Ok(())
}

/// `PUT /api/devices/{id}` setting only the routing target.
///
/// # Errors
/// Propagates any [`CliError`] from the request, including
/// [`CliError::Usage`] for a bad ID or target.
pub async fn set_rule(client: &Client, json: bool, id: &str, target: &str) -> Result<(), CliError> {
    let id = parse_uuid("device", id)?;
    let target = parse_routing_target(target)?;
    // `UpdateDeviceRequest` is deserialize-only on the daemon side; send just
    // the one field we're changing so name/type/lock are left untouched.
    let body = serde_json::json!({ "routing_target": target });
    let resp: DeviceDetailResponse = client
        .put_json(&format!("/api/devices/{id}"), &body)
        .await?;
    if json {
        return output::print_json(&resp);
    }
    println!(
        "device {} routing set to {}",
        resp.device.device.id,
        rule(resp.current_rule.as_ref())
    );
    Ok(())
}

fn print_detail(d: &DeviceWithStatus, current_rule: Option<&RoutingTarget>) {
    println!("id:           {}", d.device.id);
    println!("name:         {}", display_name(&d.device));
    println!(
        "hostname:     {}",
        output::opt(d.device.hostname.as_deref())
    );
    println!("mac:          {}", d.device.mac);
    println!(
        "manufacturer: {}",
        output::opt(d.device.manufacturer.as_deref())
    );
    println!("type:         {}", device_type(d.device.device_type));
    println!("ip:           {}", d.device.last_ip);
    println!("dhcp:         {}", dhcp_status(d.dhcp_status));
    println!("rule:         {}", rule(current_rule));
    println!("admin locked: {}", d.device.admin_locked);
    println!("first seen:   {}", d.device.first_seen.to_rfc3339());
    println!("last seen:    {}", d.device.last_seen.to_rfc3339());
}

fn display_name(device: &Device) -> String {
    device
        .name
        .as_deref()
        .or(device.hostname.as_deref())
        .unwrap_or("-")
        .to_owned()
}

fn rule(rule: Option<&RoutingTarget>) -> String {
    match rule {
        None => "(default)".to_owned(),
        Some(RoutingTarget::Direct) => "direct".to_owned(),
        Some(RoutingTarget::Default) => "default".to_owned(),
        Some(RoutingTarget::Tunnel { tunnel_id }) => format!("tunnel:{tunnel_id}"),
    }
}

fn device_type(t: DeviceType) -> &'static str {
    match t {
        DeviceType::Tv => "tv",
        DeviceType::Phone => "phone",
        DeviceType::Laptop => "laptop",
        DeviceType::Tablet => "tablet",
        DeviceType::GameConsole => "game-console",
        DeviceType::SettopBox => "settop-box",
        DeviceType::Iot => "iot",
        DeviceType::Router => "router",
        DeviceType::ManagedSwitch => "managed-switch",
        DeviceType::Server => "server",
        DeviceType::Unknown => "unknown",
    }
}

fn dhcp_status(status: DhcpStatus) -> &'static str {
    match status {
        DhcpStatus::Lease => "lease",
        DhcpStatus::Reservation => "reservation",
        DhcpStatus::External => "external",
    }
}
