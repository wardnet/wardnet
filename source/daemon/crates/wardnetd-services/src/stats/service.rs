use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Datelike, Duration, NaiveDate, TimeZone, Utc};
use wardnet_common::stats::{
    StatsBucket, StatsQuery, StatsQueryResponse, StatsSeriesPoint, StatsTopQuery, StatsTopResponse,
};
use wardnetd_data::repository::{DailyStatRow, IntradayStatRow, StatsRepository};

use crate::auth_context;
use crate::error::AppError;

/// Intraday retention window: 48 hours.
const INTRADAY_RETENTION: Duration = Duration::hours(48);

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

    /// Rollup yesterday into `stats_daily` and trim past-retention rows.
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
        let series = match q.bucket {
            StatsBucket::Day => query_daily(self.repo.as_ref(), &q).await?,
            StatsBucket::Minute => query_intraday_minute(self.repo.as_ref(), &q).await?,
            StatsBucket::Hour => query_intraday_hour(self.repo.as_ref(), &q).await?,
        };
        Ok(StatsQueryResponse {
            metric: q.metric,
            series,
        })
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

        // Roll up every complete day that has intraday data but no daily row yet.
        // Walk from 13 months ago to yesterday (today is still accumulating).
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

        // Trim intraday rows older than 48 h.
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

        // Trim daily rows older than 13 months.
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

async fn query_intraday_minute(
    repo: &dyn StatsRepository,
    q: &StatsQuery,
) -> Result<Vec<StatsSeriesPoint>, AppError> {
    let from = q.from.timestamp();
    let to = q.to.timestamp();
    let rows = repo
        .query_intraday(&q.metric, q.label_filter.as_deref(), from, to)
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

async fn query_intraday_hour(
    repo: &dyn StatsRepository,
    q: &StatsQuery,
) -> Result<Vec<StatsSeriesPoint>, AppError> {
    let from = q.from.timestamp();
    let to = q.to.timestamp();
    let rows = repo
        .query_intraday(&q.metric, q.label_filter.as_deref(), from, to)
        .await
        .map_err(AppError::Internal)?;

    // Aggregate per-minute rows into per-hour buckets.
    // Key: (hour_ts, labels). Hour_ts = minute_ts truncated to hour boundary.
    let mut buckets: HashMap<(i64, String), (f64, String)> = HashMap::new();
    for row in rows {
        let hour_ts = row.bucket_ts - (row.bucket_ts % 3600);
        let entry = buckets
            .entry((hour_ts, row.labels.clone()))
            .or_insert((0.0, row.kind.clone()));
        if row.kind == "gauge" {
            // Gauge: take the latest value (rows are ordered by bucket_ts ASC,
            // so each successive row is more recent).
            entry.0 = row.value;
        } else {
            entry.0 += row.value;
        }
    }

    let mut points: Vec<StatsSeriesPoint> = buckets
        .into_iter()
        .map(|((hour_ts, labels), (value, _kind))| StatsSeriesPoint {
            ts: bucket_ts_to_dt(hour_ts),
            value,
            labels,
        })
        .collect();
    points.sort_by_key(|p| p.ts);
    Ok(points)
}

async fn query_daily(
    repo: &dyn StatsRepository,
    q: &StatsQuery,
) -> Result<Vec<StatsSeriesPoint>, AppError> {
    let from = q.from.date_naive().to_string();
    let to = q.to.date_naive().to_string();
    let rows = repo
        .query_daily(&q.metric, q.label_filter.as_deref(), &from, &to)
        .await
        .map_err(AppError::Internal)?;
    Ok(rows.into_iter().map(day_row_to_point).collect())
}

fn bucket_ts_to_dt(ts: i64) -> DateTime<Utc> {
    Utc.timestamp_opt(ts, 0)
        .single()
        .unwrap_or_else(|| Utc.timestamp_opt(0, 0).single().expect("epoch is valid"))
}

fn day_row_to_point(r: DailyStatRow) -> StatsSeriesPoint {
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
