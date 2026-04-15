use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// DNS resolution strategy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DnsResolutionMode {
    /// Forward queries to configured upstream DNS servers.
    Forwarding,
    /// Resolve queries recursively starting from root servers.
    Recursive,
}

/// Transport protocol for upstream DNS communication.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DnsProtocol {
    Udp,
    Tcp,
    Tls,
    Https,
}

/// A configured upstream DNS server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpstreamDns {
    /// IP address or hostname (e.g. "1.1.1.1", "dns.google").
    pub address: String,
    /// Human-readable label (e.g. "Cloudflare", "Google").
    pub name: String,
    /// Transport protocol.
    pub protocol: DnsProtocol,
    /// Port (default: 53 for UDP/TCP, 853 for TLS, 443 for HTTPS).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
}

/// Top-level DNS server configuration.
///
/// Persisted as individual keys in the `system_config` KV table,
/// following the same pattern as [`crate::dhcp::DhcpConfig`].
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DnsConfig {
    pub enabled: bool,
    pub resolution_mode: DnsResolutionMode,
    pub upstream_servers: Vec<UpstreamDns>,
    pub cache_size: u32,
    pub cache_ttl_min_secs: u32,
    pub cache_ttl_max_secs: u32,
    pub dnssec_enabled: bool,
    pub rebinding_protection: bool,
    pub rate_limit_per_second: u32,
    pub ad_blocking_enabled: bool,
    pub query_log_enabled: bool,
    pub query_log_retention_days: u32,
}

impl Default for DnsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            resolution_mode: DnsResolutionMode::Forwarding,
            upstream_servers: vec![
                UpstreamDns {
                    address: "1.1.1.1".to_owned(),
                    name: "Cloudflare".to_owned(),
                    protocol: DnsProtocol::Udp,
                    port: None,
                },
                UpstreamDns {
                    address: "8.8.8.8".to_owned(),
                    name: "Google".to_owned(),
                    protocol: DnsProtocol::Udp,
                    port: None,
                },
            ],
            cache_size: 10_000,
            cache_ttl_min_secs: 0,
            cache_ttl_max_secs: 86_400,
            dnssec_enabled: false,
            rebinding_protection: true,
            rate_limit_per_second: 0,
            ad_blocking_enabled: true,
            query_log_enabled: true,
            query_log_retention_days: 7,
        }
    }
}

/// DNS record type for custom local records.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DnsRecordType {
    A,
    Aaaa,
    Cname,
    Txt,
    Mx,
    Srv,
}

/// A user-defined local DNS record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomDnsRecord {
    pub id: Uuid,
    pub zone_id: Option<Uuid>,
    pub domain: String,
    pub record_type: DnsRecordType,
    pub value: String,
    pub ttl: u32,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// An authoritative local DNS zone (e.g. "lab", "home", "local").
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DnsZone {
    pub id: Uuid,
    pub name: String,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A URL-sourced domain blocklist for ad blocking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Blocklist {
    pub id: Uuid,
    pub name: String,
    pub url: String,
    pub enabled: bool,
    pub entry_count: u64,
    pub last_updated: Option<DateTime<Utc>>,
    /// Cron expression for update schedule (e.g. "0 3 * * *").
    pub cron_schedule: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// An allowlist entry that overrides blocklist matches.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AllowlistEntry {
    pub id: Uuid,
    pub domain: String,
    pub reason: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// A user-created AdGuard-syntax filter rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomFilterRule {
    pub id: Uuid,
    pub rule_text: String,
    pub enabled: bool,
    pub comment: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A conditional forwarding rule (domain → specific upstream).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConditionalForwardingRule {
    pub id: Uuid,
    pub domain: String,
    pub upstream: String,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
}

/// Result classification for a DNS query.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DnsQueryResult {
    /// Answered by upstream server.
    Forwarded,
    /// Answered from cache.
    Cached,
    /// Blocked by ad filter.
    Blocked,
    /// Answered by a custom local record or zone.
    Local,
    /// Resolved recursively from root servers.
    Recursive,
    /// Resolution failed.
    Error,
}

/// A single entry in the DNS query log.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DnsQueryLogEntry {
    pub id: i64,
    pub timestamp: DateTime<Utc>,
    pub client_ip: String,
    pub domain: String,
    pub query_type: String,
    pub result: DnsQueryResult,
    pub upstream: Option<String>,
    pub latency_ms: f64,
    pub device_id: Option<Uuid>,
}

/// Aggregated DNS statistics over a time window.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DnsStats {
    pub total_queries: u64,
    pub blocked_queries: u64,
    pub cached_queries: u64,
    pub blocked_percent: f64,
    pub top_domains: Vec<(String, u64)>,
    pub top_blocked: Vec<(String, u64)>,
    pub top_clients: Vec<(String, u64)>,
    /// Hourly query counts: (hour label, count).
    pub queries_over_time: Vec<(String, u64)>,
}
