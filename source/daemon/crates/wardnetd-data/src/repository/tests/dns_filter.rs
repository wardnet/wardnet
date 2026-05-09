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
        "INSERT INTO devices (id, mac, last_ip, device_type, first_seen, last_seen) \
         VALUES (?, ?, ?, 'unknown', ?, ?)",
    )
    .bind(id)
    .bind(format!(
        "AA:BB:CC:DD:EE:{:02x}",
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
async fn create_and_rename_user_profile() {
    let pool = test_pool().await;
    let repo = SqliteDnsFilterRepository::new(pool);

    let id = Uuid::new_v4();
    let p = repo.create_profile(id, "Kids").await.unwrap();
    assert_eq!(p.id, id);
    assert!(!p.builtin);

    assert!(repo.rename_profile(id, "Family").await.unwrap());
    let fetched = repo.get_profile(id).await.unwrap().unwrap();
    assert_eq!(fetched.name, "Family");
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
    repo.create_profile(id, "Temp").await.unwrap();

    assert!(repo.delete_profile(id).await.unwrap());
    assert!(repo.get_profile(id).await.unwrap().is_none());
}

// ── Profile-scoped CRUD ───────────────────────────────────────────────────

#[tokio::test]
async fn blocklists_are_scoped_to_their_profile() {
    let pool = test_pool().await;
    let repo = SqliteDnsFilterRepository::new(pool);

    let custom = Uuid::new_v4();
    repo.create_profile(custom, "Custom").await.unwrap();

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

    assert!(inputs.blocked_domains.iter().any(|d| d == "ads.example"));
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
    repo.create_profile(a, "First").await.unwrap();
    repo.create_profile(b, "Second").await.unwrap();

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
    repo.create_profile(a, "A").await.unwrap();
    repo.create_profile(b, "B").await.unwrap();

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
    repo.create_profile(p, "Doomed").await.unwrap();

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
