#![allow(clippy::float_cmp)]

use super::test_pool;
use crate::repository::SqliteStatsRepository;
use crate::repository::stats::{IntradayStatRow, StatsRepository};

fn intraday(metric: &str, labels: &str, bucket_ts: i64, value: f64, kind: &str) -> IntradayStatRow {
    IntradayStatRow {
        metric: metric.to_owned(),
        labels: labels.to_owned(),
        bucket_ts,
        value,
        kind: kind.to_owned(),
    }
}

#[tokio::test]
async fn upsert_intraday_inserts_new_rows() {
    let pool = test_pool().await;
    let repo = SqliteStatsRepository::new(pool);
    repo.upsert_intraday(&[intraday("dns.queries", "{}", 1000, 5.0, "counter")])
        .await
        .unwrap();
    let rows = repo
        .query_intraday("dns.queries", None, 0, 9999)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].value, 5.0);
    assert_eq!(rows[0].kind, "counter");
}

#[tokio::test]
async fn upsert_empty_slice_is_noop() {
    let pool = test_pool().await;
    let repo = SqliteStatsRepository::new(pool);
    repo.upsert_intraday(&[]).await.unwrap();
    let rows = repo.query_intraday("any", None, 0, 9999).await.unwrap();
    assert!(rows.is_empty());
}

#[tokio::test]
async fn upsert_counter_accumulates_on_conflict() {
    let pool = test_pool().await;
    let repo = SqliteStatsRepository::new(pool);
    let row = intraday("hits", r#"{"outcome":"blocked"}"#, 1200, 3.0, "counter");
    repo.upsert_intraday(std::slice::from_ref(&row))
        .await
        .unwrap();
    repo.upsert_intraday(&[row]).await.unwrap();
    let rows = repo.query_intraday("hits", None, 0, 9999).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].value, 6.0, "counter must accumulate on conflict");
}

#[tokio::test]
async fn upsert_gauge_overwrites_on_conflict() {
    let pool = test_pool().await;
    let repo = SqliteStatsRepository::new(pool);
    repo.upsert_intraday(&[intraday("latency", "{}", 1200, 100.0, "gauge")])
        .await
        .unwrap();
    repo.upsert_intraday(&[intraday("latency", "{}", 1200, 50.0, "gauge")])
        .await
        .unwrap();
    let rows = repo.query_intraday("latency", None, 0, 9999).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].value, 50.0, "gauge must overwrite on conflict");
}

#[tokio::test]
async fn query_intraday_filters_by_time_range() {
    let pool = test_pool().await;
    let repo = SqliteStatsRepository::new(pool);
    repo.upsert_intraday(&[
        intraday("m", r#"{"a":"1"}"#, 100, 1.0, "counter"),
        intraday("m", r#"{"a":"2"}"#, 500, 2.0, "counter"),
        intraday("m", r#"{"a":"3"}"#, 900, 3.0, "counter"),
    ])
    .await
    .unwrap();
    let rows = repo.query_intraday("m", None, 200, 700).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].value, 2.0);
}

#[tokio::test]
async fn query_intraday_with_label_filter() {
    let pool = test_pool().await;
    let repo = SqliteStatsRepository::new(pool);
    repo.upsert_intraday(&[
        intraday("m", r#"{"outcome":"blocked"}"#, 100, 5.0, "counter"),
        intraday("m", r#"{"outcome":"forwarded"}"#, 100, 3.0, "counter"),
    ])
    .await
    .unwrap();
    let rows = repo
        .query_intraday("m", Some(r#"{"outcome":"blocked"}"#), 0, 9999)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].value, 5.0);
}

#[tokio::test]
async fn trim_intraday_removes_old_rows() {
    let pool = test_pool().await;
    let repo = SqliteStatsRepository::new(pool);
    repo.upsert_intraday(&[
        intraday("m", r#"{"a":"1"}"#, 100, 1.0, "counter"),
        intraday("m", r#"{"a":"2"}"#, 300, 1.0, "counter"),
    ])
    .await
    .unwrap();
    let deleted = repo.trim_intraday(200).await.unwrap();
    assert_eq!(deleted, 1);
    let remaining = repo.query_intraday("m", None, 0, 9999).await.unwrap();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].bucket_ts, 300);
}

#[tokio::test]
async fn rollup_daily_aggregates_counter_rows() {
    let pool = test_pool().await;
    let repo = SqliteStatsRepository::new(pool);
    // 2026-01-15 00:00:00 UTC = 1768435200
    let day_base = 1_768_435_200_i64;
    repo.upsert_intraday(&[
        intraday("dns.queries", "{}", day_base, 10.0, "counter"),
        intraday("dns.queries", "{}", day_base + 60, 20.0, "counter"),
    ])
    .await
    .unwrap();
    let n = repo.rollup_daily("2026-01-15").await.unwrap();
    assert!(n > 0, "rollup must insert at least one daily row");
    let daily = repo
        .query_daily("dns.queries", None, "2026-01-01", "2026-12-31")
        .await
        .unwrap();
    assert_eq!(daily.len(), 1);
    assert_eq!(
        daily[0].value, 30.0,
        "counter rollup must sum intraday values"
    );
}

#[tokio::test]
async fn rollup_daily_is_idempotent() {
    let pool = test_pool().await;
    let repo = SqliteStatsRepository::new(pool);
    let day_base = 1_768_435_200_i64;
    repo.upsert_intraday(&[intraday("m", "{}", day_base, 5.0, "counter")])
        .await
        .unwrap();
    repo.rollup_daily("2026-01-15").await.unwrap();
    repo.rollup_daily("2026-01-15").await.unwrap();
    let daily = repo
        .query_daily("m", None, "2026-01-01", "2026-12-31")
        .await
        .unwrap();
    assert_eq!(
        daily.len(),
        1,
        "second rollup must not duplicate the daily row"
    );
    assert_eq!(daily[0].value, 5.0);
}

#[tokio::test]
async fn trim_daily_removes_old_rows() {
    let pool = test_pool().await;
    let repo = SqliteStatsRepository::new(pool);
    let day_base = 1_768_435_200_i64;
    repo.upsert_intraday(&[intraday("m", "{}", day_base, 1.0, "counter")])
        .await
        .unwrap();
    repo.rollup_daily("2026-01-15").await.unwrap();
    let deleted = repo.trim_daily("2026-06-01").await.unwrap();
    assert_eq!(deleted, 1);
    let remaining = repo
        .query_daily("m", None, "2000-01-01", "2099-12-31")
        .await
        .unwrap();
    assert!(remaining.is_empty());
}

#[tokio::test]
async fn top_n_returns_results_sorted_by_total_desc() {
    let pool = test_pool().await;
    let repo = SqliteStatsRepository::new(pool);
    repo.upsert_intraday(&[
        intraday(
            "dns.queries",
            r#"{"domain":"ads.com"}"#,
            1000,
            10.0,
            "counter",
        ),
        intraday(
            "dns.queries",
            r#"{"domain":"tracker.net"}"#,
            1000,
            3.0,
            "counter",
        ),
        intraday(
            "dns.queries",
            r#"{"domain":"bad.io"}"#,
            1000,
            7.0,
            "counter",
        ),
    ])
    .await
    .unwrap();
    let entries = repo
        .top_n("dns.queries", "domain", 0, 9999, 2)
        .await
        .unwrap();
    assert_eq!(entries.len(), 2, "limit of 2 must be respected");
    assert_eq!(entries[0].total, 10.0, "highest total must be first");
    assert_eq!(entries[1].total, 7.0);
}

#[tokio::test]
async fn query_daily_with_label_filter() {
    let pool = test_pool().await;
    let repo = SqliteStatsRepository::new(pool);
    let day_base = 1_768_435_200_i64;
    repo.upsert_intraday(&[
        intraday("m", r#"{"outcome":"blocked"}"#, day_base, 5.0, "counter"),
        intraday("m", r#"{"outcome":"forwarded"}"#, day_base, 3.0, "counter"),
    ])
    .await
    .unwrap();
    repo.rollup_daily("2026-01-15").await.unwrap();
    let rows = repo
        .query_daily(
            "m",
            Some(r#"{"outcome":"blocked"}"#),
            "2026-01-01",
            "2026-12-31",
        )
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].value, 5.0);
}
