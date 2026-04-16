use async_trait::async_trait;
use uuid::Uuid;

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

    // -----------------------------------------------------------------------
    // Blocklists (Stage 2)
    // -----------------------------------------------------------------------

    /// List all blocklists.
    async fn list_blocklists(&self) -> anyhow::Result<Vec<wardnet_types::dns::Blocklist>>;

    /// Get a single blocklist by ID.
    async fn get_blocklist(
        &self,
        id: Uuid,
    ) -> anyhow::Result<Option<wardnet_types::dns::Blocklist>>;

    /// Create a new blocklist.
    async fn create_blocklist(&self, row: &BlocklistRow) -> anyhow::Result<()>;

    /// Partially update a blocklist.
    async fn update_blocklist(&self, id: Uuid, row: &BlocklistUpdate) -> anyhow::Result<()>;

    /// Delete a blocklist. Returns `true` if a row was deleted.
    async fn delete_blocklist(&self, id: Uuid) -> anyhow::Result<bool>;

    /// Replace all blocked domains for a blocklist (atomic).
    /// Returns the number of domains inserted.
    async fn replace_blocklist_domains(&self, id: Uuid, domains: &[String]) -> anyhow::Result<u64>;

    /// List all blocked domains from enabled blocklists.
    async fn list_all_blocked_domains_for_enabled(&self) -> anyhow::Result<Vec<String>>;

    /// Set or clear the last error for a blocklist.
    async fn set_blocklist_error(&self, id: Uuid, error: Option<&str>) -> anyhow::Result<()>;

    // -----------------------------------------------------------------------
    // Allowlist (Stage 2)
    // -----------------------------------------------------------------------

    /// List all allowlist entries.
    async fn list_allowlist(&self) -> anyhow::Result<Vec<wardnet_types::dns::AllowlistEntry>>;

    /// Create a new allowlist entry.
    async fn create_allowlist_entry(&self, row: &AllowlistRow) -> anyhow::Result<()>;

    /// Delete an allowlist entry. Returns `true` if a row was deleted.
    async fn delete_allowlist_entry(&self, id: Uuid) -> anyhow::Result<bool>;

    // -----------------------------------------------------------------------
    // Custom rules (Stage 2)
    // -----------------------------------------------------------------------

    /// List all custom filter rules.
    async fn list_custom_rules(&self) -> anyhow::Result<Vec<wardnet_types::dns::CustomFilterRule>>;

    /// Get a single custom filter rule by ID.
    async fn get_custom_rule(
        &self,
        id: Uuid,
    ) -> anyhow::Result<Option<wardnet_types::dns::CustomFilterRule>>;

    /// Create a new custom filter rule.
    async fn create_custom_rule(&self, row: &CustomRuleRow) -> anyhow::Result<()>;

    /// Partially update a custom filter rule.
    async fn update_custom_rule(&self, id: Uuid, row: &CustomRuleUpdate) -> anyhow::Result<()>;

    /// Delete a custom filter rule. Returns `true` if a row was deleted.
    async fn delete_custom_rule(&self, id: Uuid) -> anyhow::Result<bool>;
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

/// Row struct for blocklist inserts.
#[derive(Debug, Clone)]
pub struct BlocklistRow {
    pub id: String,
    pub name: String,
    pub url: String,
    pub enabled: bool,
    pub cron_schedule: String,
}

/// Row struct for blocklist partial updates.
#[derive(Debug, Clone, Default)]
pub struct BlocklistUpdate {
    pub name: Option<String>,
    pub url: Option<String>,
    pub enabled: Option<bool>,
    pub cron_schedule: Option<String>,
}

/// Row struct for allowlist entry inserts.
#[derive(Debug, Clone)]
pub struct AllowlistRow {
    pub id: String,
    pub domain: String,
    pub reason: Option<String>,
}

/// Row struct for custom rule inserts.
#[derive(Debug, Clone)]
pub struct CustomRuleRow {
    pub id: String,
    pub rule_text: String,
    pub enabled: bool,
    pub comment: Option<String>,
}

/// Row struct for custom rule partial updates.
#[derive(Debug, Clone, Default)]
pub struct CustomRuleUpdate {
    pub rule_text: Option<String>,
    pub enabled: Option<bool>,
    pub comment: Option<String>,
}
