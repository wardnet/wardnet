#![allow(clippy::float_cmp)]

use super::test_pool;
use crate::repository::SqliteStatsRepository;
use crate::repository::stats::{HourlyStatRow, IntradayStatRow, StatsRepository};
use wardnet_common::stats::StatsBucket;

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
        .top_n(
            "dns.queries",
            "domain",
            None,
            StatsBucket::Minute,
            0,
            9999,
            2,
        )
        .await
        .unwrap();
    assert_eq!(entries.len(), 2, "limit of 2 must be respected");
    assert_eq!(entries[0].total, 10.0, "highest total must be first");
    assert_eq!(entries[1].total, 7.0);
}

/// COALESCE fallback: ranking by `device_id` still counts traffic from
/// sources with no attributed device — those entries group by their
/// `client` IP instead of being dropped, and rows sharing a `device_id`
/// aggregate across different client IPs (DHCP churn).
#[tokio::test]
async fn top_n_fallback_groups_unattributed_by_fallback_key() {
    let pool = test_pool().await;
    let repo = SqliteStatsRepository::new(pool);
    repo.upsert_intraday(&[
        // Same device seen under two IPs — must aggregate to one entry.
        intraday(
            "dns.queries.by_client",
            r#"{"client":"10.0.0.5","device_id":"dev-a"}"#,
            1000,
            4.0,
            "counter",
        ),
        intraday(
            "dns.queries.by_client",
            r#"{"client":"10.0.0.9","device_id":"dev-a"}"#,
            1000,
            3.0,
            "counter",
        ),
        // Unknown source — no device_id label; groups by client IP.
        intraday(
            "dns.queries.by_client",
            r#"{"client":"10.0.0.77"}"#,
            1000,
            5.0,
            "counter",
        ),
    ])
    .await
    .unwrap();
    let entries = repo
        .top_n(
            "dns.queries.by_client",
            "device_id",
            Some("client"),
            StatsBucket::Minute,
            0,
            9999,
            10,
        )
        .await
        .unwrap();
    assert_eq!(entries.len(), 2, "device group + unattributed IP group");
    assert_eq!(entries[0].total, 7.0, "device totals aggregate across IPs");
    assert!(entries[0].labels.contains("dev-a"));
    assert_eq!(entries[1].total, 5.0);
    assert!(entries[1].labels.contains("10.0.0.77"));
}

/// Top-N over the hourly tier ranks `stats_hourly`, not intraday — the
/// mid-length window path.
#[tokio::test]
async fn top_n_hour_bucket_ranks_over_hourly_tier() {
    let pool = test_pool().await;
    let repo = SqliteStatsRepository::new(pool);
    repo.upsert_hourly(&[
        hourly(
            "dns.blocked.by_tracker",
            r#"{"company":"Google"}"#,
            3600,
            30.0,
            "counter",
        ),
        hourly(
            "dns.blocked.by_tracker",
            r#"{"company":"Meta"}"#,
            3600,
            12.0,
            "counter",
        ),
    ])
    .await
    .unwrap();
    // A same-metric intraday decoy must be ignored by the hourly ranking.
    repo.upsert_intraday(&[intraday(
        "dns.blocked.by_tracker",
        r#"{"company":"Adobe"}"#,
        3600,
        999.0,
        "counter",
    )])
    .await
    .unwrap();
    let entries = repo
        .top_n(
            "dns.blocked.by_tracker",
            "company",
            None,
            StatsBucket::Hour,
            0,
            99999,
            5,
        )
        .await
        .unwrap();
    assert_eq!(entries.len(), 2, "only the two hourly-tier companies");
    assert_eq!(entries[0].total, 30.0);
    assert!(entries[0].labels.contains("Google"));
}

/// Top-N over the daily tier ranks `stats_daily`, converting the Unix-second
/// bounds to calendar days — the long-window (week/month) path.
#[tokio::test]
async fn top_n_day_bucket_ranks_over_daily_tier() {
    let pool = test_pool().await;
    let repo = SqliteStatsRepository::new(pool);
    // day_base is 2026-01-15 00:00:00 UTC.
    let day_base = 1_768_435_200_i64;
    repo.upsert_intraday(&[
        intraday(
            "dns.blocked.by_tracker",
            r#"{"company":"Google"}"#,
            day_base,
            25.0,
            "counter",
        ),
        intraday(
            "dns.blocked.by_tracker",
            r#"{"company":"Criteo"}"#,
            day_base,
            9.0,
            "counter",
        ),
    ])
    .await
    .unwrap();
    repo.rollup_daily("2026-01-15").await.unwrap();
    let entries = repo
        .top_n(
            "dns.blocked.by_tracker",
            "company",
            None,
            StatsBucket::Day,
            day_base,
            day_base + 86_400,
            5,
        )
        .await
        .unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].total, 25.0);
    assert!(entries[0].labels.contains("Google"));
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

// ── stats_hourly ──────────────────────────────────────────────────────────────

fn hourly(metric: &str, labels: &str, hour_ts: i64, value: f64, kind: &str) -> HourlyStatRow {
    HourlyStatRow {
        metric: metric.to_owned(),
        labels: labels.to_owned(),
        hour_ts,
        value,
        kind: kind.to_owned(),
    }
}

#[tokio::test]
async fn upsert_hourly_inserts_new_rows() {
    let pool = test_pool().await;
    let repo = SqliteStatsRepository::new(pool);
    repo.upsert_hourly(&[hourly("dns.queries", "{}", 3600, 5.0, "counter")])
        .await
        .unwrap();
    let rows = repo
        .query_hourly("dns.queries", None, 0, 99999)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].value, 5.0);
    assert_eq!(rows[0].kind, "counter");
}

#[tokio::test]
async fn upsert_hourly_counter_accumulates_on_conflict() {
    let pool = test_pool().await;
    let repo = SqliteStatsRepository::new(pool);
    let row = hourly("hits", "{}", 3600, 3.0, "counter");
    repo.upsert_hourly(std::slice::from_ref(&row))
        .await
        .unwrap();
    repo.upsert_hourly(&[row]).await.unwrap();
    let rows = repo.query_hourly("hits", None, 0, 99999).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].value, 6.0, "counter must accumulate on conflict");
}

#[tokio::test]
async fn upsert_hourly_gauge_overwrites_on_conflict() {
    let pool = test_pool().await;
    let repo = SqliteStatsRepository::new(pool);
    repo.upsert_hourly(&[hourly("latency", "{}", 3600, 100.0, "gauge")])
        .await
        .unwrap();
    repo.upsert_hourly(&[hourly("latency", "{}", 3600, 50.0, "gauge")])
        .await
        .unwrap();
    let rows = repo.query_hourly("latency", None, 0, 99999).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].value, 50.0, "gauge must overwrite on conflict");
}

#[tokio::test]
async fn rollup_hourly_aggregates_counter_rows() {
    let pool = test_pool().await;
    let repo = SqliteStatsRepository::new(pool);
    // hour_ts = 3600 covers minute rows at 3600 and 3660.
    repo.upsert_intraday(&[
        intraday("dns.queries", "{}", 3600, 10.0, "counter"),
        intraday("dns.queries", "{}", 3660, 20.0, "counter"),
    ])
    .await
    .unwrap();
    let n = repo.rollup_hourly(3600).await.unwrap();
    assert!(n > 0, "rollup must insert at least one hourly row");
    let rows = repo
        .query_hourly("dns.queries", None, 0, 99999)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].value, 30.0,
        "counter rollup must sum intraday values"
    );
    assert_eq!(rows[0].hour_ts, 3600);
}

#[tokio::test]
async fn rollup_hourly_gauge_takes_latest_value() {
    let pool = test_pool().await;
    let repo = SqliteStatsRepository::new(pool);
    repo.upsert_intraday(&[
        intraday("latency", "{}", 3600, 100.0, "gauge"),
        intraday("latency", "{}", 3660, 50.0, "gauge"),
    ])
    .await
    .unwrap();
    repo.rollup_hourly(3600).await.unwrap();
    let rows = repo.query_hourly("latency", None, 0, 99999).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].value, 50.0,
        "gauge rollup must take the latest value"
    );
}

#[tokio::test]
async fn rollup_hourly_is_idempotent() {
    let pool = test_pool().await;
    let repo = SqliteStatsRepository::new(pool);
    repo.upsert_intraday(&[intraday("m", "{}", 3600, 5.0, "counter")])
        .await
        .unwrap();
    repo.rollup_hourly(3600).await.unwrap();
    repo.rollup_hourly(3600).await.unwrap();
    let rows = repo.query_hourly("m", None, 0, 99999).await.unwrap();
    assert_eq!(
        rows.len(),
        1,
        "second rollup must not duplicate the hourly row"
    );
    assert_eq!(rows[0].value, 5.0);
}

#[tokio::test]
async fn rollup_hourly_excludes_rows_from_next_hour() {
    let pool = test_pool().await;
    let repo = SqliteStatsRepository::new(pool);
    repo.upsert_intraday(&[
        intraday("m", "{}", 3600, 1.0, "counter"), // in this hour
        intraday("m", "{}", 7200, 9.0, "counter"), // next hour — must be excluded
    ])
    .await
    .unwrap();
    repo.rollup_hourly(3600).await.unwrap();
    let rows = repo.query_hourly("m", None, 0, 99999).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].value, 1.0,
        "next-hour rows must not bleed into rollup"
    );
}

#[tokio::test]
async fn trim_hourly_removes_old_rows() {
    let pool = test_pool().await;
    let repo = SqliteStatsRepository::new(pool);
    repo.upsert_hourly(&[
        hourly("m", "{}", 3600, 1.0, "counter"),
        hourly("m", "{}", 7200, 2.0, "counter"),
    ])
    .await
    .unwrap();
    let deleted = repo.trim_hourly(5000).await.unwrap();
    assert_eq!(deleted, 1);
    let remaining = repo.query_hourly("m", None, 0, 99999).await.unwrap();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].hour_ts, 7200);
}

#[tokio::test]
async fn query_hourly_with_label_filter() {
    let pool = test_pool().await;
    let repo = SqliteStatsRepository::new(pool);
    repo.upsert_hourly(&[
        hourly("m", r#"{"outcome":"blocked"}"#, 3600, 5.0, "counter"),
        hourly("m", r#"{"outcome":"forwarded"}"#, 3600, 3.0, "counter"),
    ])
    .await
    .unwrap();
    let rows = repo
        .query_hourly("m", Some(r#"{"outcome":"blocked"}"#), 0, 99999)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].value, 5.0);
}
