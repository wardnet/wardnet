//! Tests for [`SqliteDnsFilterRepository`] against a fresh in-memory DB.
//!
//! The migration seeds three builtin profiles plus four curated blocklist
//! URLs (disabled). These tests exercise CRUD, profile scoping, FK cascade
//! on device delete, and the device/ip join used by the hot-path runner.

use uuid::Uuid;

use super::test_pool;
use crate::repository::SqliteDnsFilterRepository;
use crate::repository::dns_filter::{
    AllowlistRow, BlocklistRow, BlocklistUpdate, CustomRuleRow, CustomRuleUpdate,
    DeviceSettingsRow, DnsFilterRepository,
};
use wardnet_common::dns_filter::DnsFilterConfig;

const AD_BLOCKING: &str = "00000000-0000-0000-0000-000000000100";
const PARENTAL: &str = "00000000-0000-0000-0000-000000000101";
const MALWARE: &str = "00000000-0000-0000-0000-000000000102";

async fn insert_device(pool: &sqlx::SqlitePool, id: &str, ip: &str) {
    sqlx::query(
        "INSERT INTO devices (id, mac, last_ip, device_type, first_seen, last_seen, zone_id) \
         VALUES (?, ?, ?, 'unknown', ?, ?, '00000000-0000-0000-0000-000000000201')",
    )
    .bind(id)
    .bind(format!(
        "aa:bb:cc:dd:ee:{:02x}",
        u32::from(id.as_bytes()[35]) & 0xff
    ))
    .bind(ip)
    .bind("2026-05-06T00:00:00Z")
    .bind("2026-05-06T00:00:00Z")
    .execute(pool)
    .await
    .unwrap();
}

// ── Migration / seed ──────────────────────────────────────────────────────

#[tokio::test]
async fn migration_seeds_three_builtin_profiles() {
    let pool = test_pool().await;
    let repo = SqliteDnsFilterRepository::new(pool);

    let profiles = repo.list_profiles().await.unwrap();
    assert_eq!(profiles.len(), 3);
    assert!(profiles.iter().all(|p| p.builtin));

    let names: Vec<&str> = profiles.iter().map(|p| p.name.as_str()).collect();
    assert!(names.contains(&"Ad Blocking"));
    assert!(names.contains(&"Parental Controls"));
    assert!(names.contains(&"Malware & Phishing"));
}

#[tokio::test]
async fn migration_backfills_existing_blocklists_into_ad_blocking() {
    let pool = test_pool().await;
    let repo = SqliteDnsFilterRepository::new(pool);

    let ad_blocking: Uuid = AD_BLOCKING.parse().unwrap();
    let lists = repo.list_blocklists(ad_blocking).await.unwrap();
    // Two seed entries from the original DNS migration.
    let names: Vec<&str> = lists.iter().map(|b| b.name.as_str()).collect();
    assert!(names.contains(&"Steven Black Unified"));
    assert!(names.contains(&"OISD Basic"));
}

#[tokio::test]
async fn migration_seeds_curated_urls_in_other_profiles_disabled() {
    let pool = test_pool().await;
    let repo = SqliteDnsFilterRepository::new(pool);

    let parental: Uuid = PARENTAL.parse().unwrap();
    let malware: Uuid = MALWARE.parse().unwrap();

    let p_lists = repo.list_blocklists(parental).await.unwrap();
    assert_eq!(p_lists.len(), 2);
    assert!(p_lists.iter().all(|b| !b.enabled));

    let m_lists = repo.list_blocklists(malware).await.unwrap();
    assert_eq!(m_lists.len(), 2);
    assert!(m_lists.iter().all(|b| !b.enabled));
}

#[tokio::test]
async fn migration_sets_default_profile_to_ad_blocking() {
    let pool = test_pool().await;
    let repo = SqliteDnsFilterRepository::new(pool);

    let cfg = repo.get_dns_filter_config().await.unwrap();
    assert!(cfg.enabled, "kill switch on by default");
    assert_eq!(cfg.default_profile_ids.len(), 1);
    assert_eq!(
        cfg.default_profile_ids[0].to_string().to_lowercase(),
        AD_BLOCKING
    );
}

// ── Profile CRUD ──────────────────────────────────────────────────────────

#[tokio::test]
async fn create_and_update_user_profile() {
    let pool = test_pool().await;
    let repo = SqliteDnsFilterRepository::new(pool);

    let id = Uuid::new_v4();
    let p = repo.create_profile(id, "Kids", None).await.unwrap();
    assert_eq!(p.id, id);
    assert!(!p.builtin);
    assert!(p.description.is_none());

    // Rename only.
    assert!(
        repo.update_profile_fields(id, Some("Family"), None)
            .await
            .unwrap()
    );
    let fetched = repo.get_profile(id).await.unwrap().unwrap();
    assert_eq!(fetched.name, "Family");
    assert!(fetched.description.is_none());

    // Set description.
    assert!(
        repo.update_profile_fields(id, None, Some(Some("A family profile")))
            .await
            .unwrap()
    );
    let fetched = repo.get_profile(id).await.unwrap().unwrap();
    assert_eq!(fetched.description.as_deref(), Some("A family profile"));

    // Clear description.
    assert!(
        repo.update_profile_fields(id, None, Some(None))
            .await
            .unwrap()
    );
    let fetched = repo.get_profile(id).await.unwrap().unwrap();
    assert!(fetched.description.is_none());
}

#[tokio::test]
async fn delete_profile_refuses_builtin_returns_false() {
    let pool = test_pool().await;
    let repo = SqliteDnsFilterRepository::new(pool);

    let ad_blocking: Uuid = AD_BLOCKING.parse().unwrap();
    let deleted = repo.delete_profile(ad_blocking).await.unwrap();
    assert!(!deleted);

    // Profile still exists.
    assert!(repo.get_profile(ad_blocking).await.unwrap().is_some());
}

#[tokio::test]
async fn delete_user_profile_succeeds() {
    let pool = test_pool().await;
    let repo = SqliteDnsFilterRepository::new(pool);

    let id = Uuid::new_v4();
    repo.create_profile(id, "Temp", None).await.unwrap();

    assert!(repo.delete_profile(id).await.unwrap());
    assert!(repo.get_profile(id).await.unwrap().is_none());
}

// ── Profile-scoped CRUD ───────────────────────────────────────────────────

#[tokio::test]
async fn blocklists_are_scoped_to_their_profile() {
    let pool = test_pool().await;
    let repo = SqliteDnsFilterRepository::new(pool);

    let custom = Uuid::new_v4();
    repo.create_profile(custom, "Custom", None).await.unwrap();

    let bl_id = Uuid::new_v4();
    repo.create_blocklist(&BlocklistRow {
        id: bl_id.to_string(),
        profile_id: custom.to_string(),
        name: "Custom list".to_owned(),
        url: "https://example.com/list.txt".to_owned(),
        enabled: true,
        cron_schedule: "0 3 * * *".to_owned(),
    })
    .await
    .unwrap();

    let custom_lists = repo.list_blocklists(custom).await.unwrap();
    assert_eq!(custom_lists.len(), 1);
    assert_eq!(custom_lists[0].id, bl_id);

    // The Ad Blocking profile still has its own seeded entries — none from
    // the new custom profile.
    let ad_blocking: Uuid = AD_BLOCKING.parse().unwrap();
    let ab_lists = repo.list_blocklists(ad_blocking).await.unwrap();
    assert!(ab_lists.iter().all(|b| b.id != bl_id));
}

#[tokio::test]
async fn update_blocklist_flips_enabled() {
    let pool = test_pool().await;
    let repo = SqliteDnsFilterRepository::new(pool);
    let ad_blocking: Uuid = AD_BLOCKING.parse().unwrap();

    let lists = repo.list_blocklists(ad_blocking).await.unwrap();
    let first = lists[0].clone();
    assert!(!first.enabled);

    repo.update_blocklist(
        first.id,
        &BlocklistUpdate {
            enabled: Some(true),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let after = repo.get_blocklist(first.id).await.unwrap().unwrap();
    assert!(after.enabled);
}

#[tokio::test]
async fn allowlist_unique_per_profile_not_global() {
    let pool = test_pool().await;
    let repo = SqliteDnsFilterRepository::new(pool);
    let ad_blocking: Uuid = AD_BLOCKING.parse().unwrap();
    let parental: Uuid = PARENTAL.parse().unwrap();

    repo.create_allowlist_entry(&AllowlistRow {
        id: Uuid::new_v4().to_string(),
        profile_id: ad_blocking.to_string(),
        domain: "shared.example".to_owned(),
        reason: None,
    })
    .await
    .unwrap();

    // Same domain in a different profile must succeed.
    repo.create_allowlist_entry(&AllowlistRow {
        id: Uuid::new_v4().to_string(),
        profile_id: parental.to_string(),
        domain: "shared.example".to_owned(),
        reason: None,
    })
    .await
    .unwrap();

    let ab = repo.list_allowlist(ad_blocking).await.unwrap();
    let pc = repo.list_allowlist(parental).await.unwrap();
    assert_eq!(ab.len(), 1);
    assert_eq!(pc.len(), 1);
}

#[tokio::test]
async fn custom_rule_round_trip_in_profile() {
    let pool = test_pool().await;
    let repo = SqliteDnsFilterRepository::new(pool);
    let ad_blocking: Uuid = AD_BLOCKING.parse().unwrap();

    let id = Uuid::new_v4();
    repo.create_custom_rule(&CustomRuleRow {
        id: id.to_string(),
        profile_id: ad_blocking.to_string(),
        rule_text: "||tracker.example^".to_owned(),
        enabled: true,
        comment: Some("seeded".to_owned()),
    })
    .await
    .unwrap();

    let rule = repo.get_custom_rule(id).await.unwrap().unwrap();
    assert_eq!(rule.profile_id, ad_blocking);
    assert!(rule.enabled);

    repo.update_custom_rule(
        id,
        &CustomRuleUpdate {
            enabled: Some(false),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let after = repo.get_custom_rule(id).await.unwrap().unwrap();
    assert!(!after.enabled);
}

#[tokio::test]
async fn load_filter_inputs_for_profile_returns_only_enabled() {
    let pool = test_pool().await;
    let repo = SqliteDnsFilterRepository::new(pool);
    let ad_blocking: Uuid = AD_BLOCKING.parse().unwrap();

    // Enable one of the seeded blocklists, then back-fill its domains.
    let lists = repo.list_blocklists(ad_blocking).await.unwrap();
    let bl = lists[0].clone();
    repo.update_blocklist(
        bl.id,
        &BlocklistUpdate {
            enabled: Some(true),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    repo.replace_blocklist_domains(bl.id, &["ads.example".to_owned()])
        .await
        .unwrap();

    repo.create_allowlist_entry(&AllowlistRow {
        id: Uuid::new_v4().to_string(),
        profile_id: ad_blocking.to_string(),
        domain: "allowed.example".to_owned(),
        reason: None,
    })
    .await
    .unwrap();

    repo.create_custom_rule(&CustomRuleRow {
        id: Uuid::new_v4().to_string(),
        profile_id: ad_blocking.to_string(),
        rule_text: "||disabled.example^".to_owned(),
        enabled: false,
        comment: None,
    })
    .await
    .unwrap();
    repo.create_custom_rule(&CustomRuleRow {
        id: Uuid::new_v4().to_string(),
        profile_id: ad_blocking.to_string(),
        rule_text: "||enabled.example^".to_owned(),
        enabled: true,
        comment: None,
    })
    .await
    .unwrap();

    let inputs = repo
        .load_filter_inputs_for_profile(ad_blocking)
        .await
        .unwrap();

    // Blocked domains are NOT part of a profile's inputs: each blocklist is
    // compiled into its own filter and the runtime profile references it, so
    // the profile query must not drag millions of domains along. The domain is
    // still reachable through the path the rebuild actually uses.
    let blocked = repo.load_blocklist_domains(bl.id).await.unwrap();
    assert!(blocked.iter().any(|d| d == "ads.example"));

    assert!(inputs.allowlist.iter().any(|d| d == "allowed.example"));
    assert!(
        inputs
            .custom_rules
            .iter()
            .any(|r| r.contains("enabled.example"))
    );
    assert!(
        inputs
            .custom_rules
            .iter()
            .all(|r| !r.contains("disabled.example")),
        "disabled rule must not be returned"
    );
}

// ── Per-device settings ───────────────────────────────────────────────────

#[tokio::test]
async fn find_device_settings_returns_none_for_unknown() {
    let pool = test_pool().await;
    let repo = SqliteDnsFilterRepository::new(pool);
    let id = Uuid::new_v4();
    assert!(repo.find_device_settings(id).await.unwrap().is_none());
}

#[tokio::test]
async fn upsert_device_settings_then_find() {
    let pool = test_pool().await;
    let dev_id = "00000000-0000-0000-0000-000000000aaa";
    insert_device(&pool, dev_id, "192.168.1.10").await;

    let repo = SqliteDnsFilterRepository::new(pool);
    let dev: Uuid = dev_id.parse().unwrap();

    repo.upsert_device_settings(&DeviceSettingsRow {
        device_id: dev.to_string(),
        enabled: false,
    })
    .await
    .unwrap();

    let s = repo.find_device_settings(dev).await.unwrap().unwrap();
    assert_eq!(s.device_id, dev);
    assert!(!s.enabled);
    assert!(s.profile_ids.is_empty());

    // Upsert again (idempotent).
    repo.upsert_device_settings(&DeviceSettingsRow {
        device_id: dev.to_string(),
        enabled: true,
    })
    .await
    .unwrap();
    let s2 = repo.find_device_settings(dev).await.unwrap().unwrap();
    assert!(s2.enabled);
}

#[tokio::test]
async fn set_device_profiles_replaces_atomically() {
    let pool = test_pool().await;
    let dev_id = "00000000-0000-0000-0000-000000000bbb";
    insert_device(&pool, dev_id, "192.168.1.20").await;
    let repo = SqliteDnsFilterRepository::new(pool);
    let dev: Uuid = dev_id.parse().unwrap();

    let ad_blocking: Uuid = AD_BLOCKING.parse().unwrap();
    let parental: Uuid = PARENTAL.parse().unwrap();
    let malware: Uuid = MALWARE.parse().unwrap();

    repo.set_device_profiles(dev, &[ad_blocking, parental])
        .await
        .unwrap();
    let s = repo.find_device_settings(dev).await.unwrap().unwrap();
    assert_eq!(s.profile_ids.len(), 2);
    assert!(s.profile_ids.contains(&ad_blocking));
    assert!(s.profile_ids.contains(&parental));

    // Replace — old set is gone.
    repo.set_device_profiles(dev, &[malware]).await.unwrap();
    let s2 = repo.find_device_settings(dev).await.unwrap().unwrap();
    assert_eq!(s2.profile_ids, vec![malware]);

    // Empty replace clears.
    repo.set_device_profiles(dev, &[]).await.unwrap();
    let s3 = repo.find_device_settings(dev).await.unwrap();
    // The device still has a settings row from earlier upserts (none here),
    // so the assignment-only state collapses to None.
    assert!(s3.is_none());
}

#[tokio::test]
async fn fk_cascade_on_device_delete() {
    let pool = test_pool().await;
    let dev_id = "00000000-0000-0000-0000-000000000ccc";
    insert_device(&pool, dev_id, "192.168.1.30").await;

    let repo = SqliteDnsFilterRepository::new(pool.clone());
    let dev: Uuid = dev_id.parse().unwrap();
    let ad_blocking: Uuid = AD_BLOCKING.parse().unwrap();

    repo.upsert_device_settings(&DeviceSettingsRow {
        device_id: dev.to_string(),
        enabled: false,
    })
    .await
    .unwrap();
    repo.set_device_profiles(dev, &[ad_blocking]).await.unwrap();

    // Foreign-key constraints are enabled on the connection — the migration
    // assumes ON DELETE CASCADE on both child tables.
    sqlx::query("PRAGMA foreign_keys = ON")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM devices WHERE id = ?")
        .bind(dev_id)
        .execute(&pool)
        .await
        .unwrap();

    assert!(repo.find_device_settings(dev).await.unwrap().is_none());
}

#[tokio::test]
async fn list_device_settings_with_ips_joins_last_ip() {
    let pool = test_pool().await;
    let dev_id = "00000000-0000-0000-0000-000000000ddd";
    insert_device(&pool, dev_id, "192.168.1.50").await;
    let repo = SqliteDnsFilterRepository::new(pool);
    let dev: Uuid = dev_id.parse().unwrap();

    repo.upsert_device_settings(&DeviceSettingsRow {
        device_id: dev.to_string(),
        enabled: true,
    })
    .await
    .unwrap();

    let rows = repo.list_device_settings_with_ips(false).await.unwrap();
    let row = rows.iter().find(|r| r.settings.device_id == dev).unwrap();
    assert_eq!(row.ip.as_deref(), Some("192.168.1.50"));
}

#[tokio::test]
async fn list_device_settings_filters_disabled_when_requested() {
    let pool = test_pool().await;
    let dev_a = "00000000-0000-0000-0000-000000000eee";
    let dev_b = "00000000-0000-0000-0000-000000000eef";
    insert_device(&pool, dev_a, "192.168.1.60").await;
    insert_device(&pool, dev_b, "192.168.1.61").await;
    let repo = SqliteDnsFilterRepository::new(pool);

    repo.upsert_device_settings(&DeviceSettingsRow {
        device_id: dev_a.to_owned(),
        enabled: true,
    })
    .await
    .unwrap();
    repo.upsert_device_settings(&DeviceSettingsRow {
        device_id: dev_b.to_owned(),
        enabled: false,
    })
    .await
    .unwrap();

    let all = repo.list_device_settings_with_ips(false).await.unwrap();
    assert_eq!(all.len(), 2);

    let only_active = repo.list_device_settings_with_ips(true).await.unwrap();
    assert_eq!(only_active.len(), 1);
    assert!(only_active[0].settings.enabled);
}

// ── Global config ─────────────────────────────────────────────────────────

#[tokio::test]
async fn set_dns_filter_config_round_trip() {
    let pool = test_pool().await;
    let repo = SqliteDnsFilterRepository::new(pool);

    let a = Uuid::new_v4();
    let b = Uuid::new_v4();
    repo.create_profile(a, "First", None).await.unwrap();
    repo.create_profile(b, "Second", None).await.unwrap();

    repo.set_dns_filter_config(&DnsFilterConfig {
        enabled: false,
        default_profile_ids: vec![a, b],
    })
    .await
    .unwrap();

    let cfg = repo.get_dns_filter_config().await.unwrap();
    assert!(!cfg.enabled);
    let mut got = cfg.default_profile_ids.clone();
    let mut want = vec![a, b];
    got.sort();
    want.sort();
    assert_eq!(got, want);
}

#[tokio::test]
async fn set_dns_filter_config_replaces_default_set() {
    let pool = test_pool().await;
    let repo = SqliteDnsFilterRepository::new(pool);

    let a = Uuid::new_v4();
    let b = Uuid::new_v4();
    repo.create_profile(a, "A", None).await.unwrap();
    repo.create_profile(b, "B", None).await.unwrap();

    // Seed a multi-profile default, then replace it with a single id.
    repo.set_dns_filter_config(&DnsFilterConfig {
        enabled: true,
        default_profile_ids: vec![a, b],
    })
    .await
    .unwrap();

    repo.set_dns_filter_config(&DnsFilterConfig {
        enabled: true,
        default_profile_ids: vec![b],
    })
    .await
    .unwrap();

    let cfg = repo.get_dns_filter_config().await.unwrap();
    assert_eq!(cfg.default_profile_ids, vec![b]);
}

#[tokio::test]
async fn set_dns_filter_config_clears_default_with_empty_vec() {
    let pool = test_pool().await;
    let repo = SqliteDnsFilterRepository::new(pool);

    repo.set_dns_filter_config(&DnsFilterConfig {
        enabled: true,
        default_profile_ids: Vec::new(),
    })
    .await
    .unwrap();

    let cfg = repo.get_dns_filter_config().await.unwrap();
    assert!(cfg.default_profile_ids.is_empty());
}

#[tokio::test]
async fn deleting_profile_cascades_default_membership() {
    let pool = test_pool().await;
    let repo = SqliteDnsFilterRepository::new(pool);

    let p = Uuid::new_v4();
    repo.create_profile(p, "Doomed", None).await.unwrap();

    repo.set_dns_filter_config(&DnsFilterConfig {
        enabled: true,
        default_profile_ids: vec![p],
    })
    .await
    .unwrap();

    repo.delete_profile(p).await.unwrap();

    let cfg = repo.get_dns_filter_config().await.unwrap();
    assert!(
        !cfg.default_profile_ids.contains(&p),
        "ON DELETE CASCADE should drop default-membership"
    );
}

// ── Bulk-import concurrency (blocklist refresh write-starvation) ─────────

/// Build pools that mirror production: an on-disk WAL database whose write
/// pool has exactly one connection. The single writer is the whole point —
/// it is what a long bulk import can monopolise.
async fn on_disk_pools(acquire_timeout: std::time::Duration) -> (crate::db::DbPools, String) {
    use sqlx::sqlite::{
        SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous,
    };

    let path = std::env::temp_dir().join(format!("wardnet_bulk_{}.db", Uuid::new_v4().simple()));
    let path_str = path.to_string_lossy().into_owned();

    let make_options = || {
        SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal)
            .busy_timeout(std::time::Duration::from_secs(30))
            .foreign_keys(true)
    };

    let write = SqlitePoolOptions::new()
        .max_connections(1)
        .min_connections(1)
        .acquire_timeout(acquire_timeout)
        .connect_with(make_options())
        .await
        .unwrap();
    let read = SqlitePoolOptions::new()
        .max_connections(5)
        .acquire_timeout(acquire_timeout)
        .connect_with(make_options())
        .await
        .unwrap();

    sqlx::migrate!("./migrations").run(&write).await.unwrap();

    (
        crate::db::DbPools {
            bulk_read: read.clone(),
            read,
            write,
        },
        path_str,
    )
}

/// Regression guard: importing a large blocklist must not hold the daemon's
/// single write connection for the entire import.
///
/// The malware/threat-intel feeds are ~1.9M domains. When `replace_blocklist_domains`
/// wrote them in one transaction it kept the sole writer busy for minutes, so every
/// other writer in the daemon (stats, DHCP leases, DNS query log, heartbeat) failed
/// with "pool timed out while waiting for an open connection". Health then flipped
/// DOWN, the soft watchdog withheld `sd_notify(WATCHDOG=1)`, and systemd SIGABRT'd
/// the daemon mid-import — leaving the UI polling a job id that no longer existed.
///
/// The invariant: an unrelated write must be able to get through *while* the import
/// is still running.
#[tokio::test]
async fn bulk_import_does_not_starve_concurrent_writers() {
    // A probe that cannot acquire the writer within this budget means the import
    // held it continuously for that long — the starvation the daemon died of.
    // CI-tolerant budget: the regression this guards held the writer for
    // MINUTES (a whole 1.9M-domain import/delete in one transaction), so 5s
    // keeps the signal decisive while a loaded shared runner — which has
    // twice failed a 1s budget by single-digit milliseconds, costing a
    // release run and a PR run — gets real headroom.
    const MAX_HOLD: std::time::Duration = std::time::Duration::from_secs(5);

    let (pools, path) = on_disk_pools(MAX_HOLD).await;
    let repo = std::sync::Arc::new(SqliteDnsFilterRepository::new_pools(pools.clone()));

    let malware: Uuid = MALWARE.parse().unwrap();
    let lists = repo.list_blocklists(malware).await.unwrap();
    let target = lists[0].id;
    let other = lists[1].id;

    // Big enough that a single-transaction import takes well over the 200ms
    // acquire timeout, so a starved writer cannot simply wait it out.
    let domains: Vec<String> = (0..400_000)
        .map(|i| format!("d{i}.malware.example"))
        .collect();

    let importer = std::sync::Arc::clone(&repo);
    let import =
        tokio::spawn(async move { importer.replace_blocklist_domains(target, &domains).await });

    // Wait until the import actually holds the single write connection.
    // Without this the probe below can grab the still-idle connection before
    // the spawned task is ever polled and pass for the wrong reason.
    while pools.write.num_idle() > 0 && !import.is_finished() {
        tokio::task::yield_now().await;
    }
    assert!(
        !import.is_finished(),
        "import finished before it could be observed holding the writer; raise the domain count"
    );

    // Probe an unrelated small write repeatedly for as long as the import runs.
    // A probe only fails if the writer stayed unavailable for the whole MAX_HOLD
    // window, so a failure here is unambiguous starvation — no tail race with the
    // import's final commit (that would have woken the waiter and succeeded).
    let mut starved_for = None;
    while !import.is_finished() {
        let probe = std::time::Instant::now();
        if repo
            .set_blocklist_error(other, Some("probe"))
            .await
            .is_err()
        {
            starved_for = Some(probe.elapsed());
            break;
        }
        tokio::task::yield_now().await;
    }

    assert!(
        starved_for.is_none(),
        "an unrelated write could not acquire the write connection for {:?} while the bulk \
         import ran — the import monopolises the single writer for its whole transaction, \
         which is what starves stats/DHCP/heartbeat/health and trips the systemd watchdog",
        starved_for.unwrap()
    );

    let count = import.await.unwrap().unwrap();
    assert_eq!(count, 400_000);

    let _ = std::fs::remove_file(&path);
}

/// The import must stay atomic from a reader's point of view: while a refresh is
/// in flight, readers keep seeing the previous list — never a half-loaded one,
/// never an empty one.
#[tokio::test]
async fn bulk_import_is_atomic_for_readers() {
    let (pools, path) = on_disk_pools(std::time::Duration::from_secs(30)).await;
    let repo = std::sync::Arc::new(SqliteDnsFilterRepository::new_pools(pools.clone()));

    let malware: Uuid = MALWARE.parse().unwrap();
    let lists = repo.list_blocklists(malware).await.unwrap();
    let target = lists[0].id;

    // Seed a known "previous" generation.
    repo.replace_blocklist_domains(target, &["old.example".to_owned()])
        .await
        .unwrap();

    let domains: Vec<String> = (0..200_000).map(|i| format!("new{i}.example")).collect();
    let importer = std::sync::Arc::clone(&repo);
    let import =
        tokio::spawn(async move { importer.replace_blocklist_domains(target, &domains).await });

    // Every observation must be either the whole old list or the whole new one —
    // never a partially-loaded blocklist. (A reader can legitimately land after
    // the flip, so "still the old list" is not assertable; "never partial" is.)
    while !import.is_finished() {
        let seen = repo.load_blocklist_domains(target).await.unwrap();
        assert!(
            seen == vec!["old.example".to_owned()] || seen.len() == 200_000,
            "reader observed a partially-applied import ({} domains); the swap must be atomic",
            seen.len()
        );
        tokio::task::yield_now().await;
    }
    import.await.unwrap().unwrap();

    let seen = repo.load_blocklist_domains(target).await.unwrap();
    assert_eq!(
        seen.len(),
        200_000,
        "after the flip readers see the new list"
    );

    let _ = std::fs::remove_file(&path);
}

/// Regression guard: deleting a blocklist must not starve the writer either.
///
/// `dns_filter_blocked_domains.blocklist_id` is `ON DELETE CASCADE`, so removing
/// a blocklist row made `SQLite` drop every one of its domains inside a single
/// implicit transaction — millions of rows for a threat-intel feed. That holds
/// the sole write connection exactly as the old single-transaction import did,
/// with the same watchdog-kill ending. Deleting a profile cascades through its
/// blocklists into the same place.
#[tokio::test]
async fn deleting_a_blocklist_does_not_starve_concurrent_writers() {
    // CI-tolerant budget: the regression this guards held the writer for
    // MINUTES (a whole 1.9M-domain import/delete in one transaction), so 5s
    // keeps the signal decisive while a loaded shared runner — which has
    // twice failed a 1s budget by single-digit milliseconds, costing a
    // release run and a PR run — gets real headroom.
    const MAX_HOLD: std::time::Duration = std::time::Duration::from_secs(5);

    let (pools, path) = on_disk_pools(MAX_HOLD).await;
    let repo = std::sync::Arc::new(SqliteDnsFilterRepository::new_pools(pools.clone()));

    let malware: Uuid = MALWARE.parse().unwrap();
    let lists = repo.list_blocklists(malware).await.unwrap();
    let target = lists[0].id;
    let other = lists[1].id;

    let domains: Vec<String> = (0..400_000)
        .map(|i| format!("d{i}.malware.example"))
        .collect();
    repo.replace_blocklist_domains(target, &domains)
        .await
        .unwrap();

    let deleter = std::sync::Arc::clone(&repo);
    let delete = tokio::spawn(async move { deleter.delete_blocklist(target).await });

    while pools.write.num_idle() > 0 && !delete.is_finished() {
        tokio::task::yield_now().await;
    }

    let mut starved_for = None;
    while !delete.is_finished() {
        let probe = std::time::Instant::now();
        if repo
            .set_blocklist_error(other, Some("probe"))
            .await
            .is_err()
        {
            starved_for = Some(probe.elapsed());
            break;
        }
        tokio::task::yield_now().await;
    }

    assert!(
        starved_for.is_none(),
        "an unrelated write could not acquire the write connection for {:?} while a blocklist \
         was being deleted — the ON DELETE CASCADE drops every domain in one transaction",
        starved_for.unwrap()
    );

    assert!(delete.await.unwrap().unwrap());
    assert!(repo.get_blocklist(target).await.unwrap().is_none());
    assert!(
        repo.load_blocklist_domains(target)
            .await
            .unwrap()
            .is_empty(),
        "the deleted blocklist's domains must be gone"
    );

    let _ = std::fs::remove_file(&path);
}

/// A refresh re-imports a list whose domains mostly did not change. The new
/// generation is written while the old one is still present, so the two
/// generations must be able to hold the same domain for the same blocklist at
/// once — otherwise `PRIMARY KEY (domain, blocklist_id)` rejects the insert and
/// every real refresh fails.
#[tokio::test]
async fn reimporting_overlapping_domains_succeeds() {
    let pool = test_pool().await;
    let repo = SqliteDnsFilterRepository::new(pool);

    let malware: Uuid = MALWARE.parse().unwrap();
    let target = repo.list_blocklists(malware).await.unwrap()[0].id;

    let first = vec!["ads.example".to_owned(), "evil.example".to_owned()];
    repo.replace_blocklist_domains(target, &first)
        .await
        .unwrap();

    // Same list again, plus one new domain — exactly what a scheduled refresh does.
    let second = vec![
        "ads.example".to_owned(),
        "evil.example".to_owned(),
        "new.example".to_owned(),
    ];
    let count = repo
        .replace_blocklist_domains(target, &second)
        .await
        .expect("re-importing an overlapping list must not violate the primary key");
    assert_eq!(count, 3);

    let mut seen = repo.load_blocklist_domains(target).await.unwrap();
    seen.sort();
    assert_eq!(seen, ["ads.example", "evil.example", "new.example"]);
}

/// A daemon killed between the flip and the cleanup orphans a generation that
/// the superseded-generation sweep would never look at again. The next import
/// must reclaim it, or those rows leak for the life of the database.
#[tokio::test]
async fn import_reclaims_generations_orphaned_by_a_crash() {
    let pool = test_pool().await;
    let repo = SqliteDnsFilterRepository::new(pool.clone());

    let malware: Uuid = MALWARE.parse().unwrap();
    let target = repo.list_blocklists(malware).await.unwrap()[0].id;

    repo.replace_blocklist_domains(target, &["a.example".to_owned()])
        .await
        .unwrap();

    // Simulate the crash: rows stranded at a generation nothing points at
    // (a flip happened, its cleanup never ran).
    sqlx::query(
        "INSERT INTO dns_filter_blocked_domains (domain, blocklist_id, generation) \
         VALUES ('orphan.example', ?, 99)",
    )
    .bind(target.to_string())
    .execute(&pool)
    .await
    .unwrap();

    repo.replace_blocklist_domains(target, &["b.example".to_owned()])
        .await
        .unwrap();

    let total: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM dns_filter_blocked_domains WHERE blocklist_id = ?",
    )
    .bind(target.to_string())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        total, 1,
        "only the active generation should remain; the orphaned generation must be reclaimed"
    );
    assert_eq!(
        repo.load_blocklist_domains(target).await.unwrap(),
        vec!["b.example".to_owned()]
    );
}

/// `entry_count` must reflect rows actually stored, and a duplicate in the
/// caller's slice must not abort the import on the composite primary key.
#[tokio::test]
async fn import_dedupes_and_counts_stored_rows() {
    let pool = test_pool().await;
    let repo = SqliteDnsFilterRepository::new(pool);

    let malware: Uuid = MALWARE.parse().unwrap();
    let target = repo.list_blocklists(malware).await.unwrap()[0].id;

    let count = repo
        .replace_blocklist_domains(
            target,
            &[
                "dup.example".to_owned(),
                "dup.example".to_owned(),
                "other.example".to_owned(),
            ],
        )
        .await
        .expect("a duplicated domain must not abort the import");

    assert_eq!(
        count, 2,
        "entry_count must be the rows stored, not the input length"
    );
    assert_eq!(repo.load_blocklist_domains(target).await.unwrap().len(), 2);
}
