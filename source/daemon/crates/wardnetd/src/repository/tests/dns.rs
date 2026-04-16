use super::test_pool;
use crate::repository::SqliteDnsRepository;
use crate::repository::dns::{
    AllowlistRow, BlocklistRow, BlocklistUpdate, CustomRuleRow, CustomRuleUpdate, DnsRepository,
    QueryLogFilter, QueryLogRow,
};
use chrono::Utc;
use uuid::Uuid;

fn ts_now() -> String {
    Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

fn sample_row(client_ip: &str, domain: &str, result: &str) -> QueryLogRow {
    QueryLogRow {
        timestamp: ts_now(),
        client_ip: client_ip.to_owned(),
        domain: domain.to_owned(),
        query_type: "A".to_owned(),
        result: result.to_owned(),
        upstream: Some("8.8.8.8".to_owned()),
        latency_ms: 1.5,
        device_id: None,
    }
}

// ── Seed blocklist IDs from migration ─────────────────────────────────────

const SEED_BL1: &str = "00000000-0000-0000-0000-000000000001";
const SEED_BL2: &str = "00000000-0000-0000-0000-000000000002";

// ── Query log tests (existing) ────────────────────────────────────────────

#[tokio::test]
async fn insert_and_query_log_batch() {
    let pool = test_pool().await;
    let repo = SqliteDnsRepository::new(pool);

    let entries = vec![
        sample_row("192.168.1.10", "example.com", "allowed"),
        sample_row("192.168.1.11", "test.org", "blocked"),
        sample_row("192.168.1.12", "foo.bar", "allowed"),
    ];
    repo.insert_query_log_batch(&entries).await.unwrap();

    let filter = QueryLogFilter::default();
    let rows = repo.query_log_paginated(10, 0, &filter).await.unwrap();
    assert_eq!(rows.len(), 3);

    let page2 = repo.query_log_paginated(10, 3, &filter).await.unwrap();
    assert!(page2.is_empty());

    let limited = repo.query_log_paginated(2, 0, &filter).await.unwrap();
    assert_eq!(limited.len(), 2);
}

#[tokio::test]
async fn query_log_filter_by_client_ip() {
    let pool = test_pool().await;
    let repo = SqliteDnsRepository::new(pool);

    let entries = vec![
        sample_row("192.168.1.10", "a.com", "allowed"),
        sample_row("192.168.1.20", "b.com", "allowed"),
        sample_row("192.168.1.10", "c.com", "blocked"),
    ];
    repo.insert_query_log_batch(&entries).await.unwrap();

    let filter = QueryLogFilter {
        client_ip: Some("192.168.1.10".to_owned()),
        ..Default::default()
    };
    let rows = repo.query_log_paginated(10, 0, &filter).await.unwrap();
    assert_eq!(rows.len(), 2);
    for row in &rows {
        assert_eq!(row.client_ip, "192.168.1.10");
    }
}

#[tokio::test]
async fn query_log_filter_by_domain() {
    let pool = test_pool().await;
    let repo = SqliteDnsRepository::new(pool);

    let entries = vec![
        sample_row("10.0.0.1", "ads.tracker.com", "blocked"),
        sample_row("10.0.0.1", "example.com", "allowed"),
        sample_row("10.0.0.1", "tracker.net", "blocked"),
    ];
    repo.insert_query_log_batch(&entries).await.unwrap();

    let filter = QueryLogFilter {
        domain: Some("tracker".to_owned()),
        ..Default::default()
    };
    let rows = repo.query_log_paginated(10, 0, &filter).await.unwrap();
    assert_eq!(rows.len(), 2);
    for row in &rows {
        assert!(row.domain.contains("tracker"));
    }
}

#[tokio::test]
async fn query_log_filter_by_result() {
    let pool = test_pool().await;
    let repo = SqliteDnsRepository::new(pool);

    let entries = vec![
        sample_row("10.0.0.1", "a.com", "allowed"),
        sample_row("10.0.0.2", "b.com", "blocked"),
        sample_row("10.0.0.3", "c.com", "blocked"),
    ];
    repo.insert_query_log_batch(&entries).await.unwrap();

    let filter = QueryLogFilter {
        result: Some("blocked".to_owned()),
        ..Default::default()
    };
    let rows = repo.query_log_paginated(10, 0, &filter).await.unwrap();
    assert_eq!(rows.len(), 2);
    for row in &rows {
        assert_eq!(row.result, "blocked");
    }
}

#[tokio::test]
async fn query_log_count() {
    let pool = test_pool().await;
    let repo = SqliteDnsRepository::new(pool);

    let entries = vec![
        sample_row("10.0.0.1", "a.com", "allowed"),
        sample_row("10.0.0.2", "b.com", "blocked"),
        sample_row("10.0.0.3", "c.com", "allowed"),
        sample_row("10.0.0.4", "d.com", "blocked"),
    ];
    repo.insert_query_log_batch(&entries).await.unwrap();

    let count = repo
        .query_log_count(&QueryLogFilter::default())
        .await
        .unwrap();
    assert_eq!(count, 4);
}

#[tokio::test]
async fn query_log_count_with_filter() {
    let pool = test_pool().await;
    let repo = SqliteDnsRepository::new(pool);

    let entries = vec![
        sample_row("10.0.0.1", "a.com", "allowed"),
        sample_row("10.0.0.2", "b.com", "blocked"),
        sample_row("10.0.0.3", "c.com", "blocked"),
        sample_row("10.0.0.4", "d.com", "allowed"),
    ];
    repo.insert_query_log_batch(&entries).await.unwrap();

    let filter = QueryLogFilter {
        result: Some("blocked".to_owned()),
        ..Default::default()
    };
    let count = repo.query_log_count(&filter).await.unwrap();
    assert_eq!(count, 2);

    let combined = QueryLogFilter {
        client_ip: Some("10.0.0.2".to_owned()),
        result: Some("blocked".to_owned()),
        ..Default::default()
    };
    let count2 = repo.query_log_count(&combined).await.unwrap();
    assert_eq!(count2, 1);
}

#[tokio::test]
async fn cleanup_query_log() {
    let pool = test_pool().await;
    let repo = SqliteDnsRepository::new(pool);

    let old_ts = "2020-01-01T00:00:00Z".to_owned();
    let recent_ts = ts_now();

    let entries = vec![
        QueryLogRow {
            timestamp: old_ts.clone(),
            client_ip: "10.0.0.1".to_owned(),
            domain: "old.com".to_owned(),
            query_type: "A".to_owned(),
            result: "allowed".to_owned(),
            upstream: None,
            latency_ms: 1.0,
            device_id: None,
        },
        QueryLogRow {
            timestamp: old_ts,
            client_ip: "10.0.0.2".to_owned(),
            domain: "ancient.com".to_owned(),
            query_type: "AAAA".to_owned(),
            result: "blocked".to_owned(),
            upstream: None,
            latency_ms: 2.0,
            device_id: None,
        },
        QueryLogRow {
            timestamp: recent_ts,
            client_ip: "10.0.0.3".to_owned(),
            domain: "fresh.com".to_owned(),
            query_type: "A".to_owned(),
            result: "allowed".to_owned(),
            upstream: Some("1.1.1.1".to_owned()),
            latency_ms: 0.5,
            device_id: None,
        },
    ];
    repo.insert_query_log_batch(&entries).await.unwrap();

    let deleted = repo.cleanup_query_log(30).await.unwrap();
    assert_eq!(deleted, 2);

    let remaining = repo
        .query_log_count(&QueryLogFilter::default())
        .await
        .unwrap();
    assert_eq!(remaining, 1);

    let rows = repo
        .query_log_paginated(10, 0, &QueryLogFilter::default())
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].domain, "fresh.com");
}

#[tokio::test]
async fn query_log_paginated_ordering() {
    let pool = test_pool().await;
    let repo = SqliteDnsRepository::new(pool);

    let entries = vec![
        sample_row("10.0.0.1", "first.com", "allowed"),
        sample_row("10.0.0.2", "second.com", "allowed"),
        sample_row("10.0.0.3", "third.com", "allowed"),
    ];
    repo.insert_query_log_batch(&entries).await.unwrap();

    let rows = repo
        .query_log_paginated(10, 0, &QueryLogFilter::default())
        .await
        .unwrap();
    assert_eq!(rows.len(), 3);

    assert_eq!(rows[0].domain, "third.com");
    assert_eq!(rows[1].domain, "second.com");
    assert_eq!(rows[2].domain, "first.com");
}

#[tokio::test]
async fn insert_empty_batch() {
    let pool = test_pool().await;
    let repo = SqliteDnsRepository::new(pool);

    repo.insert_query_log_batch(&[]).await.unwrap();

    let count = repo
        .query_log_count(&QueryLogFilter::default())
        .await
        .unwrap();
    assert_eq!(count, 0);
}

// ── Blocklist tests ───────────────────────────────────────────────────────

#[tokio::test]
async fn list_blocklists_returns_seeded() {
    let pool = test_pool().await;
    let repo = SqliteDnsRepository::new(pool);

    let lists = repo.list_blocklists().await.unwrap();
    assert_eq!(lists.len(), 2);

    let names: Vec<&str> = lists.iter().map(|b| b.name.as_str()).collect();
    assert!(names.contains(&"Steven Black Unified"));
    assert!(names.contains(&"OISD Basic"));
}

#[tokio::test]
async fn create_and_get_blocklist() {
    let pool = test_pool().await;
    let repo = SqliteDnsRepository::new(pool);

    let id = Uuid::new_v4();
    let row = BlocklistRow {
        id: id.to_string(),
        name: "Test List".to_owned(),
        url: "https://example.com/hosts".to_owned(),
        enabled: true,
        cron_schedule: "0 4 * * *".to_owned(),
    };
    repo.create_blocklist(&row).await.unwrap();

    let bl = repo.get_blocklist(id).await.unwrap().expect("should exist");
    assert_eq!(bl.id, id);
    assert_eq!(bl.name, "Test List");
    assert_eq!(bl.url, "https://example.com/hosts");
    assert!(bl.enabled);
    assert_eq!(bl.entry_count, 0);
    assert_eq!(bl.cron_schedule, "0 4 * * *");
    assert!(bl.last_error.is_none());
}

#[tokio::test]
async fn update_blocklist_partial() {
    let pool = test_pool().await;
    let repo = SqliteDnsRepository::new(pool);

    let id: Uuid = SEED_BL1.parse().unwrap();
    let original = repo.get_blocklist(id).await.unwrap().unwrap();

    repo.update_blocklist(
        id,
        &BlocklistUpdate {
            name: Some("Renamed".to_owned()),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let updated = repo.get_blocklist(id).await.unwrap().unwrap();
    assert_eq!(updated.name, "Renamed");
    // Other fields unchanged.
    assert_eq!(updated.url, original.url);
    assert_eq!(updated.enabled, original.enabled);
    assert!(updated.updated_at >= original.updated_at);
}

#[tokio::test]
async fn delete_blocklist_existing_and_nonexistent() {
    let pool = test_pool().await;
    let repo = SqliteDnsRepository::new(pool);

    let id: Uuid = SEED_BL1.parse().unwrap();
    assert!(repo.delete_blocklist(id).await.unwrap());
    assert!(!repo.delete_blocklist(id).await.unwrap());
}

#[tokio::test]
async fn replace_blocklist_domains_inserts_and_updates_metadata() {
    let pool = test_pool().await;
    let repo = SqliteDnsRepository::new(pool);

    let id: Uuid = SEED_BL1.parse().unwrap();
    // Enable it first so we can test the enabled query later.
    repo.update_blocklist(
        id,
        &BlocklistUpdate {
            enabled: Some(true),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let domains: Vec<String> = vec!["ads.example.com", "tracker.io", "bad.net"]
        .into_iter()
        .map(String::from)
        .collect();

    let count = repo.replace_blocklist_domains(id, &domains).await.unwrap();
    assert_eq!(count, 3);

    let bl = repo.get_blocklist(id).await.unwrap().unwrap();
    assert_eq!(bl.entry_count, 3);
    assert!(bl.last_updated.is_some());
    assert!(bl.last_error.is_none());
}

#[tokio::test]
async fn replace_blocklist_domains_replaces_not_appends() {
    let pool = test_pool().await;
    let repo = SqliteDnsRepository::new(pool);

    let id: Uuid = SEED_BL1.parse().unwrap();
    repo.update_blocklist(
        id,
        &BlocklistUpdate {
            enabled: Some(true),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let first = vec!["a.com".to_owned(), "b.com".to_owned()];
    repo.replace_blocklist_domains(id, &first).await.unwrap();

    let second = vec!["c.com".to_owned()];
    let count = repo.replace_blocklist_domains(id, &second).await.unwrap();
    assert_eq!(count, 1);

    let bl = repo.get_blocklist(id).await.unwrap().unwrap();
    assert_eq!(bl.entry_count, 1);

    let all = repo.list_all_blocked_domains_for_enabled().await.unwrap();
    assert_eq!(all, vec!["c.com"]);
}

#[tokio::test]
async fn list_all_blocked_domains_excludes_disabled() {
    let pool = test_pool().await;
    let repo = SqliteDnsRepository::new(pool);

    let id1: Uuid = SEED_BL1.parse().unwrap();
    let id2: Uuid = SEED_BL2.parse().unwrap();

    // Enable BL1, keep BL2 disabled (default from seed).
    repo.update_blocklist(
        id1,
        &BlocklistUpdate {
            enabled: Some(true),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    repo.replace_blocklist_domains(id1, &["enabled.com".to_owned()])
        .await
        .unwrap();
    repo.replace_blocklist_domains(id2, &["disabled.com".to_owned()])
        .await
        .unwrap();

    let domains = repo.list_all_blocked_domains_for_enabled().await.unwrap();
    assert!(domains.contains(&"enabled.com".to_owned()));
    assert!(!domains.contains(&"disabled.com".to_owned()));
}

#[tokio::test]
async fn set_blocklist_error_and_clear() {
    let pool = test_pool().await;
    let repo = SqliteDnsRepository::new(pool);

    let id: Uuid = SEED_BL1.parse().unwrap();

    // Set error.
    repo.set_blocklist_error(id, Some("download failed"))
        .await
        .unwrap();
    let bl = repo.get_blocklist(id).await.unwrap().unwrap();
    assert_eq!(bl.last_error.as_deref(), Some("download failed"));
    assert!(bl.last_error_at.is_some());

    // Clear error.
    repo.set_blocklist_error(id, None).await.unwrap();
    let bl = repo.get_blocklist(id).await.unwrap().unwrap();
    assert!(bl.last_error.is_none());
    assert!(bl.last_error_at.is_none());
}

// ── Allowlist tests ───────────────────────────────────────────────────────

#[tokio::test]
async fn list_allowlist_initially_empty() {
    let pool = test_pool().await;
    let repo = SqliteDnsRepository::new(pool);

    let entries = repo.list_allowlist().await.unwrap();
    assert!(entries.is_empty());
}

#[tokio::test]
async fn create_and_list_allowlist_entry() {
    let pool = test_pool().await;
    let repo = SqliteDnsRepository::new(pool);

    let id = Uuid::new_v4();
    let row = AllowlistRow {
        id: id.to_string(),
        domain: "safe.example.com".to_owned(),
        reason: Some("false positive".to_owned()),
    };
    repo.create_allowlist_entry(&row).await.unwrap();

    let entries = repo.list_allowlist().await.unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].id, id);
    assert_eq!(entries[0].domain, "safe.example.com");
    assert_eq!(entries[0].reason.as_deref(), Some("false positive"));
}

#[tokio::test]
async fn delete_allowlist_entry_existing_and_nonexistent() {
    let pool = test_pool().await;
    let repo = SqliteDnsRepository::new(pool);

    let id = Uuid::new_v4();
    let row = AllowlistRow {
        id: id.to_string(),
        domain: "remove.me".to_owned(),
        reason: None,
    };
    repo.create_allowlist_entry(&row).await.unwrap();

    assert!(repo.delete_allowlist_entry(id).await.unwrap());
    assert!(!repo.delete_allowlist_entry(id).await.unwrap());
}

#[tokio::test]
async fn create_allowlist_duplicate_domain_fails() {
    let pool = test_pool().await;
    let repo = SqliteDnsRepository::new(pool);

    let row1 = AllowlistRow {
        id: Uuid::new_v4().to_string(),
        domain: "dup.example.com".to_owned(),
        reason: None,
    };
    repo.create_allowlist_entry(&row1).await.unwrap();

    let row2 = AllowlistRow {
        id: Uuid::new_v4().to_string(),
        domain: "dup.example.com".to_owned(),
        reason: None,
    };
    assert!(repo.create_allowlist_entry(&row2).await.is_err());
}

// ── Custom rule tests ─────────────────────────────────────────────────────

#[tokio::test]
async fn list_custom_rules_initially_empty() {
    let pool = test_pool().await;
    let repo = SqliteDnsRepository::new(pool);

    let rules = repo.list_custom_rules().await.unwrap();
    assert!(rules.is_empty());
}

#[tokio::test]
async fn create_and_get_custom_rule() {
    let pool = test_pool().await;
    let repo = SqliteDnsRepository::new(pool);

    let id = Uuid::new_v4();
    let row = CustomRuleRow {
        id: id.to_string(),
        rule_text: "||ads.example.com^".to_owned(),
        enabled: true,
        comment: Some("block ads".to_owned()),
    };
    repo.create_custom_rule(&row).await.unwrap();

    let rule = repo
        .get_custom_rule(id)
        .await
        .unwrap()
        .expect("should exist");
    assert_eq!(rule.id, id);
    assert_eq!(rule.rule_text, "||ads.example.com^");
    assert!(rule.enabled);
    assert_eq!(rule.comment.as_deref(), Some("block ads"));
}

#[tokio::test]
async fn update_custom_rule_partial() {
    let pool = test_pool().await;
    let repo = SqliteDnsRepository::new(pool);

    let id = Uuid::new_v4();
    let row = CustomRuleRow {
        id: id.to_string(),
        rule_text: "||tracker.io^".to_owned(),
        enabled: true,
        comment: None,
    };
    repo.create_custom_rule(&row).await.unwrap();

    repo.update_custom_rule(
        id,
        &CustomRuleUpdate {
            enabled: Some(false),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let rule = repo.get_custom_rule(id).await.unwrap().unwrap();
    assert!(!rule.enabled);
    // rule_text unchanged.
    assert_eq!(rule.rule_text, "||tracker.io^");
}

#[tokio::test]
async fn delete_custom_rule_existing_and_nonexistent() {
    let pool = test_pool().await;
    let repo = SqliteDnsRepository::new(pool);

    let id = Uuid::new_v4();
    let row = CustomRuleRow {
        id: id.to_string(),
        rule_text: "@@||safe.com^".to_owned(),
        enabled: true,
        comment: None,
    };
    repo.create_custom_rule(&row).await.unwrap();

    assert!(repo.delete_custom_rule(id).await.unwrap());
    assert!(!repo.delete_custom_rule(id).await.unwrap());
}

#[tokio::test]
async fn bulk_insert_5000_domains() {
    let pool = test_pool().await;
    let repo = SqliteDnsRepository::new(pool);

    let id: Uuid = SEED_BL1.parse().unwrap();
    repo.update_blocklist(
        id,
        &BlocklistUpdate {
            enabled: Some(true),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let domains: Vec<String> = (0..5000)
        .map(|i| format!("domain-{i}.example.com"))
        .collect();

    let count = repo.replace_blocklist_domains(id, &domains).await.unwrap();
    assert_eq!(count, 5000);

    let bl = repo.get_blocklist(id).await.unwrap().unwrap();
    assert_eq!(bl.entry_count, 5000);

    let all = repo.list_all_blocked_domains_for_enabled().await.unwrap();
    assert_eq!(all.len(), 5000);
}
