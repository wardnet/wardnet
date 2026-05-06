use super::test_pool;
use crate::repository::SqliteDnsRepository;
use crate::repository::dns::{BucketSize, DnsRepository, QueryLogFilter, QueryLogRow};
use chrono::{Duration, Utc};

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
async fn query_stats_aggregates_totals() {
    let pool = test_pool().await;
    let repo = SqliteDnsRepository::new(pool);

    let entries = vec![
        sample_row("10.0.0.1", "a.com", "forwarded"),
        sample_row("10.0.0.1", "b.com", "blocked"),
        sample_row("10.0.0.2", "b.com", "blocked"),
        sample_row("10.0.0.3", "c.com", "cache_hit"),
    ];
    repo.insert_query_log_batch(&entries).await.unwrap();

    let since = Utc::now() - Duration::hours(1);
    let stats = repo.query_stats(since).await.unwrap();

    assert_eq!(stats.total_queries, 4);
    assert_eq!(stats.blocked_queries, 2);
    assert_eq!(stats.unique_clients, 3);
    assert_eq!(stats.unique_domains, 3);
    assert!((stats.avg_latency_ms - 1.5).abs() < 1e-6);
}

#[tokio::test]
async fn query_stats_excludes_rows_before_since() {
    let pool = test_pool().await;
    let repo = SqliteDnsRepository::new(pool);

    let entries = vec![
        QueryLogRow {
            timestamp: "2020-01-01T00:00:00Z".to_owned(),
            client_ip: "10.0.0.1".to_owned(),
            domain: "old.com".to_owned(),
            query_type: "A".to_owned(),
            result: "blocked".to_owned(),
            upstream: None,
            latency_ms: 1.0,
            device_id: None,
        },
        sample_row("10.0.0.2", "fresh.com", "forwarded"),
    ];
    repo.insert_query_log_batch(&entries).await.unwrap();

    let since = Utc::now() - Duration::hours(1);
    let stats = repo.query_stats(since).await.unwrap();
    assert_eq!(stats.total_queries, 1);
    assert_eq!(stats.unique_domains, 1);
}

#[tokio::test]
async fn top_domains_orders_by_count_desc() {
    let pool = test_pool().await;
    let repo = SqliteDnsRepository::new(pool);

    let mut entries = Vec::new();
    for _ in 0..3 {
        entries.push(sample_row("10.0.0.1", "ads.example.com", "blocked"));
    }
    for _ in 0..5 {
        entries.push(sample_row("10.0.0.2", "trk.example.com", "blocked"));
    }
    entries.push(sample_row("10.0.0.3", "ok.example.com", "forwarded"));
    repo.insert_query_log_batch(&entries).await.unwrap();

    let since = Utc::now() - Duration::hours(1);

    let all = repo.top_domains(since, 10, false).await.unwrap();
    assert_eq!(all.len(), 3);
    assert_eq!(all[0].domain, "trk.example.com");
    assert_eq!(all[0].count, 5);
    assert_eq!(all[1].domain, "ads.example.com");
    assert_eq!(all[1].count, 3);

    let blocked = repo.top_domains(since, 10, true).await.unwrap();
    assert_eq!(blocked.len(), 2);
    assert!(blocked.iter().all(|d| d.domain != "ok.example.com"));

    let limited = repo.top_domains(since, 1, false).await.unwrap();
    assert_eq!(limited.len(), 1);
    assert_eq!(limited[0].domain, "trk.example.com");
}

#[tokio::test]
async fn top_clients_orders_and_limits() {
    let pool = test_pool().await;
    let repo = SqliteDnsRepository::new(pool);

    let mut entries = Vec::new();
    for _ in 0..2 {
        entries.push(sample_row("10.0.0.1", "a.com", "forwarded"));
    }
    for _ in 0..4 {
        entries.push(sample_row("10.0.0.2", "b.com", "forwarded"));
    }
    entries.push(sample_row("10.0.0.3", "c.com", "forwarded"));
    repo.insert_query_log_batch(&entries).await.unwrap();

    let since = Utc::now() - Duration::hours(1);
    let rows = repo.top_clients(since, 10).await.unwrap();
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0].client_ip, "10.0.0.2");
    assert_eq!(rows[0].count, 4);
    assert_eq!(rows[1].client_ip, "10.0.0.1");
    assert_eq!(rows[1].count, 2);
}

#[tokio::test]
async fn series_buckets_groups_by_hour() {
    let pool = test_pool().await;
    let repo = SqliteDnsRepository::new(pool);

    let entries = vec![
        sample_row("10.0.0.1", "a.com", "forwarded"),
        sample_row("10.0.0.1", "b.com", "blocked"),
        sample_row("10.0.0.2", "c.com", "forwarded"),
    ];
    repo.insert_query_log_batch(&entries).await.unwrap();

    let since = Utc::now() - Duration::hours(1);
    let buckets = repo.series_buckets(since, BucketSize::Hour).await.unwrap();
    assert_eq!(buckets.len(), 1);
    assert_eq!(buckets[0].total, 3);
    assert_eq!(buckets[0].blocked, 1);
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
