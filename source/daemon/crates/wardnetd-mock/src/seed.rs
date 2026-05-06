//! Demo data seeding for the mock server.
//!
//! Populates realistic but entirely fake data via repositories so the web UI
//! has something to display without requiring a real Pi deployment:
//! devices (laptop, phone, TV, tablet, `IoT`), two `WireGuard` tunnels, a
//! disabled DNS blocklist with a few custom rules, and a single routing rule.
//!
//! Admin credentials are **not** seeded — the setup wizard runs on every
//! mock launch so developers can exercise that flow repeatedly.

use chrono::{Datelike, Duration, Timelike, Utc};
use uuid::Uuid;
use wardnetd_data::RepositoryFactory;
use wardnetd_data::repository::{
    AllowlistRow, CustomRuleRow, DailyMetricRow, DeviceRow, DhcpLeaseRow, DhcpReservationRow,
    IntradayMetricRow, QueryLogRow, TunnelRow,
};

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
    let tunnel_metrics_repo = factory.tunnel_metrics();
    let dns_repo = factory.dns();
    let dns_filter_repo = factory.dns_filter();
    // Hardcoded id of the migration-seeded "Ad Blocking" builtin profile.
    let ad_blocking_profile_id = "00000000-0000-0000-0000-000000000100".to_owned();
    let dhcp_repo = factory.dhcp();

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
    let mut device_lease_inputs = Vec::with_capacity(devices.len());
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
        device_lease_inputs.push((
            id,
            mac.to_owned(),
            hostname.map(str::to_owned),
            ip.to_owned(),
        ));
        tracing::debug!(
            device_id = %id,
            mac,
            ip,
            "seeded device: device_id={id}, mac={mac}, ip={ip}",
        );
    }

    // ------------------------------------------------------------------
    // DHCP leases — one active lease per seeded device so the Leases tab
    // and the dashboard "Active leases" stat have something to render.
    // The smart plug additionally gets a reservation so the Reservations
    // tab is also non-empty.
    // ------------------------------------------------------------------
    let lease_start = (now - Duration::hours(6)).to_rfc3339();
    let lease_end = (now + Duration::hours(18)).to_rfc3339();
    for (device_id, mac, hostname, ip) in &device_lease_inputs {
        let lease_id = Uuid::new_v4();
        let lease = DhcpLeaseRow {
            id: lease_id.to_string(),
            mac_address: mac.clone(),
            ip_address: ip.clone(),
            hostname: hostname.clone(),
            lease_start: lease_start.clone(),
            lease_end: lease_end.clone(),
            status: "active".to_owned(),
            device_id: Some(device_id.to_string()),
        };
        dhcp_repo.insert_lease(&lease).await?;
    }

    if let Some((_, mac, hostname, ip)) = device_lease_inputs.last() {
        let reservation = DhcpReservationRow {
            id: Uuid::new_v4().to_string(),
            mac_address: mac.clone(),
            ip_address: ip.clone(),
            hostname: hostname.clone(),
            description: Some("Smart plug — kitchen".to_owned()),
        };
        dhcp_repo.insert_reservation(&reservation).await?;
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
            "up",
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
    let mut up_tunnel_id: Option<Uuid> = None;
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
        if status == "up" {
            up_tunnel_id = Some(id);
        }
        tunnel_ids.push(id);
        tracing::debug!(
            tunnel_id = %id,
            label,
            interface,
            "seeded tunnel: tunnel_id={id}, label={label}, interface={interface}",
        );
    }

    // ------------------------------------------------------------------
    // Tunnel metrics history — only for the seeded "up" tunnel so the
    // detail page chart looks alive in `make run-dev`. The "down"
    // tunnel is left empty to validate the empty-state UI.
    // ------------------------------------------------------------------
    if let Some(tunnel_id) = up_tunnel_id {
        let (intraday, daily) = generate_metrics_history(tunnel_id, now);
        tunnel_metrics_repo.insert_intraday_batch(&intraday).await?;
        tunnel_metrics_repo.insert_daily_batch(&daily).await?;
        tracing::debug!(
            tunnel_id = %tunnel_id,
            intraday = intraday.len(),
            daily = daily.len(),
            "seeded tunnel metrics history: tunnel_id={tunnel_id}, intraday={i}, daily={d}",
            i = intraday.len(),
            d = daily.len(),
        );
    }

    // ------------------------------------------------------------------
    // Routing rules: route the first two seeded devices through the
    // "up" tunnel so the tunnel detail page's devices table has more
    // than one row to render. The "down" tunnel is left with no
    // devices so its empty-state UI is also exercised.
    // ------------------------------------------------------------------
    if let Some(tunnel_id) = up_tunnel_id {
        let target_json =
            serde_json::json!({ "type": "tunnel", "tunnel_id": tunnel_id.to_string() }).to_string();
        for device_id in device_ids.iter().take(2) {
            device_repo
                .upsert_user_rule(&device_id.to_string(), &target_json, &now_iso)
                .await?;
            tracing::debug!(
                device_id = %device_id,
                tunnel_id = %tunnel_id,
                "seeded routing rule: device_id={device_id}, tunnel_id={tunnel_id}",
            );
        }
    }

    // ------------------------------------------------------------------
    // DNS: one allowlist entry and one custom rule. Two default blocklists
    // are seeded by migrations (both disabled); we leave those alone so no
    // real HTTP fetch is scheduled.
    // ------------------------------------------------------------------
    dns_filter_repo
        .create_allowlist_entry(&AllowlistRow {
            id: Uuid::new_v4().to_string(),
            profile_id: ad_blocking_profile_id.clone(),
            domain: "example.com".to_owned(),
            reason: Some("demo allowlist entry".to_owned()),
        })
        .await?;

    dns_filter_repo
        .create_custom_rule(&CustomRuleRow {
            id: Uuid::new_v4().to_string(),
            profile_id: ad_blocking_profile_id.clone(),
            rule_text: "||tracker.example.net^".to_owned(),
            enabled: true,
            comment: Some("demo custom rule".to_owned()),
        })
        .await?;

    // ------------------------------------------------------------------
    // DNS query log fixture — 24 h of synthetic queries spread across the
    // seeded devices, with a realistic mix of forwarded / cache_hit /
    // blocked / rewritten / upstream_error results. Generated
    // deterministically from `now` so the dev experience is reproducible
    // across `make run-dev` restarts.
    // ------------------------------------------------------------------
    let dns_client_ips: Vec<String> = device_lease_inputs
        .iter()
        .map(|(_, _, _, ip)| ip.clone())
        .collect();
    let log_rows = generate_dns_query_log(&dns_client_ips, now);
    let total_log_rows = log_rows.len();
    for chunk in log_rows.chunks(256) {
        dns_repo.insert_query_log_batch(chunk).await?;
    }

    tracing::info!(
        devices = device_ids.len(),
        tunnels = tunnel_ids.len(),
        dns_queries = total_log_rows,
        "seeded demo data: devices={dev}, tunnels={tun}, dns_queries={dns}",
        dev = device_ids.len(),
        tun = tunnel_ids.len(),
        dns = total_log_rows,
    );

    Ok(SeededIds {
        device_ids,
        tunnel_ids,
    })
}

/// Generate fixture throughput history for a single tunnel:
///
/// - **Intraday**: 48 h × 12 samples/h = 576 rows with a diurnal sine
///   shape (peak in the evening, low overnight) plus deterministic
///   pseudo-jitter so the chart looks alive without flapping between
///   `make run-dev` runs.
/// - **Daily**: 365 rows with weekly variation (weekend spikes), so the
///   `12mo` view is fully populated.
///
/// All math is integer-only and deterministic — the output depends only
/// on `tunnel_id` and `now`, so seeding is reproducible.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_lossless,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]
fn generate_metrics_history(
    tunnel_id: Uuid,
    now: chrono::DateTime<Utc>,
) -> (Vec<IntradayMetricRow>, Vec<DailyMetricRow>) {
    const INTERVAL_SECS: i64 = 300;
    const POINTS_PER_HOUR: usize = 12;
    const HOURS: usize = 48;

    let tid = tunnel_id.to_string();

    // -- Intraday: 48 h backwards from `now`, one row per 5 min. --------
    let mut intraday = Vec::with_capacity(HOURS * POINTS_PER_HOUR);
    let start = now - chrono::Duration::hours(HOURS as i64);
    for i in 0..(HOURS * POINTS_PER_HOUR) {
        let ts_dt = start + chrono::Duration::seconds(INTERVAL_SECS * (i as i64 + 1));
        let ts = ts_dt.timestamp();

        // Diurnal shape: hours-of-day in [0, 24) → sine peaking at 21:00.
        let hour = ts_dt.hour() as f64 + (ts_dt.minute() as f64) / 60.0;
        let phase = (hour - 21.0) / 24.0 * std::f64::consts::TAU;
        let diurnal = (phase.cos() * 0.4 + 0.6).clamp(0.1, 1.0);

        // Cheap deterministic jitter ±15% derived from `i` and `tid`.
        let jitter_seed = (i as u64).wrapping_mul(2_654_435_761) ^ tunnel_id.as_u128() as u64;
        let jitter = ((jitter_seed % 31) as f64 - 15.0) / 100.0;

        // Bytes per interval — pick base values that put bytes/sec in
        // a plausible "consumer streaming" range (a few hundred KB/s).
        let base_tx = 30_000_000.0_f64; // ~100 KB/s averaged
        let base_rx = 90_000_000.0_f64; // ~300 KB/s averaged
        let tx_delta = (base_tx * diurnal * (1.0 + jitter)).max(0.0) as i64;
        let rx_delta = (base_rx * diurnal * (1.0 + jitter)).max(0.0) as i64;

        intraday.push(IntradayMetricRow {
            tunnel_id: tid.clone(),
            ts,
            bytes_tx_delta: tx_delta,
            bytes_rx_delta: rx_delta,
        });
    }

    // -- Daily: 365 rows with weekly variation. -------------------------
    let mut daily = Vec::with_capacity(365);
    for d in 1..=365 {
        let day_dt = now - chrono::Duration::days(d);
        let day = day_dt.format("%Y-%m-%d").to_string();
        // Weekday vs weekend (chrono: Sat=5, Sun=6 in num_days_from_monday).
        let dow = day_dt.weekday().num_days_from_monday();
        let weekend_boost = if dow >= 5 { 1.6 } else { 1.0 };
        let jitter_seed = (d as u64).wrapping_mul(2_654_435_761) ^ tunnel_id.as_u128() as u64;
        let jitter = ((jitter_seed % 21) as f64 - 10.0) / 100.0;

        // ~2 GB tx, ~6 GB rx on a typical weekday.
        let base_tx = 2_000_000_000.0_f64;
        let base_rx = 6_000_000_000.0_f64;
        let tx_total = (base_tx * weekend_boost * (1.0 + jitter)).max(0.0) as i64;
        let rx_total = (base_rx * weekend_boost * (1.0 + jitter)).max(0.0) as i64;

        daily.push(DailyMetricRow {
            tunnel_id: tid.clone(),
            day,
            bytes_tx_total: tx_total,
            bytes_rx_total: rx_total,
        });
    }

    (intraday, daily)
}

// ── DNS query log fixture ─────────────────────────────────────────────────

/// Build a 24-hour synthetic query log spread across the seeded device IPs.
/// Returns rows in chronological order (oldest first) so paginated UI views
/// show recent queries first.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]
fn generate_dns_query_log(client_ips: &[String], now: chrono::DateTime<Utc>) -> Vec<QueryLogRow> {
    if client_ips.is_empty() {
        return Vec::new();
    }

    // Pool of synthetic domains. Mix of "popular", "ad/tracker" (will be
    // marked blocked), CDN, and the seeded allowlist/custom-rule entries.
    let popular = [
        "github.com",
        "youtube.com",
        "wikipedia.org",
        "duckduckgo.com",
        "news.ycombinator.com",
        "reddit.com",
    ];
    let ad_blocked = [
        "doubleclick.net",
        "googletagmanager.com",
        "adservice.google.com",
        "ads.facebook.com",
        "tracker.example.net",
    ];
    let cdn = [
        "fonts.googleapis.com",
        "cdn.cloudflare.com",
        "edge-mqtt.facebook.com",
        "akamaihd.net",
    ];
    let upstreams = ["1.1.1.1", "8.8.8.8"];

    // 24 h × ~1 query / 30 s = ~2 880 rows across all clients. Plenty of
    // density for the chart without bloating the seed.
    let total_minutes = 24 * 60;
    let queries_per_minute: u32 = 2;
    let mut rows = Vec::with_capacity(total_minutes as usize * queries_per_minute as usize);

    for minute_offset in 0..total_minutes {
        for q in 0..queries_per_minute {
            // Deterministic pseudo-random index — no rng dependency.
            let seed = (minute_offset as u64).wrapping_mul(2_654_435_761) ^ u64::from(q);
            let client = &client_ips[(seed as usize) % client_ips.len()];
            let bucket_pick = (seed >> 7) % 10;

            let (domain, result) = if bucket_pick < 2 {
                // 20 % blocked
                (
                    ad_blocked[(seed >> 11) as usize % ad_blocked.len()].to_owned(),
                    "blocked",
                )
            } else if bucket_pick < 5 {
                // 30 % cache hit
                (
                    popular[(seed >> 13) as usize % popular.len()].to_owned(),
                    "cache_hit",
                )
            } else if bucket_pick == 9 {
                // 10 % CDN forward (simulates a fresh upstream lookup)
                (
                    cdn[(seed >> 17) as usize % cdn.len()].to_owned(),
                    "forwarded",
                )
            } else {
                // remainder — forwarded popular domains
                (
                    popular[(seed >> 19) as usize % popular.len()].to_owned(),
                    "forwarded",
                )
            };

            // Place oldest at the start of the loop, newest at the end.
            let ts = now - Duration::minutes(i64::from(total_minutes - minute_offset));
            let latency_ms = match result {
                "cache_hit" => 0.4 + ((seed >> 23) as f64 % 5.0) / 10.0,
                "blocked" => 0.2 + ((seed >> 23) as f64 % 3.0) / 10.0,
                _ => 12.0 + ((seed >> 23) as f64 % 50.0),
            };
            let upstream = if result == "forwarded" {
                Some(upstreams[(seed >> 29) as usize % upstreams.len()].to_owned())
            } else {
                None
            };

            rows.push(QueryLogRow {
                timestamp: ts.format("%Y-%m-%dT%H:%M:%SZ").to_string(),
                client_ip: client.clone(),
                domain,
                query_type: "A".to_owned(),
                result: result.to_owned(),
                upstream,
                latency_ms,
                device_id: None,
            });
        }
    }

    rows
}
