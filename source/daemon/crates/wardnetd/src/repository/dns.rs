use async_trait::async_trait;

/// Data access for DNS server state.
///
/// Stage 1 uses `SystemConfigRepository` for DNS config (KV pattern).
/// This trait covers structured DNS data: blocklists, records, zones,
/// rules, query log, and per-device overrides.
#[async_trait]
pub trait DnsRepository: Send + Sync {
    // -----------------------------------------------------------------------
    // Query log (Stage 6)
    // -----------------------------------------------------------------------

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
}

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
