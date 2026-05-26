#![allow(clippy::float_cmp)]

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::Utc;
use uuid::Uuid;
use wardnet_common::auth::AuthContext;
use wardnet_common::stats::{StatsBucket, StatsQuery, StatsTopEntry, StatsTopQuery};
use wardnetd_data::repository::{DailyStatRow, IntradayStatRow, StatsRepository};

use crate::auth_context;
use crate::stats::service::{StatsService, StatsServiceImpl};

fn admin_ctx() -> AuthContext {
    AuthContext::Admin {
        admin_id: Uuid::new_v4(),
    }
}

// ── In-memory mock ─────────────────────────────────────────────────────────────

#[derive(Default)]
struct MemoryStatsRepo {
    intraday: Mutex<Vec<IntradayStatRow>>,
    daily: Mutex<Vec<DailyStatRow>>,
}

#[async_trait]
impl StatsRepository for MemoryStatsRepo {
    async fn upsert_intraday(&self, rows: &[IntradayStatRow]) -> anyhow::Result<()> {
        self.intraday.lock().unwrap().extend_from_slice(rows);
        Ok(())
    }

    async fn rollup_daily(&self, _day: &str) -> anyhow::Result<usize> {
        Ok(0)
    }

    async fn trim_intraday(&self, _cutoff_ts: i64) -> anyhow::Result<u64> {
        Ok(0)
    }

    async fn trim_daily(&self, _cutoff_day: &str) -> anyhow::Result<u64> {
        Ok(0)
    }

    async fn query_intraday(
        &self,
        metric: &str,
        label_filter: Option<&str>,
        from: i64,
        to: i64,
    ) -> anyhow::Result<Vec<IntradayStatRow>> {
        let guard = self.intraday.lock().unwrap();
        Ok(guard
            .iter()
            .filter(|r| {
                r.metric == metric
                    && r.bucket_ts >= from
                    && r.bucket_ts <= to
                    && label_filter.is_none_or(|f| r.labels == f)
            })
            .cloned()
            .collect())
    }

    async fn query_daily(
        &self,
        metric: &str,
        label_filter: Option<&str>,
        from: &str,
        to: &str,
    ) -> anyhow::Result<Vec<DailyStatRow>> {
        let guard = self.daily.lock().unwrap();
        Ok(guard
            .iter()
            .filter(|r| {
                r.metric == metric
                    && r.day.as_str() >= from
                    && r.day.as_str() <= to
                    && label_filter.is_none_or(|f| r.labels == f)
            })
            .cloned()
            .collect())
    }

    async fn top_n(
        &self,
        metric: &str,
        _label_key: &str,
        from: i64,
        to: i64,
        limit: u32,
    ) -> anyhow::Result<Vec<StatsTopEntry>> {
        let guard = self.intraday.lock().unwrap();
        let mut totals: std::collections::HashMap<String, f64> = std::collections::HashMap::new();
        for r in guard
            .iter()
            .filter(|r| r.metric == metric && r.bucket_ts >= from && r.bucket_ts <= to)
        {
            *totals.entry(r.labels.clone()).or_insert(0.0) += r.value;
        }
        let mut entries: Vec<StatsTopEntry> = totals
            .into_iter()
            .map(|(labels, total)| StatsTopEntry { labels, total })
            .collect();
        entries.sort_by(|a, b| {
            b.total
                .partial_cmp(&a.total)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        entries.truncate(limit as usize);
        Ok(entries)
    }
}

fn make_service() -> StatsServiceImpl {
    StatsServiceImpl::new(Arc::new(MemoryStatsRepo::default()))
}

fn intraday(metric: &str, labels: &str, bucket_ts: i64, value: f64) -> IntradayStatRow {
    IntradayStatRow {
        metric: metric.to_owned(),
        labels: labels.to_owned(),
        bucket_ts,
        value,
        kind: "counter".to_owned(),
    }
}

// ── run_flush ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn run_flush_empty_is_noop() {
    let svc = make_service();
    svc.run_flush(vec![]).await.unwrap();
}

#[tokio::test]
async fn run_flush_delegates_rows_to_repo() {
    let repo = Arc::new(MemoryStatsRepo::default());
    let svc = StatsServiceImpl::new(repo.clone());
    svc.run_flush(vec![intraday("m", "{}", 0, 1.0)])
        .await
        .unwrap();
    assert_eq!(repo.intraday.lock().unwrap().len(), 1);
}

// ── query ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn query_returns_forbidden_without_admin_context() {
    let svc = make_service();
    let q = StatsQuery {
        metric: "m".to_owned(),
        label_filter: None,
        from: Utc::now(),
        to: Utc::now(),
        bucket: StatsBucket::Minute,
    };
    assert!(svc.query(q).await.is_err());
}

#[tokio::test]
async fn query_minute_with_admin_context() {
    let repo = Arc::new(MemoryStatsRepo::default());
    let now_ts = Utc::now().timestamp();
    let bucket_ts = now_ts - (now_ts % 60);
    repo.intraday.lock().unwrap().push(intraday(
        "dns.queries",
        r#"{"outcome":"blocked"}"#,
        bucket_ts,
        5.0,
    ));
    let svc = StatsServiceImpl::new(repo);
    let q = StatsQuery {
        metric: "dns.queries".to_owned(),
        label_filter: None,
        from: Utc::now() - chrono::Duration::hours(1),
        to: Utc::now() + chrono::Duration::hours(1),
        bucket: StatsBucket::Minute,
    };
    let resp = auth_context::with_context(admin_ctx(), svc.query(q))
        .await
        .unwrap();
    assert_eq!(resp.metric, "dns.queries");
    assert_eq!(resp.series.len(), 1);
    assert_eq!(resp.series[0].value, 5.0);
}

#[tokio::test]
async fn query_minute_with_label_filter() {
    let repo = Arc::new(MemoryStatsRepo::default());
    let now_ts = Utc::now().timestamp();
    let bucket_ts = now_ts - (now_ts % 60);
    {
        let mut g = repo.intraday.lock().unwrap();
        g.push(intraday("m", r#"{"outcome":"blocked"}"#, bucket_ts, 5.0));
        g.push(intraday("m", r#"{"outcome":"forwarded"}"#, bucket_ts, 3.0));
    }
    let svc = StatsServiceImpl::new(repo);
    let q = StatsQuery {
        metric: "m".to_owned(),
        label_filter: Some(r#"{"outcome":"blocked"}"#.to_owned()),
        from: Utc::now() - chrono::Duration::hours(1),
        to: Utc::now() + chrono::Duration::hours(1),
        bucket: StatsBucket::Minute,
    };
    let resp = auth_context::with_context(admin_ctx(), svc.query(q))
        .await
        .unwrap();
    assert_eq!(resp.series.len(), 1);
    assert_eq!(resp.series[0].value, 5.0);
}

#[tokio::test]
async fn query_hour_aggregates_minute_rows() {
    let repo = Arc::new(MemoryStatsRepo::default());
    // Two rows in the same hour at different minute offsets
    let hour_base = 1_746_000_000_i64;
    {
        let mut g = repo.intraday.lock().unwrap();
        g.push(intraday("dns.queries", "{}", hour_base, 3.0));
        g.push(intraday("dns.queries", "{}", hour_base + 60, 7.0));
    }
    let svc = StatsServiceImpl::new(repo);
    let from = chrono::DateTime::from_timestamp(hour_base - 1, 0).unwrap();
    let to = chrono::DateTime::from_timestamp(hour_base + 3600, 0).unwrap();
    let q = StatsQuery {
        metric: "dns.queries".to_owned(),
        label_filter: None,
        from,
        to,
        bucket: StatsBucket::Hour,
    };
    let resp = auth_context::with_context(admin_ctx(), svc.query(q))
        .await
        .unwrap();
    assert_eq!(
        resp.series.len(),
        1,
        "two minute rows in the same hour must collapse to one point"
    );
    assert_eq!(resp.series[0].value, 10.0);
}

#[tokio::test]
async fn query_hour_gauge_takes_latest_value() {
    let repo = Arc::new(MemoryStatsRepo::default());
    let hour_base = 1_746_000_000_i64;
    {
        let mut g = repo.intraday.lock().unwrap();
        g.push(IntradayStatRow {
            metric: "latency".to_owned(),
            labels: "{}".to_owned(),
            bucket_ts: hour_base,
            value: 100.0,
            kind: "gauge".to_owned(),
        });
        g.push(IntradayStatRow {
            metric: "latency".to_owned(),
            labels: "{}".to_owned(),
            bucket_ts: hour_base + 60,
            value: 50.0,
            kind: "gauge".to_owned(),
        });
    }
    let svc = StatsServiceImpl::new(repo);
    let from = chrono::DateTime::from_timestamp(hour_base - 1, 0).unwrap();
    let to = chrono::DateTime::from_timestamp(hour_base + 3600, 0).unwrap();
    let q = StatsQuery {
        metric: "latency".to_owned(),
        label_filter: None,
        from,
        to,
        bucket: StatsBucket::Hour,
    };
    let resp = auth_context::with_context(admin_ctx(), svc.query(q))
        .await
        .unwrap();
    assert_eq!(resp.series.len(), 1);
    assert_eq!(
        resp.series[0].value, 50.0,
        "gauge hour aggregation must take the latest value"
    );
}

#[tokio::test]
async fn query_day_returns_daily_points() {
    let repo = Arc::new(MemoryStatsRepo::default());
    repo.daily.lock().unwrap().push(DailyStatRow {
        metric: "dns.queries".to_owned(),
        labels: "{}".to_owned(),
        day: "2026-01-15".to_owned(),
        value: 100.0,
        kind: "counter".to_owned(),
    });
    let svc = StatsServiceImpl::new(repo);
    let from = chrono::DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    let to = chrono::DateTime::parse_from_rfc3339("2026-01-31T00:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    let q = StatsQuery {
        metric: "dns.queries".to_owned(),
        label_filter: None,
        from,
        to,
        bucket: StatsBucket::Day,
    };
    let resp = auth_context::with_context(admin_ctx(), svc.query(q))
        .await
        .unwrap();
    assert_eq!(resp.series.len(), 1);
    assert_eq!(resp.series[0].value, 100.0);
}

// ── top ───────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn top_returns_forbidden_without_admin_context() {
    let svc = make_service();
    let q = StatsTopQuery {
        metric: "m".to_owned(),
        label_key: "outcome".to_owned(),
        from: Utc::now(),
        to: Utc::now(),
        limit: 5,
    };
    assert!(svc.top(q).await.is_err());
}

#[tokio::test]
async fn top_with_admin_context() {
    let repo = Arc::new(MemoryStatsRepo::default());
    let now_ts = Utc::now().timestamp();
    {
        let mut g = repo.intraday.lock().unwrap();
        g.push(intraday(
            "dns.queries",
            r#"{"domain":"ads.com"}"#,
            now_ts,
            10.0,
        ));
        g.push(intraday(
            "dns.queries",
            r#"{"domain":"tracker.net"}"#,
            now_ts,
            3.0,
        ));
    }
    let svc = StatsServiceImpl::new(repo);
    let q = StatsTopQuery {
        metric: "dns.queries".to_owned(),
        label_key: "domain".to_owned(),
        from: Utc::now() - chrono::Duration::hours(1),
        to: Utc::now() + chrono::Duration::hours(1),
        limit: 5,
    };
    let resp = auth_context::with_context(admin_ctx(), svc.top(q))
        .await
        .unwrap();
    assert_eq!(resp.metric, "dns.queries");
    assert_eq!(resp.entries.len(), 2);
    assert_eq!(resp.entries[0].total, 10.0);
}

// ── run_maintenance ───────────────────────────────────────────────────────────

#[tokio::test]
async fn run_maintenance_completes_without_error() {
    let svc = make_service();
    svc.run_maintenance().await.unwrap();
}
