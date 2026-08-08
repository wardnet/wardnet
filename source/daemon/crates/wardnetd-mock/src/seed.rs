//! Demo data seeding for the mock server.
//!
//! Populates realistic but entirely fake data via repositories so the web UI
//! has something to display without requiring a real Pi deployment:
//! devices (laptop, phone, TV, tablet, `IoT`) spread across the Trusted /
//! `IoT` / Guest zones with one cross-zone casting exception, `WireGuard`
//! tunnels, a
//! disabled DNS blocklist with a few custom rules, and a single routing rule.
//!
//! Admin credentials are **not** seeded — the setup wizard runs on every
//! mock launch so developers can exercise that flow repeatedly.

use chrono::{Datelike, Duration, Utc};
use uuid::Uuid;
use wardnet_common::device::{DeviceSignalKind, ManufacturerSource};
use wardnet_common::routing_profile::DomainRoutingTarget;
use wardnet_common::zone_exception::{
    ExceptionEndpoint, ExceptionEndpointKind, ServiceSet, ServiceSpec, ZoneException,
};
use wardnetd_data::RepositoryFactory;
use wardnetd_data::repository::{
    AllowlistRow, CustomRuleRow, DeviceRow, DeviceSignalRow, DhcpLeaseRow, DhcpReservationRow,
    IntradayStatRow, NewNotification, QueryLogRow, RoutingProfileRow, RoutingRuleRow, TunnelRow,
};
use wardnetd_data::{oui, vendor_catalog};

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
    // System-zone UUIDs seeded by the `network_zones` migration.
    const ZONE_TRUSTED: &str = "00000000-0000-0000-0000-000000000201";
    const ZONE_IOT: &str = "00000000-0000-0000-0000-000000000202";
    const ZONE_GUEST: &str = "00000000-0000-0000-0000-000000000203";

    let device_repo = factory.device();
    let tunnel_repo = factory.tunnel();
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
    // Spreading the demo devices across all three system zones (rather than
    // parking everyone in Trusted) gives the Zones page a non-zero member count
    // per zone and sets up a real cross-zone (casting) boundary below.
    // Demo MACs use the A8:BB:CC prefix rather than the more obvious AA:BB:CC
    // because 0xAA has the locally-administered bit set (0xAA & 0x02 != 0).
    // That would flag every seeded device as a randomized/privacy MAC while it
    // also carried an IEEE manufacturer — a contradiction the real world never
    // produces, and one that made the dev UI look broken (issue #1099).
    let devices = [
        (
            "A8:BB:CC:11:22:01",
            Some("alice-laptop"),
            Some("Apple Inc."),
            "laptop",
            "127.0.0.1",
            Duration::minutes(2),
            ZONE_TRUSTED,
        ),
        (
            "A8:BB:CC:11:22:02",
            Some("alice-phone"),
            Some("Samsung Electronics"),
            "phone",
            "192.168.1.42",
            Duration::seconds(30),
            ZONE_TRUSTED,
        ),
        (
            "A8:BB:CC:11:22:03",
            Some("living-room-tv"),
            Some("LG Electronics"),
            "tv",
            "192.168.1.55",
            Duration::minutes(10),
            ZONE_IOT,
        ),
        (
            "A8:BB:CC:11:22:04",
            Some("kids-tablet"),
            Some("Amazon Technologies"),
            "tablet",
            "192.168.1.67",
            Duration::hours(4),
            ZONE_GUEST,
        ),
        (
            "A8:BB:CC:11:22:05",
            Some("smart-plug-kitchen"),
            Some("TP-Link"),
            "iot",
            "192.168.1.78",
            Duration::minutes(1),
            ZONE_IOT,
        ),
        // The "things that can't run a VPN" cohort — the devices the origin
        // story is about. Cameras / doorbell / vacuum sit in IoT; the media
        // boxes are trusted home devices.
        (
            "A8:BB:CC:11:22:06",
            Some("hallway-camera"),
            Some("Reolink"),
            "iot",
            "192.168.1.56",
            Duration::minutes(3),
            ZONE_IOT,
        ),
        (
            "A8:BB:CC:11:22:07",
            Some("smart-doorbell"),
            Some("Ring"),
            "iot",
            "192.168.1.57",
            Duration::seconds(45),
            ZONE_IOT,
        ),
        (
            "A8:BB:CC:11:22:08",
            Some("robot-vacuum"),
            Some("iRobot"),
            "iot",
            "192.168.1.58",
            Duration::hours(2),
            ZONE_IOT,
        ),
        (
            "A8:BB:CC:11:22:09",
            Some("games-console"),
            Some("Sony"),
            "game_console",
            "192.168.1.60",
            Duration::minutes(20),
            ZONE_TRUSTED,
        ),
        (
            "A8:BB:CC:11:22:0A",
            Some("set-top-box"),
            Some("Roku"),
            "settop_box",
            "192.168.1.61",
            Duration::hours(1),
            ZONE_TRUSTED,
        ),
        // --- Device-identification demo cases (issue #1099) ---------------
        // The reported device. 5C-E7-53 is listed to `Private` in the IEEE
        // database, so it has no public manufacturer — only the curated vendor
        // catalog can name it, and then only as a hedge ("Likely Govee").
        //
        // This is seeded with its *Wi-Fi* MAC (…:D9). The Govee app shows the
        // BLE MAC `5c:e7:53:4e:ec:db`, which is +2 away and matches nothing —
        // searching for it is what demonstrates the neighbour heuristic.
        (
            "5C:E7:53:4E:EC:D9",
            Some("govee-lamp"),
            None,
            "iot",
            "192.168.1.62",
            Duration::minutes(5),
            ZONE_IOT,
        ),
        // Locally-administered address: renders "Unknown manufacturer" plus a
        // "Randomized" badge on the MAC, never a manufacturer called
        // "Randomized MAC".
        (
            "02:1A:2B:3C:4D:5E",
            None,
            None,
            "phone",
            "192.168.1.63",
            Duration::minutes(2),
            ZONE_GUEST,
        ),
        // An MA-M/MA-S parent block listed to "IEEE Registration Authority".
        // A 24-bit lookup genuinely cannot resolve the assignee, so this shows
        // the other "Unknown manufacturer" explanation.
        (
            "B8:4C:87:00:11:22",
            None,
            None,
            "unknown",
            "192.168.1.64",
            Duration::minutes(30),
            ZONE_GUEST,
        ),
    ];

    let mut device_ids = Vec::with_capacity(devices.len());
    let mut device_lease_inputs = Vec::with_capacity(devices.len());
    // The device the user PWA resolves `/devices/me` to in local dev — tracked
    // by IP (not insertion order) so it stays correct if the seed list changes.
    let mut localhost_device_id: Option<Uuid> = None;
    // Endpoints of the demo casting exception (phone in Trusted → TV in IoT),
    // captured by hostname so they survive reordering of the seed list.
    let mut casting_from_id: Option<Uuid> = None;
    let mut casting_to_id: Option<Uuid> = None;
    // The privately-listed device from the reported case, so its detail page
    // can show the observations behind its hedged "Likely Govee" name.
    let mut govee_device_id: Option<Uuid> = None;
    for (mac, hostname, manufacturer, device_type, ip, last_seen_ago, zone_id) in devices {
        let id = Uuid::new_v4();
        let first_seen = (now - Duration::days(7)).to_rfc3339();
        let last_seen = (now - last_seen_ago).to_rfc3339();

        let row = DeviceRow {
            id: id.to_string(),
            mac: mac.to_owned(),
            hostname: hostname.map(str::to_owned),
            // Mirror the precedence `insert_new_device` applies for real
            // discoveries (issue #1099), so the dev UI shows the same states a
            // live network produces: an IEEE registrant is stated as fact, a
            // curated catalog match is a hedge, and a privately-listed or
            // randomized address has no manufacturer at all.
            manufacturer: manufacturer
                .map(str::to_owned)
                .or_else(|| vendor_catalog::lookup_oui_override(mac).map(str::to_owned)),
            manufacturer_source: if manufacturer.is_some() {
                Some(ManufacturerSource::Ieee)
            } else {
                vendor_catalog::lookup_oui_override(mac).map(|_| ManufacturerSource::Catalog)
            },
            is_randomized: oui::is_randomized_mac(mac),
            device_type: device_type.to_owned(),
            first_seen,
            last_seen,
            last_ip: ip.to_owned(),
            // Zone assigned per-device above (network_zones migration UUIDs).
            zone_id: zone_id.to_owned(),
            // Demo devices are LAN-discovered.
            connection_mode: wardnet_common::device::DeviceConnectionMode::Lan,
        };
        device_repo.insert(&row).await?;
        device_ids.push(id);
        if ip == "127.0.0.1" {
            localhost_device_id = Some(id);
        }
        match hostname {
            Some("alice-phone") => casting_from_id = Some(id),
            Some("living-room-tv") => casting_to_id = Some(id),
            _ => {}
        }
        if hostname == Some("govee-lamp") {
            govee_device_id = Some(id);
        }
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

    // Enable DNS capture on the localhost device (127.0.0.1) so the user PWA —
    // which resolves `/devices/me` to this device during local dev — receives a
    // live DNS-events stream out of the box. The fake-DNS emitter targets the
    // capture-enabled device, so the two stay in sync via the flag.
    if let Some(localhost_id) = localhost_device_id {
        device_repo
            .update_dns_capture_settings(&localhost_id.to_string(), Some(true), None, None)
            .await?;
        tracing::debug!(device_id = %localhost_id, "enabled DNS capture on localhost device");
    }

    // ------------------------------------------------------------------
    // Identification signals (issue #1099) — seeded for two devices only, so
    // the detail view shows both states it has to handle: the populated case
    // and the far more common "nothing observed yet".
    //
    // The Govee lamp is the interesting one. Its OUI is listed to `Private`,
    // so the catalog is the only thing that can name it; these rows are the
    // evidence behind the "Likely Govee" hedge the identity card shows.
    // ------------------------------------------------------------------
    let identification_repo = factory.device_identification();
    let mut demo_signals: Vec<(Uuid, DeviceSignalKind, &str)> = Vec::new();
    if let Some(govee_id) = govee_device_id {
        demo_signals.extend([
            (govee_id, DeviceSignalKind::MdnsService, "_govee._tcp"),
            (govee_id, DeviceSignalKind::DhcpHostname, "govee-lamp"),
            (
                govee_id,
                DeviceSignalKind::DhcpParamList,
                "1,3,6,15,28,51,58,59",
            ),
        ]);
    }
    if let Some(tv_id) = casting_to_id {
        demo_signals.extend([
            (tv_id, DeviceSignalKind::MdnsService, "_googlecast._tcp"),
            (tv_id, DeviceSignalKind::MdnsService, "_airplay._tcp"),
        ]);
    }
    for (device_id, kind, value) in demo_signals {
        identification_repo
            .record(&DeviceSignalRow {
                device_id: device_id.to_string(),
                kind,
                value: value.to_owned(),
                // Ask the catalog the same question the recording service asks,
                // rather than restating the rule here — a second copy would
                // drift the moment a new signal kind carries vendor
                // information.
                inferred: vendor_catalog::lookup_signal(kind, value).is_some(),
            })
            .await?;
    }

    // ------------------------------------------------------------------
    // Cross-zone (casting) exception — the phone (Trusted) is allowed to reach
    // the living-room TV (IoT) over the curated casting port set, so the Zones
    // page's exceptions card renders a real entry instead of an empty state.
    // ------------------------------------------------------------------
    if let (Some(from_id), Some(to_id)) = (casting_from_id, casting_to_id) {
        let exception = ZoneException {
            id: Uuid::new_v4(),
            from: ExceptionEndpoint {
                kind: ExceptionEndpointKind::Device,
                id: from_id,
            },
            to: ExceptionEndpoint {
                kind: ExceptionEndpointKind::Device,
                id: to_id,
            },
            service: ServiceSpec::Preset {
                set: ServiceSet::Casting,
            },
            // Casting needs discovery/streaming traffic in both directions.
            bidirectional: true,
            created_at: now,
            updated_at: now,
        };
        factory.zone_exception().insert(&exception).await?;
        tracing::debug!(
            from = %from_id,
            to = %to_id,
            "seeded casting exception: phone → living-room TV",
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

    if let Some((_, mac, hostname, ip)) = device_lease_inputs
        .iter()
        .find(|(_, _, hostname, _)| hostname.as_deref() == Some("smart-plug-kitchen"))
    {
        let reservation = DhcpReservationRow {
            id: Uuid::new_v4().to_string(),
            mac_address: mac.clone(),
            ip_address: ip.clone(),
            hostname: hostname.clone(),
            description: Some("Smart plug - kitchen".to_owned()),
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
        // Custom-config tunnel — no provider, no country. Surfaces the
        // "custom configuration" fallback icon in the TunnelCard head
        // (no flag, generic sliders glyph in the provider chip).
        (
            "Home lab",
            "",
            None,
            "wg_ward2",
            "lab.example.internal:51820",
            "up",
            "Jq8oCzXwM5b3GZpYf4uTzr6IhDvKnAQsNzL2Mo5xPwY=",
            "10.9.0.2/32",
            "10.9.0.1",
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
            override_default_dns: true,
            server_selector_country: None,
            resolved_server_name: None,
            endpoint_resolved_at: None,
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
    // Domain routing profiles (issue #241): a "Streaming (UK)" profile routing
    // streaming domains through the "up" tunnel, and a "Work" profile mixing a
    // tunnel rule with a direct carve-out. Assigned to alice-laptop (the user
    // PWA's `me` device) and living-room-tv, so the admin Routing pages, the
    // device routing-profiles card, the profile "Used by" list, and the user
    // PWA all have data. Falls back to Direct when no "up" tunnel came up.
    // ------------------------------------------------------------------
    let routing_profile_repo = factory.routing_profile();
    let tunnel_or_direct = match up_tunnel_id {
        Some(t) => DomainRoutingTarget::Tunnel { tunnel_id: t },
        None => DomainRoutingTarget::Direct,
    };

    let streaming_profile = routing_profile_repo
        .create_profile(&RoutingProfileRow {
            id: Uuid::new_v4().to_string(),
            name: "Streaming (UK)".to_owned(),
        })
        .await?;
    for pattern in ["*.netflix.com", "*.bbc.co.uk"] {
        routing_profile_repo
            .create_rule(&RoutingRuleRow {
                id: Uuid::new_v4().to_string(),
                profile_id: streaming_profile.id,
                pattern: pattern.to_owned(),
                target: tunnel_or_direct.clone(),
                enabled: true,
            })
            .await?;
    }

    let work_profile = routing_profile_repo
        .create_profile(&RoutingProfileRow {
            id: Uuid::new_v4().to_string(),
            name: "Work".to_owned(),
        })
        .await?;
    routing_profile_repo
        .create_rule(&RoutingRuleRow {
            id: Uuid::new_v4().to_string(),
            profile_id: work_profile.id,
            pattern: "*.slack.com".to_owned(),
            target: tunnel_or_direct.clone(),
            enabled: true,
        })
        .await?;
    routing_profile_repo
        .create_rule(&RoutingRuleRow {
            id: Uuid::new_v4().to_string(),
            profile_id: work_profile.id,
            pattern: "intranet.example.com".to_owned(),
            target: DomainRoutingTarget::Direct,
            enabled: true,
        })
        .await?;

    // alice-laptop (user PWA `me`) gets Streaming; living-room-tv gets both in
    // priority order so multi-profile display + reordering have data.
    if let Some(alice) = localhost_device_id {
        routing_profile_repo
            .set_device_profiles(alice, &[streaming_profile.id])
            .await?;
    }
    if let Some(tv) = casting_to_id {
        routing_profile_repo
            .set_device_profiles(tv, &[streaming_profile.id, work_profile.id])
            .await?;
    }
    tracing::debug!("seeded routing profiles: 'Streaming (UK)' + 'Work'");

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
    // (device_id, ip) pairs: every seeded row carries write-time device
    // attribution, exactly as the real DNS server records it (the
    // `DeviceIpSnapshot` resolves the client IP to a device id when the query
    // is logged). Attributing every client keeps a known device from showing
    // up twice in Top clients — once by device id and once by a bare IP — the
    // failure mode issue #941 flagged, which was a seed artifact rather than a
    // product bug (top-N ranks by device id and only falls back to the raw IP
    // for genuinely unattributed traffic, so it never merges an IP back onto a
    // device — an IP can be reassigned by DHCP).
    let dns_clients: Vec<(Option<String>, String)> = device_lease_inputs
        .iter()
        .map(|(id, _, _, ip)| (Some(id.to_string()), ip.clone()))
        .collect();
    let log_rows = generate_dns_query_log(&dns_clients, now);
    let total_log_rows = log_rows.len();
    for chunk in log_rows.chunks(256) {
        dns_repo.insert_query_log_batch(chunk).await?;
    }

    // ------------------------------------------------------------------
    // DNS stats — 48 h of per-minute intraday rows so the 1h, 24h and
    // partial 7d tabs have data immediately, plus one rollup row per day
    // for the 12m daily chart.
    // ------------------------------------------------------------------
    let stats_repo = factory.stats();

    let intraday_stat_rows = generate_dns_intraday_stats(&dns_clients, now);
    let total_intraday = intraday_stat_rows.len();
    for chunk in intraday_stat_rows.chunks(256) {
        stats_repo.upsert_intraday(chunk).await?;
    }

    let daily_rollup_days = seed_daily_stats(&*stats_repo, &dns_clients, now).await?;

    // ------------------------------------------------------------------
    // Tunnel stats — throughput counters + latency gauge per tunnel.
    // The down / custom tunnels reuse the same generator with their own
    // `tunnel_id` as the RNG seed so each detail-page chart looks
    // distinct. Both the 48 h intraday range and the 12 m daily rollups
    // are seeded so every range tab has data on first launch.
    // ------------------------------------------------------------------
    for tunnel_id in &tunnel_ids {
        let intraday = generate_tunnel_intraday_stats(*tunnel_id, now);
        for chunk in intraday.chunks(256) {
            stats_repo.upsert_intraday(chunk).await?;
        }
        seed_tunnel_daily_stats(&*stats_repo, *tunnel_id, now).await?;
        tracing::debug!(
            tunnel_id = %tunnel_id,
            intraday = intraday.len(),
            "seeded tunnel stats: tunnel_id={tunnel_id}, intraday={i}",
            i = intraday.len(),
        );
    }

    // ------------------------------------------------------------------
    // Admin notification feed (issue #482) — a few entries so the System
    // screen's feed has something to display.
    // ------------------------------------------------------------------
    let notification_repo = factory.notification();
    let feed = [
        (
            "tunnel_offline",
            "Tunnel offline",
            "Stockholm #4 went offline.",
            Some("/tunnels"),
            tunnel_ids.first().copied(),
            Duration::hours(3),
        ),
        (
            "routing_changed",
            "Routing change",
            "alice-laptop changed routing to direct (no tunnel).",
            Some("/devices"),
            device_ids.first().copied(),
            Duration::minutes(45),
        ),
        (
            "new_device_quarantined",
            "New device",
            "New device eve-tablet joined, in Guest. Approve in the app.",
            Some("/devices"),
            device_ids.get(3).copied(),
            Duration::minutes(5),
        ),
    ];
    for (kind, title, body, url, subject_id, age) in feed {
        notification_repo
            .insert(NewNotification {
                id: &Uuid::new_v4().to_string(),
                kind,
                title,
                body,
                url,
                subject_id: subject_id.map(|id| id.to_string()).as_deref(),
                created_at: &(now - age).to_rfc3339(),
            })
            .await?;
    }

    tracing::info!(
        devices = device_ids.len(),
        tunnels = tunnel_ids.len(),
        dns_queries = total_log_rows,
        stat_intraday = total_intraday,
        stat_daily_days = daily_rollup_days,
        "seeded demo data: devices={dev}, tunnels={tun}, dns_queries={dns}, stat_intraday={si}, stat_daily_days={sd}",
        dev = device_ids.len(),
        tun = tunnel_ids.len(),
        dns = total_log_rows,
        si = total_intraday,
        sd = daily_rollup_days,
    );

    Ok(SeededIds {
        device_ids,
        tunnel_ids,
    })
}

/// Generate 48 h of per-minute intraday rows for a single tunnel:
/// `tunnel.bytes.tx` and `tunnel.bytes.rx` counter increments plus
/// `tunnel.latency.rtt_ms` gauge readings, all labelled by `tunnel_id`.
///
/// The counter values follow a diurnal sine shape (peak in the evening,
/// low overnight). Latency hovers in the 25–80 ms range with cheap
/// deterministic jitter and occasional ~150 ms spikes — enough texture
/// for the chart without looking synthetic.
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
fn generate_tunnel_intraday_stats(
    tunnel_id: Uuid,
    now: chrono::DateTime<Utc>,
) -> Vec<IntradayStatRow> {
    const HOURS: i64 = 48;

    let labels = format!(r#"{{"tunnel_id":"{tunnel_id}"}}"#);
    let end_ts = (now.timestamp() / 60) * 60;
    let start_ts = end_ts - HOURS * 3_600;

    let mut rows = Vec::with_capacity(((end_ts - start_ts) / 60) as usize * 3);
    let mut bucket_ts = start_ts;
    while bucket_ts <= end_ts {
        let hour_of_day = ((bucket_ts % 86_400) as f64) / 3_600.0;
        let phase = (hour_of_day - 21.0) / 24.0 * std::f64::consts::TAU;
        let diurnal = (phase.cos() * 0.4 + 0.6).clamp(0.1, 1.0);

        let jitter_seed =
            (bucket_ts as u64).wrapping_mul(2_654_435_761) ^ (tunnel_id.as_u128() as u64);
        let jitter = ((jitter_seed % 31) as f64 - 15.0) / 100.0;

        // Bytes per minute — plausible "consumer streaming" range
        // (~100 KB/s tx, ~300 KB/s rx averaged).
        let tx_bpm = (6_000_000.0 * diurnal * (1.0 + jitter)).max(0.0);
        let rx_bpm = (18_000_000.0 * diurnal * (1.0 + jitter)).max(0.0);

        rows.push(IntradayStatRow {
            metric: "tunnel.bytes.tx".to_owned(),
            labels: labels.clone(),
            bucket_ts,
            value: tx_bpm,
            kind: "counter".to_owned(),
        });
        rows.push(IntradayStatRow {
            metric: "tunnel.bytes.rx".to_owned(),
            labels: labels.clone(),
            bucket_ts,
            value: rx_bpm,
            kind: "counter".to_owned(),
        });

        // Latency: base 25–80 ms with ±5 ms jitter; a 1-in-60 chance of
        // a ~150 ms spike (one every ~hour on average).
        let base_latency = 25.0 + (((jitter_seed >> 5) % 56) as f64);
        let latency_jitter = ((jitter_seed >> 11) % 11) as f64 - 5.0;
        let spike = if (jitter_seed >> 17).is_multiple_of(60) {
            150.0
        } else {
            0.0
        };
        let rtt_ms = (base_latency + latency_jitter + spike).max(5.0);
        rows.push(IntradayStatRow {
            metric: "tunnel.latency.rtt_ms".to_owned(),
            labels: labels.clone(),
            bucket_ts,
            value: rtt_ms,
            kind: "gauge".to_owned(),
        });

        bucket_ts += 60;
    }

    rows
}

/// Seed 13 months of daily rollups for a single tunnel — one row per
/// day per metric. The values mirror the same diurnal/weekday shape as
/// the intraday generator so the 12 m chart matches the 1 h/24 h
/// chart's character. Counter rows roll up to daily totals; the
/// latency gauge rolls up to a daily average.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_lossless,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]
async fn seed_tunnel_daily_stats(
    stats_repo: &dyn wardnetd_data::repository::StatsRepository,
    tunnel_id: Uuid,
    now: chrono::DateTime<Utc>,
) -> anyhow::Result<()> {
    const DAILY_RETENTION_DAYS: i64 = 397;

    let labels = format!(r#"{{"tunnel_id":"{tunnel_id}"}}"#);

    for days_ago in 1..=DAILY_RETENTION_DAYS {
        let day_dt = now - Duration::days(days_ago);
        let day_str = day_dt.format("%Y-%m-%d").to_string();
        let day_ts = day_dt.timestamp();
        let midnight_ts = day_ts - (day_ts % 86_400);
        let noon_ts = midnight_ts + 43_200;

        let dow = day_dt.weekday().num_days_from_monday();
        let weekend_boost = if dow >= 5 { 1.6 } else { 1.0 };
        let jitter_seed =
            (days_ago as u64).wrapping_mul(2_654_435_761) ^ (tunnel_id.as_u128() as u64);
        let jitter = ((jitter_seed % 21) as f64 - 10.0) / 100.0;

        // ~2 GB tx, ~6 GB rx on a typical weekday.
        let tx_total = (2_000_000_000.0 * weekend_boost * (1.0 + jitter)).max(0.0);
        let rx_total = (6_000_000_000.0 * weekend_boost * (1.0 + jitter)).max(0.0);

        // Daily average latency 30–70 ms with deterministic drift.
        let avg_latency = 30.0 + ((jitter_seed >> 7) % 41) as f64;

        let day_rows = vec![
            IntradayStatRow {
                metric: "tunnel.bytes.tx".to_owned(),
                labels: labels.clone(),
                bucket_ts: noon_ts,
                value: tx_total,
                kind: "counter".to_owned(),
            },
            IntradayStatRow {
                metric: "tunnel.bytes.rx".to_owned(),
                labels: labels.clone(),
                bucket_ts: noon_ts,
                value: rx_total,
                kind: "counter".to_owned(),
            },
            IntradayStatRow {
                metric: "tunnel.latency.rtt_ms".to_owned(),
                labels: labels.clone(),
                bucket_ts: noon_ts,
                value: avg_latency,
                kind: "gauge".to_owned(),
            },
        ];

        stats_repo.upsert_intraday(&day_rows).await?;
        stats_repo.rollup_daily(&day_str).await?;
    }

    Ok(())
}

// ── DNS query log fixture ─────────────────────────────────────────────────

/// Sorted-JSON labels for a `dns.queries.by_client` row, matching the
/// format the real log sink writes: `device_id` present only when the
/// client is attributed to a device (keys sorted: `client` < `device_id`).
fn client_stat_labels(device_id: Option<&str>, client_ip: &str) -> String {
    match device_id {
        Some(id) => format!(r#"{{"client":"{client_ip}","device_id":"{id}"}}"#),
        None => format!(r#"{{"client":"{client_ip}"}}"#),
    }
}

/// Build a 24-hour synthetic query log spread across the seeded device IPs.
/// Returns rows in chronological order (oldest first) so paginated UI views
/// show recent queries first.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]
fn generate_dns_query_log(
    clients: &[(Option<String>, String)],
    now: chrono::DateTime<Utc>,
) -> Vec<QueryLogRow> {
    if clients.is_empty() {
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
            let (device_id, client) = &clients[(seed as usize) % clients.len()];
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
            } else if bucket_pick == 8 {
                // 10 % negative answers (AAAA/HTTPS for names lacking the
                // record type, or nonexistent hosts) — NXDOMAIN/NODATA.
                (
                    popular[(seed >> 15) as usize % popular.len()].to_owned(),
                    "negative",
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
            let upstream = if result == "forwarded" || result == "negative" {
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
                device_id: device_id.clone(),
                protocol: "udp".to_owned(),
            });
        }
    }

    rows
}

// ── DNS stats seeding ─────────────────────────────────────────────────────

const AD_BLOCKED_DOMAINS: [&str; 5] = [
    "doubleclick.net",
    "googletagmanager.com",
    "adservice.google.com",
    "ads.facebook.com",
    "tracker.example.net",
];

/// Domain share of the blocked-query total (must sum to 1.0).
const DOMAIN_WEIGHTS: [f64; 5] = [0.35, 0.25, 0.20, 0.12, 0.08];

/// Client share of total queries (must sum to 1.0, first 5 entries used).
const CLIENT_WEIGHTS: [f64; 5] = [0.30, 0.25, 0.20, 0.15, 0.10];

/// Generate 48 h of per-minute intraday stats rows for the DNS metrics.
///
/// Covers the 1h, 24h, and partial 7d tabs. The event emitter keeps adding
/// live rows on top of these, so the charts stay fresh after launch.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]
fn generate_dns_intraday_stats(
    clients: &[(Option<String>, String)],
    now: chrono::DateTime<Utc>,
) -> Vec<IntradayStatRow> {
    const HOURS: i64 = 48;
    // Peak ~10 queries/min at 20:00, trough ~2/min at 04:00.
    const BASE_QPM: f64 = 10.0;

    let end_ts = (now.timestamp() / 60) * 60;
    let start_ts = end_ts - HOURS * 3_600;

    let capacity =
        ((end_ts - start_ts) / 60) as usize * (3 + AD_BLOCKED_DOMAINS.len() + clients.len().min(5));
    let mut rows = Vec::with_capacity(capacity);

    let mut minute_ts = start_ts;
    while minute_ts <= end_ts {
        let hour_of_day = ((minute_ts % 86_400) as f64) / 3_600.0;
        // Diurnal: cosine peaking at 20:00, trough at 08:00.
        let phase = (hour_of_day - 20.0) / 24.0 * std::f64::consts::TAU;
        let diurnal = (phase.cos() * 0.4 + 0.6).clamp(0.2, 1.0);

        let seed = minute_ts as u64;
        let jitter = ((seed.wrapping_mul(2_654_435_761) % 21) as f64 - 10.0) / 100.0;
        let total_qpm = (BASE_QPM * diurnal * (1.0 + jitter)).max(0.5);

        let blocked = (total_qpm * 0.20).max(0.05);
        let forwarded = total_qpm * 0.50;
        let cached = total_qpm * 0.30;

        rows.push(IntradayStatRow {
            metric: "dns.queries".to_owned(),
            labels: r#"{"outcome":"blocked"}"#.to_owned(),
            bucket_ts: minute_ts,
            value: blocked,
            kind: "counter".to_owned(),
        });
        rows.push(IntradayStatRow {
            metric: "dns.queries".to_owned(),
            labels: r#"{"outcome":"forwarded"}"#.to_owned(),
            bucket_ts: minute_ts,
            value: forwarded,
            kind: "counter".to_owned(),
        });
        rows.push(IntradayStatRow {
            metric: "dns.queries".to_owned(),
            labels: r#"{"outcome":"cached"}"#.to_owned(),
            bucket_ts: minute_ts,
            value: cached,
            kind: "counter".to_owned(),
        });

        for (i, domain) in AD_BLOCKED_DOMAINS.iter().enumerate() {
            rows.push(IntradayStatRow {
                metric: "dns.queries.by_domain".to_owned(),
                labels: format!(r#"{{"domain":"{domain}"}}"#),
                bucket_ts: minute_ts,
                value: (blocked * DOMAIN_WEIGHTS[i]).max(0.01),
                kind: "counter".to_owned(),
            });
        }

        for (i, (device_id, client_ip)) in clients.iter().enumerate().take(5) {
            rows.push(IntradayStatRow {
                metric: "dns.queries.by_client".to_owned(),
                labels: client_stat_labels(device_id.as_deref(), client_ip),
                bucket_ts: minute_ts,
                value: (total_qpm * CLIENT_WEIGHTS[i]).max(0.01),
                kind: "counter".to_owned(),
            });
        }

        minute_ts += 60;
    }

    rows
}

/// Insert one representative intraday row per `(day, metric, labels)` then
/// roll it into `stats_daily`, giving the 12m chart 13 months of history.
///
/// The historical intraday rows survive until the flush runner's trim pass;
/// the daily rows persist permanently. Returns the number of days processed.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]
async fn seed_daily_stats(
    stats_repo: &dyn wardnetd_data::repository::StatsRepository,
    clients: &[(Option<String>, String)],
    now: chrono::DateTime<Utc>,
) -> anyhow::Result<usize> {
    const DAILY_RETENTION_DAYS: i64 = 397;
    // ~14 400 queries per weekday (10/min × 1440 min).
    const BASE_QUERIES_PER_DAY: f64 = 14_400.0;

    for days_ago in 1..=DAILY_RETENTION_DAYS {
        let day_dt = now - Duration::days(days_ago);
        let day_str = day_dt.format("%Y-%m-%d").to_string();

        // Noon UTC timestamp — always within the correct calendar day.
        let day_ts = day_dt.timestamp();
        let midnight_ts = day_ts - (day_ts % 86_400);
        let noon_ts = midnight_ts + 43_200;

        let dow = day_dt.weekday().num_days_from_monday();
        let weekend_boost = if dow >= 5 { 1.3 } else { 1.0 };
        let jitter_seed = (days_ago as u64).wrapping_mul(2_654_435_761);
        let jitter = ((jitter_seed % 21) as f64 - 10.0) / 100.0;

        let total = (BASE_QUERIES_PER_DAY * weekend_boost * (1.0 + jitter)).max(100.0);
        let blocked = total * 0.20;
        let forwarded = total * 0.50;
        let cached = total * 0.30;

        let mut day_rows = vec![
            IntradayStatRow {
                metric: "dns.queries".to_owned(),
                labels: r#"{"outcome":"blocked"}"#.to_owned(),
                bucket_ts: noon_ts,
                value: blocked,
                kind: "counter".to_owned(),
            },
            IntradayStatRow {
                metric: "dns.queries".to_owned(),
                labels: r#"{"outcome":"forwarded"}"#.to_owned(),
                bucket_ts: noon_ts,
                value: forwarded,
                kind: "counter".to_owned(),
            },
            IntradayStatRow {
                metric: "dns.queries".to_owned(),
                labels: r#"{"outcome":"cached"}"#.to_owned(),
                bucket_ts: noon_ts,
                value: cached,
                kind: "counter".to_owned(),
            },
        ];

        for (i, domain) in AD_BLOCKED_DOMAINS.iter().enumerate() {
            day_rows.push(IntradayStatRow {
                metric: "dns.queries.by_domain".to_owned(),
                labels: format!(r#"{{"domain":"{domain}"}}"#),
                bucket_ts: noon_ts,
                value: (blocked * DOMAIN_WEIGHTS[i]).max(0.1),
                kind: "counter".to_owned(),
            });
        }

        for (i, (device_id, client_ip)) in clients.iter().enumerate().take(5) {
            day_rows.push(IntradayStatRow {
                metric: "dns.queries.by_client".to_owned(),
                labels: client_stat_labels(device_id.as_deref(), client_ip),
                bucket_ts: noon_ts,
                value: (total * CLIENT_WEIGHTS[i]).max(0.1),
                kind: "counter".to_owned(),
            });
        }

        stats_repo.upsert_intraday(&day_rows).await?;
        stats_repo.rollup_daily(&day_str).await?;
    }

    Ok(DAILY_RETENTION_DAYS as usize)
}
