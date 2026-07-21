#![allow(clippy::float_cmp)]

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::Utc;
use uuid::Uuid;
use wardnet_common::auth::AuthContext;
use wardnet_common::stats::{StatsBucket, StatsQuery, StatsTopEntry, StatsTopQuery};
use wardnetd_data::repository::{DailyStatRow, HourlyStatRow, IntradayStatRow, StatsRepository};

use crate::auth_context;
use crate::error::AppError;
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
    hourly: Mutex<Vec<HourlyStatRow>>,
    daily: Mutex<Vec<DailyStatRow>>,
}

#[async_trait]
impl StatsRepository for MemoryStatsRepo {
    async fn upsert_intraday(&self, rows: &[IntradayStatRow]) -> anyhow::Result<()> {
        self.intraday.lock().unwrap().extend_from_slice(rows);
        Ok(())
    }

    async fn trim_intraday(&self, _cutoff_ts: i64) -> anyhow::Result<u64> {
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

    async fn upsert_hourly(&self, rows: &[HourlyStatRow]) -> anyhow::Result<()> {
        self.hourly.lock().unwrap().extend_from_slice(rows);
        Ok(())
    }

    async fn rollup_hourly(&self, _hour_ts: i64) -> anyhow::Result<usize> {
        Ok(0)
    }

    async fn trim_hourly(&self, _cutoff_ts: i64) -> anyhow::Result<u64> {
        Ok(0)
    }

    async fn query_hourly(
        &self,
        metric: &str,
        label_filter: Option<&str>,
        from: i64,
        to: i64,
    ) -> anyhow::Result<Vec<HourlyStatRow>> {
        let guard = self.hourly.lock().unwrap();
        Ok(guard
            .iter()
            .filter(|r| {
                r.metric == metric
                    && r.hour_ts >= from
                    && r.hour_ts <= to
                    && label_filter.is_none_or(|f| r.labels == f)
            })
            .cloned()
            .collect())
    }

    async fn rollup_daily(&self, _day: &str) -> anyhow::Result<usize> {
        Ok(0)
    }

    async fn trim_daily(&self, _cutoff_day: &str) -> anyhow::Result<u64> {
        Ok(0)
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

    #[allow(clippy::too_many_arguments)]
    async fn top_n(
        &self,
        metric: &str,
        label_key: &str,
        fallback_label_key: Option<&str>,
        bucket: StatsBucket,
        from: i64,
        to: i64,
        limit: u32,
    ) -> anyhow::Result<Vec<StatsTopEntry>> {
        // Mirrors the SQL COALESCE semantics: group by `label_key`'s value,
        // falling back to `fallback_label_key`'s; skip entries with neither.
        let extract = |labels: &str, key: &str| -> Option<String> {
            serde_json::from_str::<serde_json::Value>(labels)
                .ok()?
                .get(key)?
                .as_str()
                .map(ToOwned::to_owned)
        };
        // Gather (labels, value) pairs from the tier `bucket` selects, matching
        // the SQL implementation's tier routing.
        let matched: Vec<(String, f64)> = match bucket {
            StatsBucket::Minute => self
                .intraday
                .lock()
                .unwrap()
                .iter()
                .filter(|r| r.metric == metric && r.bucket_ts >= from && r.bucket_ts <= to)
                .map(|r| (r.labels.clone(), r.value))
                .collect(),
            StatsBucket::Hour => self
                .hourly
                .lock()
                .unwrap()
                .iter()
                .filter(|r| r.metric == metric && r.hour_ts >= from && r.hour_ts <= to)
                .map(|r| (r.labels.clone(), r.value))
                .collect(),
            StatsBucket::Day => {
                let from_day = chrono::DateTime::from_timestamp(from, 0)
                    .unwrap_or_default()
                    .date_naive()
                    .to_string();
                let to_day = chrono::DateTime::from_timestamp(to, 0)
                    .unwrap_or_default()
                    .date_naive()
                    .to_string();
                self.daily
                    .lock()
                    .unwrap()
                    .iter()
                    .filter(|r| {
                        r.metric == metric
                            && r.day.as_str() >= from_day.as_str()
                            && r.day.as_str() <= to_day.as_str()
                    })
                    .map(|r| (r.labels.clone(), r.value))
                    .collect()
            }
        };
        let mut totals: std::collections::HashMap<String, (String, f64)> =
            std::collections::HashMap::new();
        for (labels, value) in &matched {
            let key = extract(labels, label_key)
                .or_else(|| fallback_label_key.and_then(|f| extract(labels, f)));
            let Some(key) = key else { continue };
            let entry = totals.entry(key).or_insert_with(|| (labels.clone(), 0.0));
            entry.1 += value;
        }
        let mut entries: Vec<StatsTopEntry> = totals
            .into_values()
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

fn hourly(metric: &str, labels: &str, hour_ts: i64, value: f64, kind: &str) -> HourlyStatRow {
    HourlyStatRow {
        metric: metric.to_owned(),
        labels: labels.to_owned(),
        hour_ts,
        value,
        kind: kind.to_owned(),
    }
}

fn daily(metric: &str, labels: &str, day: &str, value: f64) -> DailyStatRow {
    DailyStatRow {
        metric: metric.to_owned(),
        labels: labels.to_owned(),
        day: day.to_owned(),
        value,
        kind: "counter".to_owned(),
    }
}

// ── run_flush ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn run_flush_empty_is_noop() {
    let svc = make_service();
    auth_context::with_context(admin_ctx(), svc.run_flush(vec![]))
        .await
        .unwrap();
}

#[tokio::test]
async fn run_flush_delegates_rows_to_repo() {
    let repo = Arc::new(MemoryStatsRepo::default());
    let svc = StatsServiceImpl::new(repo.clone());
    auth_context::with_context(
        admin_ctx(),
        svc.run_flush(vec![intraday("m", "{}", 0, 1.0)]),
    )
    .await
    .unwrap();
    assert_eq!(repo.intraday.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn run_flush_forbidden_without_admin_context() {
    let repo = Arc::new(MemoryStatsRepo::default());
    let svc = StatsServiceImpl::new(repo.clone());
    let err = svc
        .run_flush(vec![intraday("m", "{}", 0, 1.0)])
        .await
        .unwrap_err();
    assert!(matches!(
        err.downcast_ref::<AppError>(),
        Some(AppError::Forbidden(_))
    ));
    assert!(
        repo.intraday.lock().unwrap().is_empty(),
        "no rows may be written without an admin context"
    );
}

// ── query ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn query_returns_forbidden_without_admin_context() {
    let svc = make_service();
    let q = StatsQuery {
        metric: Some("m".to_owned()),
        metrics: None,
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
        metric: Some("dns.queries".to_owned()),
        metrics: None,
        label_filter: None,
        from: Utc::now() - chrono::Duration::hours(1),
        to: Utc::now() + chrono::Duration::hours(1),
        bucket: StatsBucket::Minute,
    };
    let resp = auth_context::with_context(admin_ctx(), svc.query(q))
        .await
        .unwrap();
    assert_eq!(resp.metric.as_deref(), Some("dns.queries"));
    assert_eq!(resp.series.as_ref().unwrap().len(), 1);
    assert_eq!(resp.series.as_ref().unwrap()[0].value, 5.0);
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
        metric: Some("m".to_owned()),
        metrics: None,
        label_filter: Some(r#"{"outcome":"blocked"}"#.to_owned()),
        from: Utc::now() - chrono::Duration::hours(1),
        to: Utc::now() + chrono::Duration::hours(1),
        bucket: StatsBucket::Minute,
    };
    let resp = auth_context::with_context(admin_ctx(), svc.query(q))
        .await
        .unwrap();
    assert_eq!(resp.series.as_ref().unwrap().len(), 1);
    assert_eq!(resp.series.as_ref().unwrap()[0].value, 5.0);
}

#[tokio::test]
async fn query_hour_reads_from_hourly_table() {
    let repo = Arc::new(MemoryStatsRepo::default());
    // Seed stats_hourly directly — StatsBucket::Hour now queries that
    // table rather than aggregating from intraday in-memory.
    let hour_base = 1_746_000_000_i64;
    {
        let mut g = repo.hourly.lock().unwrap();
        g.push(hourly("dns.queries", "{}", hour_base, 10.0, "counter"));
        g.push(hourly(
            "dns.queries",
            "{}",
            hour_base + 3600,
            20.0,
            "counter",
        ));
    }
    let svc = StatsServiceImpl::new(repo);
    let from = chrono::DateTime::from_timestamp(hour_base - 1, 0).unwrap();
    let to = chrono::DateTime::from_timestamp(hour_base + 7200, 0).unwrap();
    let q = StatsQuery {
        metric: Some("dns.queries".to_owned()),
        metrics: None,
        label_filter: None,
        from,
        to,
        bucket: StatsBucket::Hour,
    };
    let resp = auth_context::with_context(admin_ctx(), svc.query(q))
        .await
        .unwrap();
    let series = resp.series.as_ref().unwrap();
    assert_eq!(series.len(), 2, "two hourly rows must produce two points");
    assert_eq!(series[0].value, 10.0);
    assert_eq!(series[1].value, 20.0);
}

#[tokio::test]
async fn query_hour_with_label_filter() {
    let repo = Arc::new(MemoryStatsRepo::default());
    let hour_base = 1_746_000_000_i64;
    {
        let mut g = repo.hourly.lock().unwrap();
        g.push(hourly(
            "m",
            r#"{"outcome":"blocked"}"#,
            hour_base,
            5.0,
            "counter",
        ));
        g.push(hourly(
            "m",
            r#"{"outcome":"forwarded"}"#,
            hour_base,
            3.0,
            "counter",
        ));
    }
    let svc = StatsServiceImpl::new(repo);
    let from = chrono::DateTime::from_timestamp(hour_base - 1, 0).unwrap();
    let to = chrono::DateTime::from_timestamp(hour_base + 3600, 0).unwrap();
    let q = StatsQuery {
        metric: Some("m".to_owned()),
        metrics: None,
        label_filter: Some(r#"{"outcome":"blocked"}"#.to_owned()),
        from,
        to,
        bucket: StatsBucket::Hour,
    };
    let resp = auth_context::with_context(admin_ctx(), svc.query(q))
        .await
        .unwrap();
    assert_eq!(resp.series.as_ref().unwrap().len(), 1);
    assert_eq!(resp.series.as_ref().unwrap()[0].value, 5.0);
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
        metric: Some("dns.queries".to_owned()),
        metrics: None,
        label_filter: None,
        from,
        to,
        bucket: StatsBucket::Day,
    };
    let resp = auth_context::with_context(admin_ctx(), svc.query(q))
        .await
        .unwrap();
    assert_eq!(resp.series.as_ref().unwrap().len(), 1);
    assert_eq!(resp.series.as_ref().unwrap()[0].value, 100.0);
}

#[tokio::test]
async fn query_multi_metric_returns_results_map() {
    let repo = Arc::new(MemoryStatsRepo::default());
    let now_ts = Utc::now().timestamp();
    let bucket_ts = now_ts - (now_ts % 60);
    {
        let mut g = repo.intraday.lock().unwrap();
        g.push(intraday(
            "tunnel.bytes.tx",
            r#"{"tunnel_id":"a"}"#,
            bucket_ts,
            100.0,
        ));
        g.push(intraday(
            "tunnel.bytes.rx",
            r#"{"tunnel_id":"a"}"#,
            bucket_ts,
            200.0,
        ));
    }
    let svc = StatsServiceImpl::new(repo);
    let q = StatsQuery {
        metric: None,
        metrics: Some(vec![
            "tunnel.bytes.tx".to_owned(),
            "tunnel.bytes.rx".to_owned(),
        ]),
        label_filter: None,
        from: Utc::now() - chrono::Duration::hours(1),
        to: Utc::now() + chrono::Duration::hours(1),
        bucket: StatsBucket::Minute,
    };
    let resp = auth_context::with_context(admin_ctx(), svc.query(q))
        .await
        .unwrap();
    assert!(resp.metric.is_none());
    assert!(resp.series.is_none());
    let results = resp
        .results
        .expect("multi-metric response populates results");
    assert_eq!(results.len(), 2);
    assert_eq!(results.get("tunnel.bytes.tx").unwrap()[0].value, 100.0);
    assert_eq!(results.get("tunnel.bytes.rx").unwrap()[0].value, 200.0);
}

#[tokio::test]
async fn query_multi_metric_with_empty_list_returns_empty_results() {
    let svc = make_service();
    let q = StatsQuery {
        metric: None,
        metrics: Some(vec![]),
        label_filter: None,
        from: Utc::now() - chrono::Duration::hours(1),
        to: Utc::now() + chrono::Duration::hours(1),
        bucket: StatsBucket::Minute,
    };
    let resp = auth_context::with_context(admin_ctx(), svc.query(q))
        .await
        .unwrap();
    assert!(resp.metric.is_none());
    assert!(resp.series.is_none());
    assert_eq!(resp.results.unwrap().len(), 0);
}

#[tokio::test]
async fn query_rejects_when_both_metric_and_metrics_set() {
    let svc = make_service();
    let q = StatsQuery {
        metric: Some("m".to_owned()),
        metrics: Some(vec!["n".to_owned()]),
        label_filter: None,
        from: Utc::now(),
        to: Utc::now(),
        bucket: StatsBucket::Minute,
    };
    let err = auth_context::with_context(admin_ctx(), svc.query(q))
        .await
        .expect_err("setting both metric and metrics must be a bad request");
    assert!(matches!(err, AppError::BadRequest(_)), "got {err:?}");
}

#[tokio::test]
async fn query_rejects_when_neither_metric_nor_metrics_set() {
    let svc = make_service();
    let q = StatsQuery {
        metric: None,
        metrics: None,
        label_filter: None,
        from: Utc::now(),
        to: Utc::now(),
        bucket: StatsBucket::Minute,
    };
    let err = auth_context::with_context(admin_ctx(), svc.query(q))
        .await
        .expect_err("setting neither metric nor metrics must be a bad request");
    assert!(matches!(err, AppError::BadRequest(_)), "got {err:?}");
}

// ── top ───────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn top_returns_forbidden_without_admin_context() {
    let svc = make_service();
    let q = StatsTopQuery {
        metric: "m".to_owned(),
        label_key: "outcome".to_owned(),
        fallback_label_key: None,
        bucket: None,
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
        fallback_label_key: None,
        bucket: None,
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

#[tokio::test]
async fn top_day_bucket_ranks_over_the_daily_tier() {
    // Rank trackers over a multi-day window: with `bucket: Day` the ranking
    // must read the daily tier, not intraday — the long-window path this
    // enables. Intraday holds a decoy that must be ignored.
    let repo = Arc::new(MemoryStatsRepo::default());
    let today = Utc::now().date_naive().to_string();
    {
        let mut d = repo.daily.lock().unwrap();
        d.push(daily(
            "dns.blocked.by_tracker",
            r#"{"company":"Google"}"#,
            &today,
            40.0,
        ));
        d.push(daily(
            "dns.blocked.by_tracker",
            r#"{"company":"Meta"}"#,
            &today,
            12.0,
        ));
    }
    {
        // A stale intraday row for the same metric must not leak into a
        // daily-tier ranking.
        let mut i = repo.intraday.lock().unwrap();
        i.push(intraday(
            "dns.blocked.by_tracker",
            r#"{"company":"Adobe"}"#,
            Utc::now().timestamp(),
            999.0,
        ));
    }
    let svc = StatsServiceImpl::new(repo);
    let q = StatsTopQuery {
        metric: "dns.blocked.by_tracker".to_owned(),
        label_key: "company".to_owned(),
        fallback_label_key: None,
        bucket: Some(StatsBucket::Day),
        from: Utc::now() - chrono::Duration::days(7),
        to: Utc::now() + chrono::Duration::days(1),
        limit: 5,
    };
    let resp = auth_context::with_context(admin_ctx(), svc.top(q))
        .await
        .unwrap();
    assert_eq!(resp.entries.len(), 2, "only the two daily-tier companies");
    assert_eq!(resp.entries[0].total, 40.0);
    assert!(
        resp.entries[0].labels.contains("Google"),
        "top tracker should be Google, got {}",
        resp.entries[0].labels
    );
}

// ── run_maintenance ───────────────────────────────────────────────────────────

#[tokio::test]
async fn run_maintenance_completes_without_error() {
    let svc = make_service();
    auth_context::with_context(admin_ctx(), svc.run_maintenance())
        .await
        .unwrap();
}

#[tokio::test]
async fn run_maintenance_forbidden_without_admin_context() {
    let svc = make_service();
    let err = svc.run_maintenance().await.unwrap_err();
    assert!(matches!(
        err.downcast_ref::<AppError>(),
        Some(AppError::Forbidden(_))
    ));
}
