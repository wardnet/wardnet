use super::test_pool;
use crate::repository::SqliteDnsRepository;
use crate::repository::dns::{DnsRepository, QueryLogFilter, QueryLogRow};
use chrono::Utc;
use wardnet_common::dns::DnsQueryResult;

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

    // Substring semantics: a partial IP from the free-text filter narrows
    // to every client that contains it.
    let partial = QueryLogFilter {
        client_ip: Some("192.168.1".to_owned()),
        ..Default::default()
    };
    let rows = repo.query_log_paginated(10, 0, &partial).await.unwrap();
    assert_eq!(rows.len(), 3, "partial IP must match all 192.168.1.* rows");
}

/// The device filter matches on write-time attribution: the same device
/// stays findable after its IP changes, and an unattributed row (NULL
/// `device_id`) never matches a device filter.
#[tokio::test]
async fn query_log_filter_by_device_id() {
    let pool = test_pool().await;
    let repo = SqliteDnsRepository::new(pool);

    let device = "550e8400-e29b-41d4-a716-446655440000";
    let mut before_dhcp_churn = sample_row("192.168.1.10", "a.com", "allowed");
    before_dhcp_churn.device_id = Some(device.to_owned());
    let mut after_dhcp_churn = sample_row("192.168.1.55", "b.com", "allowed");
    after_dhcp_churn.device_id = Some(device.to_owned());
    // Different device later holding the first IP — must not match.
    let unattributed_on_same_ip = sample_row("192.168.1.10", "c.com", "blocked");

    repo.insert_query_log_batch(&[before_dhcp_churn, after_dhcp_churn, unattributed_on_same_ip])
        .await
        .unwrap();

    let filter = QueryLogFilter {
        device_id: Some(device.to_owned()),
        ..Default::default()
    };
    let rows = repo.query_log_paginated(10, 0, &filter).await.unwrap();
    assert_eq!(rows.len(), 2, "both IPs' rows attribute to the device");
    for row in &rows {
        assert_eq!(row.device_id.as_deref(), Some(device));
    }
    assert_eq!(repo.query_log_count(&filter).await.unwrap(), 2);
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
        result: Some(DnsQueryResult::Blocked),
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
        result: Some(DnsQueryResult::Blocked),
        ..Default::default()
    };
    let count = repo.query_log_count(&filter).await.unwrap();
    assert_eq!(count, 2);

    let combined = QueryLogFilter {
        client_ip: Some("10.0.0.2".to_owned()),
        result: Some(DnsQueryResult::Blocked),
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
