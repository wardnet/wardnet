//! DNS server data access — query log + stats only.
//!
//! After the Stage 7 split (issue #221), DNS filter sources (blocklists,
//! allowlist, custom rules) and per-device settings live behind
//! [`DnsFilterRepository`](super::dns_filter::DnsFilterRepository). This
//! repository covers what's left: the query log persistence batch and the
//! aggregations that drive the DNS observability UI.

use async_trait::async_trait;
use chrono::{DateTime, Utc};

#[async_trait]
pub trait DnsRepository: Send + Sync {
    /// Batch-insert query log entries.
    async fn insert_query_log_batch(&self, entries: &[QueryLogRow]) -> anyhow::Result<()>;

    /// Paginated query log with optional filters.
    async fn query_log_paginated(
        &self,
        limit: u32,
        offset: u32,
        filter: &QueryLogFilter,
    ) -> anyhow::Result<Vec<QueryLogRow>>;

    /// Count matching query log entries.
    async fn query_log_count(&self, filter: &QueryLogFilter) -> anyhow::Result<u64>;

    /// Delete query log entries older than `retention_days`.
    async fn cleanup_query_log(&self, retention_days: u32) -> anyhow::Result<u64>;

    /// Aggregate totals for entries since `since`.
    async fn query_stats(&self, since: DateTime<Utc>) -> anyhow::Result<QueryStatsRow>;

    /// Top domains by hit count since `since`. When `blocked_only` is true,
    /// counts only entries with `result = 'blocked'`.
    async fn top_domains(
        &self,
        since: DateTime<Utc>,
        limit: u32,
        blocked_only: bool,
    ) -> anyhow::Result<Vec<TopDomainRow>>;

    /// Top client IPs by query count since `since`.
    async fn top_clients(
        &self,
        since: DateTime<Utc>,
        limit: u32,
    ) -> anyhow::Result<Vec<TopClientRow>>;

    /// Time-bucketed series of total / blocked counts since `since`.
    async fn series_buckets(
        &self,
        since: DateTime<Utc>,
        bucket: BucketSize,
    ) -> anyhow::Result<Vec<SeriesBucketRow>>;
}

// ── Row / update structs ──────────────────────────────────────────────────

/// Row struct for DNS query log inserts.
#[derive(Debug, Clone)]
pub struct QueryLogRow {
    pub timestamp: String,
    pub client_ip: String,
    pub domain: String,
    pub query_type: String,
    pub result: String,
    pub upstream: Option<String>,
    pub latency_ms: f64,
    pub device_id: Option<String>,
}

/// Filters for query log pagination.
#[derive(Debug, Clone, Default)]
pub struct QueryLogFilter {
    pub client_ip: Option<String>,
    pub domain: Option<String>,
    pub result: Option<String>,
}

/// Aggregated counters returned by `DnsRepository::query_stats`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct QueryStatsRow {
    pub total_queries: u64,
    pub blocked_queries: u64,
    pub avg_latency_ms: f64,
    pub unique_clients: u64,
    pub unique_domains: u64,
}

/// Row returned by `DnsRepository::top_domains`.
#[derive(Debug, Clone)]
pub struct TopDomainRow {
    pub domain: String,
    pub count: u64,
}

/// Row returned by `DnsRepository::top_clients`.
#[derive(Debug, Clone)]
pub struct TopClientRow {
    pub client_ip: String,
    pub count: u64,
}

/// Time-bucket size for `DnsRepository::series_buckets`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BucketSize {
    Minute,
    Hour,
}

impl BucketSize {
    /// `SQLite` `strftime` format string for this bucket size.
    #[must_use]
    pub fn strftime_fmt(self) -> &'static str {
        match self {
            Self::Minute => "%Y-%m-%d %H:%M",
            Self::Hour => "%Y-%m-%d %H",
        }
    }
}

/// Row returned by `DnsRepository::series_buckets`.
#[derive(Debug, Clone)]
pub struct SeriesBucketRow {
    pub bucket: String,
    pub total: u64,
    pub blocked: u64,
}
