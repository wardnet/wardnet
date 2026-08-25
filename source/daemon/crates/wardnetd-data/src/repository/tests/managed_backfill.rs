//! Tests for the one-shot `managed` backfill in
//! `20260810000000_managed_devices.sql` (issue #1181).
//!
//! The backfill is a one-way door: it runs once per box, on real data, and
//! getting it wrong is either a silently-revoked credential (too exclusive) or
//! retention silently disabled forever (too inclusive). So these tests execute
//! the **shipped statement**, read out of the migration file, rather than a
//! copy pasted into the test — a copy would keep passing after the real
//! predicate drifted.
//!
//! Mechanics: `test_pool` has already run every migration, so `managed` exists
//! and is backfilled. Each test resets it to 0, seeds one artefact, and re-runs
//! the statement — which is idempotent, being a pure `UPDATE ... WHERE`.

use std::sync::LazyLock;

use sqlx::SqlitePool;

use super::test_pool;

const MIGRATION: &str = include_str!("../../../migrations/20260810000000_managed_devices.sql");

const DEV: &str = "00000000-0000-0000-0000-000000000001";
const ZONE: &str = "00000000-0000-0000-0000-000000000201";
const OTHER_ZONE: &str = "00000000-0000-0000-0000-0000000009f1";

/// Extract the backfill `UPDATE` from the migration file.
///
/// Splitting on `;` is safe here only because the statement contains no string
/// literal holding one — asserted below, so this stops working loudly rather
/// than silently testing half a predicate.
static BACKFILL: LazyLock<String> = LazyLock::new(|| {
    let stmt = MIGRATION
        .split(';')
        .map(|s| {
            // Drop comment lines so the `starts_with` test sees real SQL.
            s.lines()
                .filter(|l| !l.trim_start().starts_with("--"))
                .collect::<Vec<_>>()
                .join("\n")
        })
        .find(|s| s.trim_start().starts_with("UPDATE devices SET managed"))
        .expect("migration must contain the backfill UPDATE");
    assert!(
        stmt.contains("EXISTS"),
        "extracted statement looks truncated: {stmt}"
    );
    stmt
});

async fn seed_device(pool: &SqlitePool, name: Option<&str>) {
    sqlx::query(
        "INSERT INTO devices (id, mac, last_ip, name, device_type, first_seen, last_seen, zone_id) \
         VALUES (?, 'aa:bb:cc:dd:ee:01', '192.168.1.10', ?, 'unknown', \
                 '2026-03-07T00:00:00Z', '2026-03-07T00:00:00Z', ?)",
    )
    .bind(DEV)
    .bind(name)
    .bind(ZONE)
    .execute(pool)
    .await
    .unwrap();

    // Every migration has already run, so the device arrives pre-backfilled.
    // Reset to the pre-migration state these tests are about.
    sqlx::query("UPDATE devices SET managed = 0")
        .execute(pool)
        .await
        .unwrap();
}

async fn run_backfill(pool: &SqlitePool) -> bool {
    sqlx::query(&**BACKFILL).execute(pool).await.unwrap();
    sqlx::query_scalar::<_, i64>("SELECT managed FROM devices WHERE id = ?")
        .bind(DEV)
        .fetch_one(pool)
        .await
        .unwrap()
        == 1
}

/// Seed a device with no artefacts, apply `setup`, then report whether the
/// shipped backfill promotes it.
async fn promotes_after(setup: &'static str) -> bool {
    let pool = test_pool().await;
    seed_device(&pool, None).await;
    for stmt in setup.split(";@").filter(|s| !s.trim().is_empty()) {
        sqlx::query(stmt).execute(&pool).await.unwrap();
    }
    run_backfill(&pool).await
}

// ── Promotes: admin artefacts ────────────────────────────────────────────────

#[tokio::test]
async fn bare_device_is_not_promoted() {
    // The control. A device with no name and no artefacts is exactly the
    // population retention exists for; if this ever flips, the feature is off.
    let pool = test_pool().await;
    seed_device(&pool, None).await;
    assert!(!run_backfill(&pool).await);
}

#[tokio::test]
async fn a_name_promotes() {
    let pool = test_pool().await;
    seed_device(&pool, Some("Living room TV")).await;
    assert!(run_backfill(&pool).await);
}

#[tokio::test]
async fn admin_lock_promotes() {
    assert!(promotes_after("UPDATE devices SET admin_locked = 1").await);
}

#[tokio::test]
async fn dns_capture_promotes() {
    assert!(promotes_after("UPDATE devices SET dns_capture_enabled = 1").await);
}

#[tokio::test]
async fn an_admin_routing_rule_promotes() {
    assert!(
        promotes_after(
            "INSERT INTO routing_rules (id, device_id, target_json, created_by) \
             VALUES ('r1', '00000000-0000-0000-0000-000000000001', '{\"type\":\"direct\"}', 'admin')"
        )
        .await
    );
}

#[tokio::test]
async fn a_routing_profile_assignment_promotes() {
    assert!(
        promotes_after(
            "INSERT INTO routing_profiles (id, name) VALUES ('p1', 'streaming');@\
             INSERT INTO routing_device_profile (device_id, profile_id) \
             VALUES ('00000000-0000-0000-0000-000000000001', 'p1')"
        )
        .await
    );
}

#[tokio::test]
async fn dns_filter_device_settings_promote() {
    assert!(
        promotes_after(
            "INSERT INTO dns_filter_device_settings (device_id, enabled) \
             VALUES ('00000000-0000-0000-0000-000000000001', 0)"
        )
        .await
    );
}

#[tokio::test]
async fn a_dns_filter_profile_assignment_promotes() {
    assert!(
        promotes_after(
            "INSERT INTO dns_filter_profiles (id, name) VALUES ('fp1', 'kids');@\
             INSERT INTO dns_filter_device_profile (device_id, profile_id) \
             VALUES ('00000000-0000-0000-0000-000000000001', 'fp1')"
        )
        .await
    );
}

#[tokio::test]
async fn a_private_dns_grant_promotes() {
    // The case that most needs it: a roaming phone's grant would otherwise be
    // cascaded away 30 days after it last touched the LAN.
    assert!(
        promotes_after(
            "INSERT INTO private_dns_grants (id, device_id, token, created_at) \
             VALUES ('g1', '00000000-0000-0000-0000-000000000001', 'abc', '2026-03-07T00:00:00Z')"
        )
        .await
    );
}

#[tokio::test]
async fn an_inbound_wg_peer_promotes() {
    assert!(
        promotes_after(
            "INSERT INTO inbound_wg_peers (id, public_key, allowed_ip, name, created_at, device_id) \
             VALUES ('w1', 'pk', '10.100.64.2/32', 'phone', '2026-03-07T00:00:00Z', \
                     '00000000-0000-0000-0000-000000000001')"
        )
        .await
    );
}

#[tokio::test]
async fn a_dhcp_reservation_promotes_by_mac() {
    // Reservations are MAC-keyed, not device_id-keyed — the one artefact that
    // would otherwise outlive the device row entirely.
    assert!(
        promotes_after(
            "INSERT INTO dhcp_reservations (id, mac_address, ip_address) \
             VALUES ('res1', 'aa:bb:cc:dd:ee:01', '192.168.1.10')"
        )
        .await
    );
}

#[tokio::test]
async fn a_zone_exception_promotes_from_either_side() {
    let from = "INSERT INTO zone_exceptions (id, from_kind, from_id, to_kind, to_id) \
         VALUES ('e1', 'device', '00000000-0000-0000-0000-000000000001', 'zone', 'z9')";
    let to = "INSERT INTO zone_exceptions (id, from_kind, from_id, to_kind, to_id) \
         VALUES ('e2', 'zone', 'z9', 'device', '00000000-0000-0000-0000-000000000001')";
    assert!(promotes_after(from).await, "device on the `from` side");
    assert!(promotes_after(to).await, "device on the `to` side");
}

// ── Does NOT promote: self-service and machine-derived ───────────────────────

#[tokio::test]
async fn a_user_routing_rule_does_not_promote() {
    // The load-bearing exclusion. A guest tapping a toggle in the user PWA is
    // the device asking, not the admin deciding — promoting here would make
    // every guest phone permanently exempt from retention, which is the exact
    // population issue #1181 is about.
    assert!(
        !promotes_after(
            "INSERT INTO routing_rules (id, device_id, target_json, created_by) \
             VALUES ('r1', '00000000-0000-0000-0000-000000000001', '{\"type\":\"direct\"}', 'user')"
        )
        .await
    );
}

#[tokio::test]
async fn a_device_access_request_does_not_promote() {
    assert!(
        !promotes_after(
            "INSERT INTO device_access_requests (id, device_id, kind, domain, created_at) \
             VALUES ('rr1', '00000000-0000-0000-0000-000000000001', 'block', 'ads.example', \
                     '2026-03-07T00:00:00Z')"
        )
        .await
    );
}

/// The device *asking* for Private DNS must not promote it — only the admin's
/// approval does, by way of `grant_device`. CONTEXT.md's managed-device rule:
/// "a device's own self-service acts never promote it — that is the device
/// asking, not the admin deciding."
#[tokio::test]
async fn a_private_dns_access_request_does_not_promote() {
    assert!(
        !promotes_after(
            "INSERT INTO device_access_requests (id, device_id, kind, created_at) \
             VALUES ('rr2', '00000000-0000-0000-0000-000000000001', 'private_dns', \
                     '2026-08-14T00:00:00Z')"
        )
        .await
    );
}

#[tokio::test]
async fn a_push_subscription_does_not_promote() {
    // Deliberately left dangling by design — see the push_subscriptions
    // migration: a forgotten-then-rediscovered device keeps its MAC and its
    // subscription stays valid; stale rows are pruned on 404/410 instead.
    assert!(
        !promotes_after(
            "INSERT INTO push_subscriptions (id, owner_kind, owner_key, endpoint, p256dh, auth, created_at) \
             VALUES ('s1', 'device', 'aa:bb:cc:dd:ee:01', 'https://push.example/1', 'k', 'a', \
                     '2026-03-07T00:00:00Z')"
        )
        .await
    );
}

#[tokio::test]
async fn a_non_default_zone_does_not_promote() {
    // The deliberate backfill gap (ADR 0032). Zone membership is sticky, so
    // `zone_id != <default-for-new>` cannot distinguish "an admin moved this
    // device" from "the default-for-new flag was later re-pointed". Promoting
    // on it would mark essentially every device on such a box managed and
    // silently disable retention entirely.
    let pool = test_pool().await;
    seed_device(&pool, None).await;
    sqlx::query("INSERT INTO network_zones (id, name, is_default, is_default_for_new) VALUES (?, 'Guest-backfill-test', 0, 0)")
        .bind(OTHER_ZONE)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE devices SET zone_id = ? WHERE id = ?")
        .bind(OTHER_ZONE)
        .bind(DEV)
        .execute(&pool)
        .await
        .unwrap();

    assert!(!run_backfill(&pool).await);
}

#[tokio::test]
async fn the_backfill_is_idempotent() {
    // It is a pure `UPDATE ... WHERE`, so re-running it must never demote a
    // device it already promoted — which is what lets these tests re-run it
    // against an already-migrated pool at all.
    let pool = test_pool().await;
    seed_device(&pool, Some("Living room TV")).await;
    assert!(run_backfill(&pool).await);
    assert!(run_backfill(&pool).await);
}
