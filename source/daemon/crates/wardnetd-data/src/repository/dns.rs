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

    /// One page of the query log, newest first, with optional filters.
    ///
    /// `before` is a keyset cursor: the page holds the newest `limit` rows
    /// whose id is below it, and `None` starts at the newest row. Pagination
    /// is forward-only by construction — an offset makes the database walk and
    /// discard every row the caller already read, so page cost grows with
    /// depth, whereas a cursor turns every page into the same seek.
    async fn query_log_paginated(
        &self,
        limit: u32,
        before: Option<i64>,
        filter: &QueryLogFilter,
    ) -> anyhow::Result<Vec<QueryLogPageRow>>;

    /// Delete query log entries older than `retention_days`, then make a
    /// best-effort attempt to prune the domain lookups the delete orphaned.
    ///
    /// Returns the query-log rows deleted. The retention delete commits before
    /// the prune runs, so a prune failure is logged and the count still
    /// returned rather than reported as a failed cleanup — the consequence is
    /// orphans surviving a tick, not lost retention.
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

/// A stored query-log row, paired with the id that addresses it.
///
/// Distinct from [`QueryLogRow`], which is what a caller hands to
/// `insert_query_log_batch`, where an id would be a value nothing can supply:
/// the column is assigned by the insert.
#[derive(Debug, Clone)]
pub struct QueryLogPageRow {
    /// Rowid of the entry. Descending id is the log's newest-first order, so
    /// this doubles as the keyset cursor for the page that follows.
    pub id: i64,
    pub entry: QueryLogRow,
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
