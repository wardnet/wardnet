use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Datelike, Duration, NaiveDate, TimeZone, Utc};
use wardnet_common::stats::{
    StatsBucket, StatsQuery, StatsQueryResponse, StatsSeriesPoint, StatsTopQuery, StatsTopResponse,
};
use wardnetd_data::repository::{IntradayStatRow, StatsRepository};

use crate::auth_context;
use crate::error::AppError;

/// Intraday retention: 25 hours. Intraday rows older than this are
/// trimmed during maintenance; all queries for older data must use
/// the hourly tier.
const INTRADAY_RETENTION: Duration = Duration::hours(25);

/// Hourly retention: 8 days.
const HOURLY_RETENTION: Duration = Duration::hours(8 * 24);

/// Daily retention window: 13 months (≈ 397 days).
const DAILY_RETENTION_DAYS: i64 = 397;

#[async_trait]
pub trait StatsService: Send + Sync {
    /// Query a time series for `q.metric`, optionally filtered by `q.label_filter`.
    async fn query(&self, q: StatsQuery) -> Result<StatsQueryResponse, AppError>;

    /// Return the top-N label values ranked by total value for `q.metric`.
    async fn top(&self, q: StatsTopQuery) -> Result<StatsTopResponse, AppError>;

    /// Flush a batch of pre-drained buffer rows into `stats_intraday`.
    async fn run_flush(&self, rows: Vec<IntradayStatRow>) -> anyhow::Result<()>;

    /// Rollup intraday → hourly → daily and trim past-retention rows.
    async fn run_maintenance(&self) -> anyhow::Result<()>;
}

pub struct StatsServiceImpl {
    repo: Arc<dyn StatsRepository>,
}

impl StatsServiceImpl {
    #[must_use]
    pub fn new(repo: Arc<dyn StatsRepository>) -> Self {
        Self { repo }
    }
}

#[async_trait]
impl StatsService for StatsServiceImpl {
    async fn query(&self, q: StatsQuery) -> Result<StatsQueryResponse, AppError> {
        auth_context::require_admin()?;

        match (q.metric.as_ref(), q.metrics.as_ref()) {
            (Some(_), Some(_)) => Err(AppError::BadRequest(
                "stats query: pass either `metric` or `metrics`, not both".to_owned(),
            )),
            (None, None) => Err(AppError::BadRequest(
                "stats query: one of `metric` or `metrics` is required".to_owned(),
            )),
            (Some(metric), None) => {
                let series = run_query(self.repo.as_ref(), &q, metric).await?;
                Ok(StatsQueryResponse {
                    metric: Some(metric.clone()),
                    series: Some(series),
                    results: None,
                })
            }
            (None, Some(metrics)) => {
                let mut results = HashMap::with_capacity(metrics.len());
                for metric in metrics {
                    let series = run_query(self.repo.as_ref(), &q, metric).await?;
                    results.insert(metric.clone(), series);
                }
                Ok(StatsQueryResponse {
                    metric: None,
                    series: None,
                    results: Some(results),
                })
            }
        }
    }

    async fn top(&self, q: StatsTopQuery) -> Result<StatsTopResponse, AppError> {
        auth_context::require_admin()?;
        let from = q.from.timestamp();
        let to = q.to.timestamp();
        let entries = self
            .repo
            .top_n(&q.metric, &q.label_key, from, to, q.limit)
            .await
            .map_err(AppError::Internal)?;
        Ok(StatsTopResponse {
            metric: q.metric,
            entries,
        })
    }

    async fn run_flush(&self, rows: Vec<IntradayStatRow>) -> anyhow::Result<()> {
        if rows.is_empty() {
            return Ok(());
        }
        self.repo.upsert_intraday(&rows).await
    }

    async fn run_maintenance(&self) -> anyhow::Result<()> {
        let now = Utc::now();

        // ── Hourly rollup ─────────────────────────────────────────────────────
        // Walk every complete hour within the hourly retention window and
        // roll up intraday minute rows into stats_hourly. INSERT OR IGNORE
        // makes repeated calls idempotent — after the initial backfill each
        // maintenance tick only adds the one hour that just completed.
        let current_hour_ts = (now.timestamp() / 3600) * 3600;
        let hourly_walk_start = ((now - HOURLY_RETENTION).timestamp() / 3600) * 3600;
        let mut hour_ts = hourly_walk_start;
        while hour_ts < current_hour_ts {
            match self.repo.rollup_hourly(hour_ts).await {
                Ok(n) if n > 0 => {
                    tracing::debug!(
                        hour_ts,
                        rows = n,
                        "stats hourly rollup: hour_ts={hour_ts}, rows={n}"
                    );
                }
                Ok(_) => {}
                Err(e) => {
                    tracing::warn!(hour_ts, error = %e, "stats hourly rollup failed: hour_ts={hour_ts}, error={e}");
                }
            }
            hour_ts += 3600;
        }

        // Trim hourly rows outside the 8 d retention window.
        let hourly_cutoff = (now - HOURLY_RETENTION).timestamp();
        match self.repo.trim_hourly(hourly_cutoff).await {
            Ok(n) if n > 0 => {
                tracing::info!(deleted = n, "stats hourly trim: deleted={n}");
            }
            Ok(_) => {}
            Err(e) => {
                tracing::warn!(error = %e, "stats hourly trim failed: {e}");
            }
        }

        // ── Daily rollup ──────────────────────────────────────────────────────
        // Roll up every complete day that has intraday data but no daily
        // row yet. Walk from 13 months ago to yesterday (today is still
        // accumulating).
        let today = now.date_naive();
        let retention_start = today - chrono::Duration::days(DAILY_RETENTION_DAYS);
        let mut day = retention_start;
        while day < today {
            let day_str = day.to_string();
            match self.repo.rollup_daily(&day_str).await {
                Ok(n) if n > 0 => {
                    tracing::debug!(day = %day_str, rows = n, "stats daily rollup: day={day_str}, rows={n}");
                }
                Ok(_) => {}
                Err(e) => {
                    tracing::warn!(day = %day_str, error = %e, "stats daily rollup failed: day={day_str}, error={e}");
                }
            }
            day += chrono::Duration::days(1);
        }

        // ── Trim intraday ─────────────────────────────────────────────────────
        let intraday_cutoff = (now - INTRADAY_RETENTION).timestamp();
        match self.repo.trim_intraday(intraday_cutoff).await {
            Ok(n) if n > 0 => {
                tracing::info!(deleted = n, "stats intraday trim: deleted={n}");
            }
            Ok(_) => {}
            Err(e) => {
                tracing::warn!(error = %e, "stats intraday trim failed: {e}");
            }
        }

        // ── Trim daily ────────────────────────────────────────────────────────
        let daily_cutoff = (today - chrono::Duration::days(DAILY_RETENTION_DAYS)).to_string();
        match self.repo.trim_daily(&daily_cutoff).await {
            Ok(n) if n > 0 => {
                tracing::info!(deleted = n, "stats daily trim: deleted={n}");
            }
            Ok(_) => {}
            Err(e) => {
                tracing::warn!(error = %e, "stats daily trim failed: {e}");
            }
        }

        Ok(())
    }
}

// ── Query helpers ─────────────────────────────────────────────────────────────

/// Run a single-metric query against `repo` honouring `q.bucket`.
async fn run_query(
    repo: &dyn StatsRepository,
    q: &StatsQuery,
    metric: &str,
) -> Result<Vec<StatsSeriesPoint>, AppError> {
    match q.bucket {
        StatsBucket::Day => query_daily(repo, q, metric).await,
        StatsBucket::Minute => query_intraday_minute(repo, q, metric).await,
        StatsBucket::Hour => query_hourly(repo, q, metric).await,
    }
}

async fn query_intraday_minute(
    repo: &dyn StatsRepository,
    q: &StatsQuery,
    metric: &str,
) -> Result<Vec<StatsSeriesPoint>, AppError> {
    let from = q.from.timestamp();
    let to = q.to.timestamp();
    let rows = repo
        .query_intraday(metric, q.label_filter.as_deref(), from, to)
        .await
        .map_err(AppError::Internal)?;
    Ok(rows
        .into_iter()
        .map(|r| StatsSeriesPoint {
            ts: bucket_ts_to_dt(r.bucket_ts),
            value: r.value,
            labels: r.labels,
        })
        .collect())
}

async fn query_hourly(
    repo: &dyn StatsRepository,
    q: &StatsQuery,
    metric: &str,
) -> Result<Vec<StatsSeriesPoint>, AppError> {
    let from = q.from.timestamp();
    let to = q.to.timestamp();
    let rows = repo
        .query_hourly(metric, q.label_filter.as_deref(), from, to)
        .await
        .map_err(AppError::Internal)?;
    Ok(rows
        .into_iter()
        .map(|r| StatsSeriesPoint {
            ts: bucket_ts_to_dt(r.hour_ts),
            value: r.value,
            labels: r.labels,
        })
        .collect())
}

async fn query_daily(
    repo: &dyn StatsRepository,
    q: &StatsQuery,
    metric: &str,
) -> Result<Vec<StatsSeriesPoint>, AppError> {
    let from = q.from.date_naive().to_string();
    let to = q.to.date_naive().to_string();
    let rows = repo
        .query_daily(metric, q.label_filter.as_deref(), &from, &to)
        .await
        .map_err(AppError::Internal)?;
    Ok(rows.into_iter().map(day_row_to_point).collect())
}

fn bucket_ts_to_dt(ts: i64) -> DateTime<Utc> {
    Utc.timestamp_opt(ts, 0)
        .single()
        .unwrap_or_else(|| Utc.timestamp_opt(0, 0).single().expect("epoch is valid"))
}

fn day_row_to_point(r: wardnetd_data::repository::DailyStatRow) -> StatsSeriesPoint {
    let naive = NaiveDate::parse_from_str(&r.day, "%Y-%m-%d")
        .unwrap_or_else(|_| NaiveDate::from_ymd_opt(1970, 1, 1).expect("valid"));
    let ts = Utc
        .with_ymd_and_hms(naive.year(), naive.month(), naive.day(), 0, 0, 0)
        .single()
        .unwrap_or_else(|| Utc.timestamp_opt(0, 0).single().expect("epoch is valid"));
    StatsSeriesPoint {
        ts,
        value: r.value,
        labels: r.labels,
    }
}

/// Expose `StatsTopEntry` for API consumers.
pub use wardnet_common::stats::StatsTopEntry as TopEntry;
