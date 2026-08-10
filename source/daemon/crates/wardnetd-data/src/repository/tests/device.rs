use super::test_pool;
use crate::repository::device::DeviceRow;
use crate::repository::{DeviceRepository, SqliteDeviceRepository};
use wardnet_common::device::{DeviceConnectionMode, DeviceType, ManufacturerSource};
use wardnet_common::routing::{RoutingTarget, RuleCreator};

const DEV1: &str = "00000000-0000-0000-0000-000000000001";
const DEV2: &str = "00000000-0000-0000-0000-000000000002";
const DEV3: &str = "00000000-0000-0000-0000-000000000003";

fn sample_device_row(id: &str, mac: &str, ip: &str) -> DeviceRow {
    DeviceRow {
        id: id.to_owned(),
        mac: mac.to_owned(),
        hostname: Some("my-host".to_owned()),
        manufacturer: Some("Apple".to_owned()),
        manufacturer_source: None,
        is_randomized: false,
        device_type: "phone".to_owned(),
        first_seen: "2026-03-07T00:00:00Z".to_owned(),
        last_seen: "2026-03-07T00:00:00Z".to_owned(),
        last_ip: ip.to_owned(),
        zone_id: "00000000-0000-0000-0000-000000000201".to_owned(),
        connection_mode: DeviceConnectionMode::Lan,
    }
}

async fn insert_device(pool: &sqlx::SqlitePool, id: &str, mac: &str, ip: &str) {
    insert_device_seen_at(pool, id, mac, ip, "2026-03-07T00:00:00Z").await;
}

async fn insert_device_seen_at(
    pool: &sqlx::SqlitePool,
    id: &str,
    mac: &str,
    ip: &str,
    last_seen: &str,
) {
    sqlx::query(
        "INSERT INTO devices (id, mac, last_ip, device_type, first_seen, last_seen, zone_id) \
         VALUES (?, ?, ?, 'unknown', ?, ?, '00000000-0000-0000-0000-000000000201')",
    )
    .bind(id)
    .bind(mac)
    .bind(ip)
    .bind("2026-03-07T00:00:00Z")
    .bind(last_seen)
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn find_by_ip_found() {
    let pool = test_pool().await;
    insert_device(&pool, DEV1, "aa:bb:cc:dd:ee:01", "192.168.1.10").await;
    let repo = SqliteDeviceRepository::new(pool);

    let device = repo.find_by_ip("192.168.1.10").await.unwrap().unwrap();
    assert_eq!(device.id.to_string(), DEV1);
    assert_eq!(device.mac, "aa:bb:cc:dd:ee:01");
    assert_eq!(device.last_ip, "192.168.1.10");
    assert_eq!(device.device_type, DeviceType::Unknown);
    assert!(!device.admin_locked);
}

#[tokio::test]
async fn find_by_ip_not_found() {
    let pool = test_pool().await;
    let repo = SqliteDeviceRepository::new(pool);

    let result = repo.find_by_ip("10.0.0.99").await.unwrap();
    assert!(result.is_none());
}

// Regression for issue #831: `last_ip` is not unique (departed devices keep
// their row), so DHCP recycling an address leaves two rows sharing it. The
// lookup must resolve to the most recently seen (live) device, not an arbitrary
// rowid-ordered (stale) one — the IP-keyed self-service auth path depends on it.
#[tokio::test]
async fn find_by_ip_returns_most_recently_seen_on_collision() {
    let pool = test_pool().await;
    // DEV1 is the older, departed occupant of the address; DEV2 is the live
    // device DHCP later handed the same IP to. DEV1 is inserted first so it wins
    // the rowid ordering that the un-ordered query used to return.
    insert_device_seen_at(
        &pool,
        DEV1,
        "aa:bb:cc:dd:ee:01",
        "192.168.1.10",
        "2026-03-07T00:00:00Z",
    )
    .await;
    insert_device_seen_at(
        &pool,
        DEV2,
        "aa:bb:cc:dd:ee:02",
        "192.168.1.10",
        "2026-03-08T00:00:00Z",
    )
    .await;
    let repo = SqliteDeviceRepository::new(pool);

    let device = repo.find_by_ip("192.168.1.10").await.unwrap().unwrap();
    assert_eq!(
        device.id.to_string(),
        DEV2,
        "collision must resolve to the more recently seen device"
    );
}

// The mDNS observer (issue #1115) needs the *full* set of devices on an address,
// not just the most-recently-seen one, so it can skip attributing a vendor when
// the mapping is ambiguous. `find_all_by_ip` returns every row sharing the IP.
#[tokio::test]
async fn find_all_by_ip_returns_every_device_on_a_shared_address() {
    let pool = test_pool().await;
    insert_device_seen_at(
        &pool,
        DEV1,
        "aa:bb:cc:dd:ee:01",
        "192.168.1.10",
        "2026-03-07T00:00:00Z",
    )
    .await;
    insert_device_seen_at(
        &pool,
        DEV2,
        "aa:bb:cc:dd:ee:02",
        "192.168.1.10",
        "2026-03-08T00:00:00Z",
    )
    .await;
    insert_device(&pool, DEV3, "aa:bb:cc:dd:ee:03", "192.168.1.20").await;
    let repo = SqliteDeviceRepository::new(pool);

    let shared = repo.find_all_by_ip("192.168.1.10").await.unwrap();
    assert_eq!(
        shared.len(),
        2,
        "both claimants of the address are returned"
    );

    let lone = repo.find_all_by_ip("192.168.1.20").await.unwrap();
    assert_eq!(lone.len(), 1);
    assert_eq!(lone[0].id.to_string(), DEV3);

    assert!(
        repo.find_all_by_ip("10.0.0.99").await.unwrap().is_empty(),
        "an unclaimed address matches nothing"
    );
}

// The empty-string sentinel of departed rows must never match, exactly as in
// `find_by_ip` — otherwise every departed device would resolve to one address.
#[tokio::test]
async fn find_all_by_ip_empty_returns_none() {
    let pool = test_pool().await;
    insert_device_seen_at(&pool, DEV1, "aa:bb:cc:dd:ee:01", "", "2026-03-07T00:00:00Z").await;
    insert_device_seen_at(&pool, DEV2, "aa:bb:cc:dd:ee:02", "", "2026-03-08T00:00:00Z").await;
    let repo = SqliteDeviceRepository::new(pool);

    assert!(repo.find_all_by_ip("").await.unwrap().is_empty());
}

// Regression for issue #831: clearing a departed device's `last_ip` must make
// its row unresolvable by that IP so it can never again be returned for a live
// device's source address.
#[tokio::test]
async fn clear_last_ip_removes_row_from_ip_lookup() {
    let pool = test_pool().await;
    insert_device(&pool, DEV1, "aa:bb:cc:dd:ee:01", "192.168.1.10").await;
    let repo = SqliteDeviceRepository::new(pool);

    // Precondition: the device resolves by its IP.
    assert!(repo.find_by_ip("192.168.1.10").await.unwrap().is_some());

    repo.clear_last_ip(DEV1).await.unwrap();

    // The row still exists but no longer carries the address...
    let device = repo.find_by_id(DEV1).await.unwrap().unwrap();
    assert_eq!(device.last_ip, "");
    // ...so a lookup for that IP no longer returns the departed device.
    assert!(repo.find_by_ip("192.168.1.10").await.unwrap().is_none());
}

// Regression for issue #831: empty string is the "no known address" sentinel
// written to departed devices, not a real address. Every departed device shares
// it, so an empty lookup must never match — otherwise it would resolve to an
// arbitrary departed row (and, via `get_device_for_ip(&device.last_ip)`, surface
// another device's routing rule).
#[tokio::test]
async fn find_by_ip_empty_returns_none() {
    let pool = test_pool().await;
    // Two departed devices, both with their address cleared to the empty
    // sentinel and distinct last_seen timestamps.
    insert_device_seen_at(&pool, DEV1, "aa:bb:cc:dd:ee:01", "", "2026-03-07T00:00:00Z").await;
    insert_device_seen_at(&pool, DEV2, "aa:bb:cc:dd:ee:02", "", "2026-03-08T00:00:00Z").await;
    let repo = SqliteDeviceRepository::new(pool);

    assert!(repo.find_by_ip("").await.unwrap().is_none());
}

#[tokio::test]
async fn find_by_id_found() {
    let pool = test_pool().await;
    insert_device(&pool, DEV1, "aa:bb:cc:dd:ee:01", "192.168.1.10").await;
    let repo = SqliteDeviceRepository::new(pool);

    let device = repo.find_by_id(DEV1).await.unwrap().unwrap();
    assert_eq!(device.mac, "aa:bb:cc:dd:ee:01");
}

#[tokio::test]
async fn find_by_mac_found() {
    let pool = test_pool().await;
    insert_device(&pool, DEV1, "aa:bb:cc:dd:ee:01", "192.168.1.10").await;
    let repo = SqliteDeviceRepository::new(pool);

    let device = repo
        .find_by_mac("aa:bb:cc:dd:ee:01")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(device.id.to_string(), DEV1);
}

#[tokio::test]
async fn find_by_mac_not_found() {
    let pool = test_pool().await;
    let repo = SqliteDeviceRepository::new(pool);

    let result = repo.find_by_mac("ff:ff:ff:ff:ff:ff").await.unwrap();
    assert!(result.is_none());
}

/// Cross-case equality acceptance test for issue #312: an uppercase MAC
/// inserted via the repo must be findable with a lowercase MAC argument
/// (and the row must come back in canonical lowercase form). This pins
/// the contract that `SqliteDeviceRepository` lowercases on both write
/// and `_by_mac` lookup, so callers can stop normalising per-call.
#[tokio::test]
async fn insert_uppercase_mac_is_findable_by_lowercase() {
    let pool = test_pool().await;
    let repo = SqliteDeviceRepository::new(pool);

    let row = sample_device_row(DEV1, "AA:BB:CC:DD:EE:01", "192.168.1.10");
    repo.insert(&row).await.unwrap();

    let device = repo
        .find_by_mac("aa:bb:cc:dd:ee:01")
        .await
        .unwrap()
        .expect("lowercase lookup must hit the row inserted with uppercase MAC");
    assert_eq!(device.mac, "aa:bb:cc:dd:ee:01");

    // Reverse direction also works: uppercase argument resolves the
    // (now-lowercase) stored row.
    let again = repo
        .find_by_mac("AA:BB:CC:DD:EE:01")
        .await
        .unwrap()
        .expect("uppercase lookup must hit the same canonical row");
    assert_eq!(again.id, device.id);
}

#[tokio::test]
async fn insert_new_device() {
    let pool = test_pool().await;
    let repo = SqliteDeviceRepository::new(pool);

    let row = sample_device_row(DEV1, "aa:bb:cc:dd:ee:01", "192.168.1.10");
    repo.insert(&row).await.unwrap();

    let device = repo.find_by_id(DEV1).await.unwrap().unwrap();
    assert_eq!(device.mac, "aa:bb:cc:dd:ee:01");
    assert_eq!(device.hostname, Some("my-host".to_owned()));
    assert_eq!(device.manufacturer, Some("Apple".to_owned()));
    assert_eq!(device.device_type, DeviceType::Phone);
    assert_eq!(device.last_ip, "192.168.1.10");
}

#[tokio::test]
async fn update_last_seen_and_ip() {
    let pool = test_pool().await;
    let repo = SqliteDeviceRepository::new(pool);

    let row = sample_device_row(DEV1, "aa:bb:cc:dd:ee:01", "192.168.1.10");
    repo.insert(&row).await.unwrap();

    repo.update_last_seen_and_ip(
        DEV1,
        "192.168.1.20",
        "2026-03-07T12:00:00Z",
        DeviceConnectionMode::Remote,
    )
    .await
    .unwrap();

    let device = repo.find_by_id(DEV1).await.unwrap().unwrap();
    assert_eq!(device.last_ip, "192.168.1.20");
    assert_eq!(device.last_seen.to_rfc3339(), "2026-03-07T12:00:00+00:00");
    assert_eq!(device.connection_mode, DeviceConnectionMode::Remote);
}

#[tokio::test]
async fn update_last_seen_batch_multiple() {
    let pool = test_pool().await;
    let repo = SqliteDeviceRepository::new(pool);

    repo.insert(&sample_device_row(
        DEV1,
        "aa:bb:cc:dd:ee:01",
        "192.168.1.10",
    ))
    .await
    .unwrap();
    repo.insert(&sample_device_row(
        DEV2,
        "aa:bb:cc:dd:ee:02",
        "192.168.1.11",
    ))
    .await
    .unwrap();

    let updates = vec![
        (DEV1.to_owned(), "2026-03-07T06:00:00Z".to_owned()),
        (DEV2.to_owned(), "2026-03-07T07:00:00Z".to_owned()),
    ];
    repo.update_last_seen_batch(&updates).await.unwrap();

    let d1 = repo.find_by_id(DEV1).await.unwrap().unwrap();
    let d2 = repo.find_by_id(DEV2).await.unwrap().unwrap();
    assert_eq!(d1.last_seen.to_rfc3339(), "2026-03-07T06:00:00+00:00");
    assert_eq!(d2.last_seen.to_rfc3339(), "2026-03-07T07:00:00+00:00");
}

#[tokio::test]
async fn update_hostname() {
    let pool = test_pool().await;
    let repo = SqliteDeviceRepository::new(pool);

    repo.insert(&sample_device_row(
        DEV1,
        "aa:bb:cc:dd:ee:01",
        "192.168.1.10",
    ))
    .await
    .unwrap();

    repo.update_hostname(DEV1, "new-hostname").await.unwrap();

    let device = repo.find_by_id(DEV1).await.unwrap().unwrap();
    assert_eq!(device.hostname, Some("new-hostname".to_owned()));
}

#[tokio::test]
async fn update_name_and_type() {
    let pool = test_pool().await;
    let repo = SqliteDeviceRepository::new(pool);

    repo.insert(&sample_device_row(
        DEV1,
        "aa:bb:cc:dd:ee:01",
        "192.168.1.10",
    ))
    .await
    .unwrap();

    repo.update_name_and_type(DEV1, Some("Living Room TV"), "tv")
        .await
        .unwrap();

    let device = repo.find_by_id(DEV1).await.unwrap().unwrap();
    assert_eq!(device.name, Some("Living Room TV".to_owned()));
    assert_eq!(device.device_type, DeviceType::Tv);
}

#[tokio::test]
async fn find_stale_returns_old_devices() {
    let pool = test_pool().await;
    let repo = SqliteDeviceRepository::new(pool);

    // Device 1: old last_seen.
    let mut row1 = sample_device_row(DEV1, "aa:bb:cc:dd:ee:01", "192.168.1.10");
    row1.last_seen = "2026-03-06T00:00:00Z".to_owned();
    repo.insert(&row1).await.unwrap();

    // Device 2: recent last_seen.
    let mut row2 = sample_device_row(DEV2, "aa:bb:cc:dd:ee:02", "192.168.1.11");
    row2.last_seen = "2026-03-07T12:00:00Z".to_owned();
    repo.insert(&row2).await.unwrap();

    let stale = repo.find_stale("2026-03-07T00:00:00Z").await.unwrap();
    assert_eq!(stale.len(), 1);
    assert_eq!(stale[0].id.to_string(), DEV1);
}

#[tokio::test]
async fn find_all_returns_all_devices() {
    let pool = test_pool().await;
    let repo = SqliteDeviceRepository::new(pool);

    repo.insert(&sample_device_row(
        DEV1,
        "aa:bb:cc:dd:ee:01",
        "192.168.1.10",
    ))
    .await
    .unwrap();
    repo.insert(&sample_device_row(
        DEV2,
        "aa:bb:cc:dd:ee:02",
        "192.168.1.11",
    ))
    .await
    .unwrap();
    repo.insert(&sample_device_row(
        DEV3,
        "aa:bb:cc:dd:ee:03",
        "192.168.1.12",
    ))
    .await
    .unwrap();

    let devices = repo.find_all().await.unwrap();
    assert_eq!(devices.len(), 3);
}

#[tokio::test]
async fn find_rule_for_device_found() {
    let pool = test_pool().await;
    insert_device(&pool, DEV1, "aa:bb:cc:dd:ee:01", "192.168.1.10").await;
    sqlx::query(
        "INSERT INTO routing_rules (id, device_id, target_json, created_by) \
         VALUES ('r1', ?, '{\"type\":\"direct\"}', 'user')",
    )
    .bind(DEV1)
    .execute(&pool)
    .await
    .unwrap();

    let repo = SqliteDeviceRepository::new(pool);
    let rule = repo.find_rule_for_device(DEV1).await.unwrap().unwrap();
    assert_eq!(rule.target, RoutingTarget::Direct);
    assert_eq!(rule.created_by, RuleCreator::User);
}

#[tokio::test]
async fn find_rule_not_found() {
    let pool = test_pool().await;
    insert_device(&pool, DEV1, "aa:bb:cc:dd:ee:01", "192.168.1.10").await;
    let repo = SqliteDeviceRepository::new(pool);

    let result = repo.find_rule_for_device(DEV1).await.unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn find_all_rules_returns_every_rule_once() {
    let pool = test_pool().await;
    // DEV1 → tunnel, DEV2 → direct, DEV3 → explicit default; a fourth device
    // with no rule must be absent from the result.
    insert_device(&pool, DEV1, "aa:bb:cc:dd:ee:01", "192.168.1.10").await;
    insert_device(&pool, DEV2, "aa:bb:cc:dd:ee:02", "192.168.1.11").await;
    insert_device(&pool, DEV3, "aa:bb:cc:dd:ee:03", "192.168.1.12").await;
    let dev4 = "00000000-0000-0000-0000-000000000004";
    insert_device(&pool, dev4, "aa:bb:cc:dd:ee:04", "192.168.1.13").await;

    let tunnel_id = "11111111-1111-1111-1111-111111111111";
    for (id, target_json) in [
        (
            DEV1,
            format!("{{\"type\":\"tunnel\",\"tunnel_id\":\"{tunnel_id}\"}}"),
        ),
        (DEV2, "{\"type\":\"direct\"}".to_owned()),
        (DEV3, "{\"type\":\"default\"}".to_owned()),
    ] {
        sqlx::query(
            "INSERT INTO routing_rules (id, device_id, target_json, created_by) \
             VALUES (?, ?, ?, 'user')",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(id)
        .bind(target_json)
        .execute(&pool)
        .await
        .unwrap();
    }

    let repo = SqliteDeviceRepository::new(pool);
    let rules = repo.find_all_rules().await.unwrap();

    // Exactly the three devices with a rule — dev4 (no rule) is absent.
    assert_eq!(rules.len(), 3);
    let by_id: std::collections::HashMap<_, _> = rules
        .into_iter()
        .map(|r| (r.device_id.to_string(), r.target))
        .collect();
    assert_eq!(
        by_id.get(DEV1),
        Some(&RoutingTarget::Tunnel {
            tunnel_id: tunnel_id.parse().unwrap(),
        })
    );
    assert_eq!(by_id.get(DEV2), Some(&RoutingTarget::Direct));
    assert_eq!(by_id.get(DEV3), Some(&RoutingTarget::Default));
    assert!(!by_id.contains_key(dev4));
}

#[tokio::test]
async fn find_all_rules_empty_when_no_rules() {
    let pool = test_pool().await;
    insert_device(&pool, DEV1, "aa:bb:cc:dd:ee:01", "192.168.1.10").await;
    let repo = SqliteDeviceRepository::new(pool);

    let rules = repo.find_all_rules().await.unwrap();
    assert!(rules.is_empty());
}

#[tokio::test]
async fn upsert_user_rule_insert_and_update() {
    let pool = test_pool().await;
    insert_device(&pool, DEV1, "aa:bb:cc:dd:ee:01", "192.168.1.10").await;
    let repo = SqliteDeviceRepository::new(pool);

    // Insert.
    repo.upsert_user_rule(DEV1, "{\"type\":\"direct\"}", "2026-03-07T00:00:00Z")
        .await
        .unwrap();
    let rule = repo.find_rule_for_device(DEV1).await.unwrap().unwrap();
    assert_eq!(rule.target, RoutingTarget::Direct);

    // Update (upsert).
    repo.upsert_user_rule(DEV1, "{\"type\":\"default\"}", "2026-03-07T01:00:00Z")
        .await
        .unwrap();
    let rule = repo.find_rule_for_device(DEV1).await.unwrap().unwrap();
    assert_eq!(rule.target, RoutingTarget::Default);
}

#[tokio::test]
async fn update_admin_locked_sets_flag() {
    let pool = test_pool().await;
    let repo = SqliteDeviceRepository::new(pool);

    repo.insert(&sample_device_row(
        DEV1,
        "aa:bb:cc:dd:ee:01",
        "192.168.1.10",
    ))
    .await
    .unwrap();

    // Initially unlocked.
    let device = repo.find_by_id(DEV1).await.unwrap().unwrap();
    assert!(!device.admin_locked);

    // Lock.
    repo.update_admin_locked(DEV1, true).await.unwrap();
    let device = repo.find_by_id(DEV1).await.unwrap().unwrap();
    assert!(device.admin_locked);

    // Unlock.
    repo.update_admin_locked(DEV1, false).await.unwrap();
    let device = repo.find_by_id(DEV1).await.unwrap().unwrap();
    assert!(!device.admin_locked);
}

#[tokio::test]
async fn count_devices() {
    let pool = test_pool().await;
    insert_device(&pool, DEV1, "aa:bb:cc:dd:ee:01", "192.168.1.10").await;
    insert_device(&pool, DEV2, "aa:bb:cc:dd:ee:02", "192.168.1.11").await;
    insert_device(&pool, DEV3, "aa:bb:cc:dd:ee:03", "192.168.1.12").await;
    let repo = SqliteDeviceRepository::new(pool);

    assert_eq!(repo.count().await.unwrap(), 3);
}

#[tokio::test]
async fn update_dns_capture_settings_found() {
    let pool = test_pool().await;
    insert_device(&pool, DEV1, "aa:bb:cc:dd:ee:01", "192.168.1.10").await;
    let repo = SqliteDeviceRepository::new(pool);

    let updated = repo
        .update_dns_capture_settings(DEV1, Some(true), Some(500), Some(14))
        .await
        .unwrap();
    assert!(updated, "should return true when the device exists");

    let device = repo.find_by_id(DEV1).await.unwrap().unwrap();
    assert!(device.dns_capture_enabled);
    assert_eq!(device.dns_capture_cap_count, 500);
    assert_eq!(device.dns_capture_cap_days, 14);
}

#[tokio::test]
async fn update_dns_capture_settings_not_found() {
    let pool = test_pool().await;
    let repo = SqliteDeviceRepository::new(pool);

    let nonexistent = "00000000-0000-0000-0000-000000000099";
    let updated = repo
        .update_dns_capture_settings(nonexistent, Some(true), Some(500), Some(14))
        .await
        .unwrap();
    assert!(
        !updated,
        "should return false when the device does not exist"
    );
}

#[tokio::test]
async fn update_dns_capture_settings_partial() {
    let pool = test_pool().await;
    insert_device(&pool, DEV1, "aa:bb:cc:dd:ee:01", "192.168.1.10").await;
    let repo = SqliteDeviceRepository::new(pool);

    let updated = repo
        .update_dns_capture_settings(DEV1, Some(true), None, None)
        .await
        .unwrap();
    assert!(updated);

    let device = repo.find_by_id(DEV1).await.unwrap().unwrap();
    assert!(device.dns_capture_enabled);
    // cap_count and cap_days must retain the migration defaults (1000 and 7).
    assert_eq!(device.dns_capture_cap_count, 1000);
    assert_eq!(device.dns_capture_cap_days, 7);
}

#[tokio::test]
async fn find_all_capture_enabled_ids_returns_enabled_only() {
    let pool = test_pool().await;
    insert_device(&pool, DEV1, "aa:bb:cc:dd:ee:01", "192.168.1.10").await;
    insert_device(&pool, DEV2, "aa:bb:cc:dd:ee:02", "192.168.1.11").await;
    insert_device(&pool, DEV3, "aa:bb:cc:dd:ee:03", "192.168.1.12").await;
    let repo = SqliteDeviceRepository::new(pool);

    // Enable capture on DEV1 and DEV3; leave DEV2 disabled.
    repo.update_dns_capture_settings(DEV1, Some(true), None, None)
        .await
        .unwrap();
    repo.update_dns_capture_settings(DEV3, Some(true), None, None)
        .await
        .unwrap();

    let mut ids = repo.find_all_capture_enabled_ids().await.unwrap();
    ids.sort();

    let mut expected = vec![DEV1.to_owned(), DEV3.to_owned()];
    expected.sort();

    assert_eq!(ids, expected);
}

#[tokio::test]
async fn find_devices_for_tunnel_selects_every_mapped_column() {
    // Regression guard (issue #1099): this query builds its column list inline
    // rather than from the shared `const … _SQL` strings, so it silently fell
    // out of sync when `DeviceRow` gained `manufacturer_source`/`is_randomized`
    // and failed at runtime with ColumnNotFound. Every other caller in the tree
    // is a mock, so nothing caught it — this exercises the real SQL.
    let pool = test_pool().await;
    insert_device(&pool, DEV1, "aa:bb:cc:dd:ee:01", "192.168.1.10").await;
    let tunnel_id = "00000000-0000-0000-0000-0000000003a1";
    sqlx::query(
        "INSERT INTO routing_rules (id, device_id, target_json, created_by) \
         VALUES ('r1', ?, ?, 'user')",
    )
    .bind(DEV1)
    .bind(format!(
        "{{\"type\":\"tunnel\",\"tunnel_id\":\"{tunnel_id}\"}}"
    ))
    .execute(&pool)
    .await
    .unwrap();

    let repo = SqliteDeviceRepository::new(pool);
    let devices = repo.find_devices_for_tunnel(tunnel_id).await.unwrap();

    assert_eq!(devices.len(), 1);
    assert_eq!(devices[0].mac, "aa:bb:cc:dd:ee:01");
}

#[tokio::test]
async fn randomized_flag_is_derived_from_the_address_not_the_old_sentinel() {
    // The migration derives `is_randomized` from the locally-administered bit
    // rather than from the old 'Randomized MAC' manufacturer string, so a row
    // that never carried the sentinel is still classified correctly (#1099).
    let pool = test_pool().await;
    // 0x02 has the LA bit set; 0xa8 does not.
    insert_device(&pool, DEV1, "02:1a:2b:3c:4d:5e", "192.168.1.10").await;
    insert_device(&pool, DEV2, "a8:bb:cc:dd:ee:ff", "192.168.1.11").await;

    // The rows above were inserted *after* migrations ran, so re-run the
    // backfill's own predicate against them. That is the part that can be
    // wrong — the set of second hex characters carrying the LA bit.
    sqlx::query(
        "UPDATE devices SET is_randomized = 1 \
         WHERE substr(mac, 2, 1) IN ('2', '3', '6', '7', 'a', 'b', 'e', 'f')",
    )
    .execute(&pool)
    .await
    .unwrap();

    let flags: Vec<(String, i64)> =
        sqlx::query_as("SELECT mac, is_randomized FROM devices ORDER BY mac")
            .fetch_all(&pool)
            .await
            .unwrap();

    assert_eq!(
        flags,
        vec![
            ("02:1a:2b:3c:4d:5e".to_owned(), 1),
            ("a8:bb:cc:dd:ee:ff".to_owned(), 0),
        ]
    );
}

#[tokio::test]
async fn insert_round_trips_manufacturer_provenance_and_randomized_flag() {
    // Covers the write side of the identification columns (issue #1099): the
    // other device tests all insert with `manufacturer_source: None`, so the
    // enum-to-string serialisation on insert was never exercised.
    let pool = test_pool().await;
    let repo = SqliteDeviceRepository::new(pool);

    let mut row = sample_device_row(DEV1, "5c:e7:53:4e:ec:d9", "192.168.1.10");
    row.manufacturer = Some("Govee".to_owned());
    row.manufacturer_source = Some(ManufacturerSource::Catalog);
    row.is_randomized = true;
    repo.insert(&row).await.unwrap();

    let device = repo.find_by_id(DEV1).await.unwrap().unwrap();
    assert_eq!(device.manufacturer.as_deref(), Some("Govee"));
    assert_eq!(
        device.manufacturer_source,
        Some(ManufacturerSource::Catalog)
    );
    assert!(device.is_randomized);
}

#[tokio::test]
async fn provenance_without_a_manufacturer_is_dropped_on_read() {
    // The documented invariant is "`manufacturer_source` is Some exactly when
    // `manufacturer` is Some" — restated by the SDK, the Go client and the
    // OpenAPI schema. A row that violates it must not surface a dangling
    // "likely" with nothing to qualify.
    let pool = test_pool().await;
    insert_device(&pool, DEV1, "aa:bb:cc:dd:ee:01", "192.168.1.10").await;
    sqlx::query(
        "UPDATE devices SET manufacturer = NULL, manufacturer_source = 'catalog' WHERE id = ?",
    )
    .bind(DEV1)
    .execute(&pool)
    .await
    .unwrap();

    let device = SqliteDeviceRepository::new(pool)
        .find_by_id(DEV1)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(device.manufacturer, None);
    assert_eq!(device.manufacturer_source, None);
}

// ── managed + retention (issue #1181) ────────────────────────────────────────

/// Enable foreign-key enforcement, matching production (`db.rs` sets this
/// PRAGMA on both pools).
///
/// It is off by default in `SQLite` and `test_pool` does not set it, so without
/// this a cascade test would pass by never cascading — and the orphaned rows
/// would only ever appear in production. The tests below assert the PRAGMA is
/// actually on before relying on it.
async fn enable_foreign_keys(pool: &sqlx::SqlitePool) {
    sqlx::query("PRAGMA foreign_keys = ON")
        .execute(pool)
        .await
        .unwrap();
    let on: i64 = sqlx::query_scalar("PRAGMA foreign_keys")
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(
        on, 1,
        "foreign keys must be enforced for the cascade to bite"
    );
}

async fn set_managed_flag(pool: &sqlx::SqlitePool, id: &str, managed: bool) {
    sqlx::query("UPDATE devices SET managed = ? WHERE id = ?")
        .bind(i32::from(managed))
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
}

async fn device_ids(pool: &sqlx::SqlitePool) -> Vec<String> {
    sqlx::query_scalar::<_, String>("SELECT id FROM devices ORDER BY id")
        .fetch_all(pool)
        .await
        .unwrap()
}

const STALE: &str = "2026-01-01T00:00:00Z";
const RECENT: &str = "2026-06-01T00:00:00Z";
const CUTOFF: &str = "2026-03-01T00:00:00Z";

#[tokio::test]
async fn insert_leaves_new_device_unmanaged() {
    // A freshly discovered device is never managed — `insert` omits the column
    // and takes the schema DEFAULT 0. This is what makes discovery-inserted
    // devices eligible for retention at all.
    let pool = test_pool().await;
    let repo = SqliteDeviceRepository::new(pool);
    repo.insert(&sample_device_row(
        DEV1,
        "aa:bb:cc:dd:ee:01",
        "192.168.1.10",
    ))
    .await
    .unwrap();

    let device = repo.find_by_id(DEV1).await.unwrap().unwrap();
    assert!(!device.managed);
}

#[tokio::test]
async fn set_managed_round_trips_and_is_idempotent() {
    let pool = test_pool().await;
    insert_device(&pool, DEV1, "aa:bb:cc:dd:ee:01", "192.168.1.10").await;
    let repo = SqliteDeviceRepository::new(pool);

    repo.set_managed(DEV1, true).await.unwrap();
    assert!(repo.find_by_id(DEV1).await.unwrap().unwrap().managed);

    // Promotion is called from every admin config act, so re-promoting an
    // already-managed device must be a no-op rather than an error.
    repo.set_managed(DEV1, true).await.unwrap();
    assert!(repo.find_by_id(DEV1).await.unwrap().unwrap().managed);

    repo.set_managed(DEV1, false).await.unwrap();
    assert!(!repo.find_by_id(DEV1).await.unwrap().unwrap().managed);
}

#[tokio::test]
async fn set_managed_on_missing_device_is_a_no_op() {
    let pool = test_pool().await;
    let repo = SqliteDeviceRepository::new(pool);
    // A promotion must never be the thing that fails an otherwise-successful
    // configuration change (e.g. a DHCP reservation for a not-yet-seen MAC).
    repo.set_managed(DEV1, true).await.unwrap();
}

#[tokio::test]
async fn delete_unmanaged_before_prunes_only_unmanaged_and_stale() {
    let pool = test_pool().await;
    // DEV1: unmanaged + stale  -> pruned
    // DEV2: unmanaged + recent -> kept (still around, just quiet)
    // DEV3: managed  + stale   -> kept (never auto-deleted at any age)
    insert_device_seen_at(&pool, DEV1, "aa:bb:cc:dd:ee:01", "192.168.1.10", STALE).await;
    insert_device_seen_at(&pool, DEV2, "aa:bb:cc:dd:ee:02", "192.168.1.11", RECENT).await;
    insert_device_seen_at(&pool, DEV3, "aa:bb:cc:dd:ee:03", "192.168.1.12", STALE).await;
    set_managed_flag(&pool, DEV3, true).await;

    let repo = SqliteDeviceRepository::new(pool.clone());
    let pruned = repo.delete_unmanaged_before(CUTOFF).await.unwrap();

    assert_eq!(pruned.len(), 1);
    assert_eq!(pruned[0].id, DEV1);
    assert_eq!(pruned[0].mac, "aa:bb:cc:dd:ee:01");
    assert_eq!(pruned[0].last_seen, STALE);
    assert_eq!(
        device_ids(&pool).await,
        vec![DEV2.to_owned(), DEV3.to_owned()]
    );
}

#[tokio::test]
async fn delete_unmanaged_before_prunes_a_departed_device_with_a_cleared_ip() {
    // A departed device has its `last_ip` emptied by `clear_last_ip`, not its
    // row deleted. The prune predicate keys on `last_seen` only, so the empty
    // sentinel must not exempt it — these are precisely the rows that
    // accumulate.
    let pool = test_pool().await;
    insert_device_seen_at(&pool, DEV1, "aa:bb:cc:dd:ee:01", "", STALE).await;
    insert_device_seen_at(&pool, DEV2, "aa:bb:cc:dd:ee:02", "192.168.1.11", STALE).await;

    let repo = SqliteDeviceRepository::new(pool.clone());
    let pruned = repo.delete_unmanaged_before(CUTOFF).await.unwrap();

    assert_eq!(pruned.len(), 2, "a cleared last_ip must not exempt a row");
    assert!(device_ids(&pool).await.is_empty());
}

#[tokio::test]
async fn delete_unmanaged_before_returns_empty_when_nothing_matches() {
    let pool = test_pool().await;
    insert_device_seen_at(&pool, DEV1, "aa:bb:cc:dd:ee:01", "192.168.1.10", RECENT).await;

    let repo = SqliteDeviceRepository::new(pool.clone());
    assert!(
        repo.delete_unmanaged_before(CUTOFF)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(device_ids(&pool).await, vec![DEV1.to_owned()]);
}

#[tokio::test]
async fn pruning_cascades_child_rows_and_nulls_the_lease_link() {
    // The safety argument for deleting an unmanaged row is that nothing admin
    // -authored references it, and that the machine-authored children clean
    // themselves up. This pins that second half: CASCADE children go, the
    // SET NULL lease link is nulled rather than deleting the lease.
    let pool = test_pool().await;
    enable_foreign_keys(&pool).await;
    insert_device_seen_at(&pool, DEV1, "aa:bb:cc:dd:ee:01", "192.168.1.10", STALE).await;

    sqlx::query(
        "INSERT INTO dns_events (device_id, domain, status, captured_at) \
         VALUES (?, 'example.com', 'allowed', ?)",
    )
    .bind(DEV1)
    .bind(STALE)
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO device_signals (device_id, kind, value) \
         VALUES (?, 'dhcp_hostname', 'my-host')",
    )
    .bind(DEV1)
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO dhcp_leases (id, mac_address, ip_address, lease_start, lease_end, device_id) \
         VALUES ('lease-1', 'aa:bb:cc:dd:ee:01', '192.168.1.10', ?, ?, ?)",
    )
    .bind(STALE)
    .bind(RECENT)
    .bind(DEV1)
    .execute(&pool)
    .await
    .unwrap();

    let repo = SqliteDeviceRepository::new(pool.clone());
    assert_eq!(repo.delete_unmanaged_before(CUTOFF).await.unwrap().len(), 1);

    let events: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM dns_events")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(events, 0, "dns_events should cascade away");

    let signals: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM device_signals")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(signals, 0, "device_signals should cascade away");

    let lease_device: Option<String> =
        sqlx::query_scalar("SELECT device_id FROM dhcp_leases WHERE id = 'lease-1'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        lease_device, None,
        "the lease survives with a null device_id"
    );
}

#[tokio::test]
async fn delete_rule_for_device_removes_the_rule_and_tolerates_a_missing_one() {
    // The release's revert-to-default. Deleting is used rather than writing a
    // `Direct` rule because "no rule" is the state a never-configured device is
    // in, and because a `Direct` write is zone-validated (issue #1181).
    let pool = test_pool().await;
    insert_device(&pool, DEV1, "aa:bb:cc:dd:ee:01", "192.168.1.10").await;
    let repo = SqliteDeviceRepository::new(pool);

    repo.upsert_user_rule(DEV1, r#"{"type":"direct"}"#, "2026-03-07T00:00:00Z")
        .await
        .unwrap();
    assert!(repo.find_rule_for_device(DEV1).await.unwrap().is_some());

    repo.delete_rule_for_device(DEV1).await.unwrap();
    assert!(repo.find_rule_for_device(DEV1).await.unwrap().is_none());

    // The release calls this unconditionally, so a second delete must be a
    // no-op rather than an error.
    repo.delete_rule_for_device(DEV1).await.unwrap();
}
