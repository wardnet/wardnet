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
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema, utoipa::IntoParams)]
pub struct StatsQuery {
    pub metric: String,
    /// Exact labels JSON string to filter by. `None` returns all label combinations.
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
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct StatsQueryResponse {
    pub metric: String,
    pub series: Vec<StatsSeriesPoint>,
}

/// Parameters for a top-N stats query.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema, utoipa::IntoParams)]
pub struct StatsTopQuery {
    pub metric: String,
    /// Label dimension to rank by (e.g. `"domain"`, `"client"`).
    pub label_key: String,
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
