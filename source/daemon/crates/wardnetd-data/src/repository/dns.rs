//! DNS server data access — query log persistence only.
//!
//! After the Stage 7 split (issue #221), DNS filter sources (blocklists,
//! allowlist, custom rules) and per-device settings live behind
//! [`DnsFilterRepository`](super::dns_filter::DnsFilterRepository). This
//! repository covers query log persistence and pagination.
//!
//! DNS observability stats (totals, top domains, top clients, series) are now
//! handled by the generic stats subsystem (`stats_intraday` / `stats_daily`
//! tables, `StatsRepository`) rather than by on-the-fly aggregations over the
//! query log table.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use wardnet_common::dns::DnsQueryResult;

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

    /// Delete query log entries older than `retention_days`, then prune the
    /// domain lookups the delete orphaned. Returns the query-log rows deleted.
    async fn cleanup_query_log(&self, retention_days: u32) -> anyhow::Result<u64>;
}

// ── Row / update structs ──────────────────────────────────────────────────

/// Row struct for DNS query log inserts.
#[derive(Debug, Clone)]
pub struct QueryLogRow {
    /// Whole seconds. The producer truncates sub-second precision explicitly so
    /// the streamed event never carries a resolution the epoch-second column
    /// cannot return.
    pub timestamp: DateTime<Utc>,
    pub client_ip: String,
    pub domain: String,
    pub query_type: String,
    pub result: String,
    pub upstream: Option<String>,
    pub latency_ms: f64,
    pub device_id: Option<String>,
    /// Transport the query arrived over: `"udp"` (classic `:53`) or `"dot"`
    /// (the `:853` DNS-over-TLS listener, issue #912).
    pub protocol: String,
}

/// Filters for query log pagination.
#[derive(Debug, Clone, Default)]
pub struct QueryLogFilter {
    pub client_ip: Option<String>,
    /// Exact match on the write-time device attribution (stable across
    /// DHCP reassignment, unlike `client_ip`).
    pub device_id: Option<String>,
    pub domain: Option<String>,
    pub result: Option<DnsQueryResult>,
}
