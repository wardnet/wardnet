//! Demo data seeding for the mock server.
//!
//! Populates realistic but entirely fake data via repositories so the web UI
//! has something to display without requiring a real Pi deployment:
//! devices (laptop, phone, TV, tablet, `IoT`), two `WireGuard` tunnels, a
//! disabled DNS blocklist with a few custom rules, and a single routing rule.
//!
//! Admin credentials are **not** seeded — the setup wizard runs on every
//! mock launch so developers can exercise that flow repeatedly.

use chrono::{Duration, Utc};
use uuid::Uuid;
use wardnetd_data::RepositoryFactory;
use wardnetd_data::repository::{AllowlistRow, CustomRuleRow, DeviceRow, TunnelRow};

/// IDs of the entities inserted by [`populate`], so the event emitter can
/// refer to them.
#[derive(Debug, Clone, Default)]
pub struct SeededIds {
    pub device_ids: Vec<Uuid>,
    pub tunnel_ids: Vec<Uuid>,
}

/// Populate the given repository factory with demo data.
///
/// Safe to call on a freshly-initialized (empty) database only — does not
/// deduplicate against existing rows.
#[allow(clippy::too_many_lines)]
pub async fn populate(factory: &dyn RepositoryFactory) -> anyhow::Result<SeededIds> {
    let device_repo = factory.device();
    let tunnel_repo = factory.tunnel();
    let dns_repo = factory.dns();

    let now = Utc::now();
    let now_iso = now.to_rfc3339();

    // ------------------------------------------------------------------
    // Devices
    // ------------------------------------------------------------------
    let devices = [
        (
            "AA:BB:CC:11:22:01",
            Some("alice-laptop"),
            Some("Apple Inc."),
            "laptop",
            "192.168.1.23",
            Duration::minutes(2),
        ),
        (
            "AA:BB:CC:11:22:02",
            Some("alice-phone"),
            Some("Samsung Electronics"),
            "phone",
            "192.168.1.42",
            Duration::seconds(30),
        ),
        (
            "AA:BB:CC:11:22:03",
            Some("living-room-tv"),
            Some("LG Electronics"),
            "tv",
            "192.168.1.55",
            Duration::minutes(10),
        ),
        (
            "AA:BB:CC:11:22:04",
            Some("kids-tablet"),
            Some("Amazon Technologies"),
            "tablet",
            "192.168.1.67",
            Duration::hours(4),
        ),
        (
            "AA:BB:CC:11:22:05",
            Some("smart-plug-kitchen"),
            Some("TP-Link"),
            "iot",
            "192.168.1.78",
            Duration::minutes(1),
        ),
    ];

    let mut device_ids = Vec::with_capacity(devices.len());
    for (mac, hostname, manufacturer, device_type, ip, last_seen_ago) in devices {
        let id = Uuid::new_v4();
        let first_seen = (now - Duration::days(7)).to_rfc3339();
        let last_seen = (now - last_seen_ago).to_rfc3339();

        let row = DeviceRow {
            id: id.to_string(),
            mac: mac.to_owned(),
            hostname: hostname.map(str::to_owned),
            manufacturer: manufacturer.map(str::to_owned),
            device_type: device_type.to_owned(),
            first_seen,
            last_seen,
            last_ip: ip.to_owned(),
        };
        device_repo.insert(&row).await?;
        device_ids.push(id);
        tracing::debug!(
            device_id = %id,
            mac,
            ip,
            "seeded device: device_id={id}, mac={mac}, ip={ip}",
        );
    }

    // ------------------------------------------------------------------
    // Tunnels
    // ------------------------------------------------------------------
    let tunnels = [
        (
            "NordVPN US-1234",
            "US",
            Some("nordvpn"),
            "wg_ward0",
            "us1234.nordvpn.com:51820",
            "down",
            // realistic-looking fake public key
            "wFVuJ3gx+w9kZl1/KxCZYqU9QOHkP3nCqjXmU8ZIxRI=",
            "10.5.0.2/32",
            "1.1.1.1",
        ),
        (
            "ProtonVPN Netherlands-7",
            "NL",
            Some("protonvpn"),
            "wg_ward1",
            "nl-07.protonvpn.net:51820",
            "down",
            "M1oeUgbpZ2aLh8QH0nC5jpUeE7xG9m+YIyHj2lX8v0Q=",
            "10.2.0.5/32",
            "10.2.0.1",
        ),
    ];

    let mut tunnel_ids = Vec::with_capacity(tunnels.len());
    for (label, country, provider, interface, endpoint, status, peer_pk, address_cidr, dns_ip) in
        tunnels
    {
        let id = Uuid::new_v4();
        let address_json = serde_json::to_string(&[address_cidr])?;
        let dns_json = serde_json::to_string(&[dns_ip])?;
        let peer_json = serde_json::json!({
            "public_key": peer_pk,
            "endpoint": endpoint,
            "allowed_ips": ["0.0.0.0/0"],
            "preshared_key": null,
            "persistent_keepalive": 25u16,
        })
        .to_string();

        let row = TunnelRow {
            id: id.to_string(),
            label: label.to_owned(),
            country_code: country.to_owned(),
            provider: provider.map(str::to_owned),
            interface_name: interface.to_owned(),
            endpoint: endpoint.to_owned(),
            status: status.to_owned(),
            address: address_json,
            dns: dns_json,
            peer_config: peer_json,
            listen_port: None,
        };
        tunnel_repo.insert(&row).await?;
        tunnel_ids.push(id);
        tracing::debug!(
            tunnel_id = %id,
            label,
            interface,
            "seeded tunnel: tunnel_id={id}, label={label}, interface={interface}",
        );
    }

    // ------------------------------------------------------------------
    // Routing rule: first device → first tunnel.
    // ------------------------------------------------------------------
    if let (Some(device_id), Some(tunnel_id)) = (device_ids.first(), tunnel_ids.first()) {
        let target_json =
            serde_json::json!({ "type": "tunnel", "tunnel_id": tunnel_id.to_string() }).to_string();
        device_repo
            .upsert_user_rule(&device_id.to_string(), &target_json, &now_iso)
            .await?;
        tracing::debug!(
            device_id = %device_id,
            tunnel_id = %tunnel_id,
            "seeded routing rule: device_id={device_id}, tunnel_id={tunnel_id}",
        );
    }

    // ------------------------------------------------------------------
    // DNS: one allowlist entry and one custom rule. Two default blocklists
    // are seeded by migrations (both disabled); we leave those alone so no
    // real HTTP fetch is scheduled.
    // ------------------------------------------------------------------
    dns_repo
        .create_allowlist_entry(&AllowlistRow {
            id: Uuid::new_v4().to_string(),
            domain: "example.com".to_owned(),
            reason: Some("demo allowlist entry".to_owned()),
        })
        .await?;

    dns_repo
        .create_custom_rule(&CustomRuleRow {
            id: Uuid::new_v4().to_string(),
            rule_text: "||tracker.example.net^".to_owned(),
            enabled: true,
            comment: Some("demo custom rule".to_owned()),
        })
        .await?;

    tracing::info!(
        devices = device_ids.len(),
        tunnels = tunnel_ids.len(),
        "seeded demo data: devices={dev}, tunnels={tun}",
        dev = device_ids.len(),
        tun = tunnel_ids.len(),
    );

    Ok(SeededIds {
        device_ids,
        tunnel_ids,
    })
}
