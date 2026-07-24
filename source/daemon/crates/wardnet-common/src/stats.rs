use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Time-series bucket granularity for stats queries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum StatsBucket {
    Minute,
    Hour,
    Day,
}

/// Parameters for a time-series stats query.
///
/// Exactly one of `metric` or `metrics` must be set. The single-metric
/// form returns a flat series under `series`; the multi-metric form
/// runs one query per metric (sharing the same `label_filter`, `from`,
/// `to`, and `bucket`) and returns a `metric_name -> series` map under
/// `results`. The multi-metric path is provided to let callers fetch
/// multiple related metrics in one round-trip (e.g. the tunnel
/// detail page fetching tx, rx, and `rtt_ms` together).
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct StatsQuery {
    /// Single metric name. Mutually exclusive with `metrics`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metric: Option<String>,
    /// Multiple metric names to fan out over in a single call. Mutually
    /// exclusive with `metric`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metrics: Option<Vec<String>>,
    /// Exact labels JSON string to filter by. `None` returns all label combinations.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label_filter: Option<String>,
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
    pub bucket: StatsBucket,
}

/// A single point in a stats time series.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct StatsSeriesPoint {
    pub ts: DateTime<Utc>,
    pub value: f64,
    /// Sorted JSON labels object for this data point.
    pub labels: String,
}

/// Response for a time-series stats query.
///
/// The shape of the response mirrors the request: a single-metric
/// query returns `metric` + `series`; a multi-metric query returns
/// `results` (metric name → series). Exactly one variant is populated.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct StatsQueryResponse {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metric: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub series: Option<Vec<StatsSeriesPoint>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub results: Option<HashMap<String, Vec<StatsSeriesPoint>>>,
}

/// Parameters for a top-N stats query.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct StatsTopQuery {
    pub metric: String,
    /// Label dimension to rank by (e.g. `"domain"`, `"client"`).
    pub label_key: String,
    /// Optional label dimension to group by when `label_key` is absent
    /// from an entry's labels (e.g. rank by `"device_id"` falling back to
    /// `"client"` for queries from sources with no known device).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback_label_key: Option<String>,
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
    pub limit: u32,
}

/// A single entry in a top-N result.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct StatsTopEntry {
    /// Sorted JSON labels object for this entry.
    pub labels: String,
    pub total: f64,
}

/// Response for a top-N stats query.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct StatsTopResponse {
    pub metric: String,
    pub entries: Vec<StatsTopEntry>,
}
