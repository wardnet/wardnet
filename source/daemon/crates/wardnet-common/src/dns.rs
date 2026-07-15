use std::net::IpAddr;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Identifies the upstream resolver pool a query should be forwarded to.
///
/// `Default` routes a query through the system-wide upstream servers
/// configured in [`DnsConfig::upstream_servers`]. `Tunnel(_)` routes a
/// query through the resolver bound to that tunnel's interface — this is
/// the variant that makes tunneled-device queries egress via the tunnel
/// rather than leaking to the ISP via the default route.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum UpstreamId {
    /// System-wide DNS upstream pool.
    Default,
    /// Per-tunnel DNS upstream pool, identified by the tunnel UUID.
    Tunnel(Uuid),
}

/// DNS resolution strategy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum DnsResolutionMode {
    /// Forward queries to configured upstream DNS servers.
    Forwarding,
    /// Resolve queries recursively starting from root servers.
    Recursive,
}

impl DnsResolutionMode {
    /// The `snake_case` wire string, matching the serde representation.
    /// Used to persist the mode in the `system_config` KV store.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Forwarding => "forwarding",
            Self::Recursive => "recursive",
        }
    }
}

/// How the configured upstreams are used on the forwarding path
/// (only meaningful in [`DnsResolutionMode::Forwarding`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ForwarderSelectionMode {
    /// Use the upstreams in their listed order: send to the first, and only
    /// fall back to the next if it fails. The list order (priority) matters.
    /// This is the default.
    #[default]
    Failover,
    /// Forward to all configured upstreams and let the resolver route to the
    /// fastest-responding one (by live decayed round-trip time). Order is
    /// ignored.
    Fastest,
    /// Forward exclusively to a single chosen upstream (see
    /// [`DnsConfig::single_upstream`]); the others are not used.
    Single,
}

impl ForwarderSelectionMode {
    /// The `snake_case` wire string, matching the serde representation.
    /// Used to persist the mode in the `system_config` KV store.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Failover => "failover",
            Self::Fastest => "fastest",
            Self::Single => "single",
        }
    }

    /// Parse the `snake_case` wire string, falling back to the default
    /// ([`Self::Failover`]) for unknown/legacy values.
    #[must_use]
    pub fn from_wire(s: &str) -> Self {
        match s {
            "fastest" => Self::Fastest,
            "single" => Self::Single,
            _ => Self::Failover,
        }
    }
}

/// Transport protocol for upstream DNS communication.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum DnsProtocol {
    Udp,
    Tcp,
    Tls,
    Https,
}

/// A configured upstream DNS server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
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
    /// TLS server name (SNI) for DoT/DoH. Required when `protocol` is
    /// `Tls`/`Https`, ignored otherwise: `address` is the IP to dial, and
    /// this is the hostname the upstream's certificate must match (e.g.
    /// address `"1.1.1.1"`, `tls_server_name` `"cloudflare-dns.com"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tls_server_name: Option<String>,
}

/// Rolling-average latency telemetry for one configured upstream, produced by
/// the background latency prober and surfaced in the DNS status response so the
/// UI can show per-server performance. Not persisted.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct UpstreamLatency {
    /// The upstream's `address`, matching [`UpstreamDns::address`].
    pub address: String,
    /// Rolling-average round-trip time in milliseconds, or `None` until the
    /// first successful probe.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avg_latency_ms: Option<f64>,
    /// Whether the most recent probe reached the upstream.
    pub reachable: bool,
}

/// Top-level DNS server configuration.
///
/// Persisted as individual keys in the `system_config` KV table,
/// following the same pattern as [`crate::dhcp::DhcpConfig`].
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct DnsConfig {
    pub enabled: bool,
    pub resolution_mode: DnsResolutionMode,
    pub upstream_servers: Vec<UpstreamDns>,
    /// How the configured upstreams are used on the forwarding path.
    pub forwarder_selection_mode: ForwarderSelectionMode,
    /// The `address` of the single chosen upstream. `Some` iff
    /// `forwarder_selection_mode` is [`ForwarderSelectionMode::Single`], and
    /// always one of the `upstream_servers` addresses.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub single_upstream: Option<String>,
    pub cache_size: u32,
    pub cache_ttl_min_secs: u32,
    pub cache_ttl_max_secs: u32,
    pub dnssec_enabled: bool,
    pub rebinding_protection: bool,
    pub rate_limit_per_second: u32,
    /// Global DNS filtering emergency stop. When `false`, every query
    /// short-circuits to `Pass` regardless of per-device or per-profile
    /// state — the kill switch above the kill switch.
    pub dns_filtering_enabled: bool,
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
                    tls_server_name: None,
                },
                UpstreamDns {
                    address: "8.8.8.8".to_owned(),
                    name: "Google".to_owned(),
                    protocol: DnsProtocol::Udp,
                    port: None,
                    tls_server_name: None,
                },
                UpstreamDns {
                    address: "9.9.9.9".to_owned(),
                    name: "Quad9".to_owned(),
                    protocol: DnsProtocol::Udp,
                    port: None,
                    tls_server_name: None,
                },
            ],
            forwarder_selection_mode: ForwarderSelectionMode::Failover,
            single_upstream: None,
            cache_size: 10_000,
            cache_ttl_min_secs: 0,
            cache_ttl_max_secs: 86_400,
            dnssec_enabled: false,
            rebinding_protection: true,
            rate_limit_per_second: 0,
            dns_filtering_enabled: true,
            query_log_enabled: true,
            query_log_retention_days: 7,
        }
    }
}

/// DNS record type for custom local records.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DnsRecordType {
    A,
    Aaaa,
    Cname,
    Txt,
    Mx,
    Srv,
}

/// Provenance of a custom local DNS record.
///
/// `Manual` records are admin-created via the API. `Dhcp` records are
/// auto-registered by [`crate::event::WardnetEvent::DhcpLeaseAssigned`] /
/// `DhcpLeaseRenewed` (the `{hostname}.lan` integration). `System` is
/// reserved for daemon-seeded records that must not be hand-edited.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum DnsRecordSource {
    Manual,
    Dhcp,
    System,
}

/// A user-defined local DNS record.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CustomDnsRecord {
    pub id: Uuid,
    pub zone_id: Option<Uuid>,
    pub domain: String,
    pub record_type: DnsRecordType,
    pub value: String,
    pub ttl: u32,
    pub enabled: bool,
    pub source: DnsRecordSource,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Provenance of an authoritative local DNS zone.
///
/// `Manual` zones are admin-created via the API and freely deletable. `System`
/// zones are daemon-seeded (currently only the `.lan` zone) and **cannot be
/// deleted** — `DnsLocalService::delete_zone` rejects them. Zones are never
/// DHCP-sourced, so (unlike [`DnsRecordSource`]) there is no `Dhcp` variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum DnsZoneSource {
    Manual,
    System,
}

/// An authoritative local DNS zone (e.g. "lab", "home", "local").
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct DnsZone {
    pub id: Uuid,
    pub name: String,
    pub enabled: bool,
    pub source: DnsZoneSource,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A URL-sourced domain blocklist scoped to a DNS filter profile.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct Blocklist {
    pub id: Uuid,
    pub profile_id: Uuid,
    pub name: String,
    pub url: String,
    pub enabled: bool,
    pub entry_count: u64,
    pub last_updated: Option<DateTime<Utc>>,
    /// Cron expression for update schedule (e.g. "0 3 * * *").
    pub cron_schedule: String,
    /// Error message from the most recent failed download/parse, if any.
    /// Cleared on a successful refresh. Surfaced in the Ad Blocking UI so
    /// users can diagnose blocklist issues without inspecting daemon logs.
    pub last_error: Option<String>,
    /// Timestamp of the most recent failed download/parse.
    pub last_error_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// An allowlist entry, scoped to a DNS filter profile, that overrides
/// blocklist matches.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct AllowlistEntry {
    pub id: Uuid,
    pub profile_id: Uuid,
    pub domain: String,
    pub reason: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// A user-created AdGuard-syntax filter rule, scoped to a DNS filter profile.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CustomFilterRule {
    pub id: Uuid,
    pub profile_id: Uuid,
    pub rule_text: String,
    pub enabled: bool,
    pub comment: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A conditional forwarding rule (domain → specific upstream).
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ConditionalForwardingRule {
    pub id: Uuid,
    pub domain: String,
    pub upstream: String,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
}

/// Result classification for a DNS query.
///
/// Each variant's `snake_case` serialisation exactly matches the string the DNS
/// resolver writes to the `dns_query_log.result` column, so no translation is
/// needed between the DB and the API.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum DnsQueryResult {
    /// Answered by upstream server.
    Forwarded,
    /// Answered from cache — resolver string `"cache_hit"`.
    CacheHit,
    /// Blocked by the DNS filter.
    Blocked,
    /// Filter would have blocked the query, but the per-device kill switch
    /// (or the global emergency stop) suppressed the block. Surfaced in the
    /// query log so admins can see what filtering would catch.
    BlockedSkipped,
    /// Answered by a custom local record or zone — resolver string `"rewritten"`.
    Rewritten,
    /// Resolved recursively from root servers.
    Recursive,
    /// Upstream resolver returned an error — resolver string `"upstream_error"`.
    UpstreamError,
    /// Resolved successfully with a negative answer: the name (NXDOMAIN) or
    /// the requested record type (NODATA) does not exist — resolver string
    /// `"negative"`.
    Negative,
    /// Answered directly from a local authoritative record — resolver string `"authoritative"`.
    Authoritative,
    /// Local authoritative zone answered NODATA: the name exists but has no
    /// record of the requested type — resolver string `"authoritative_nodata"`.
    AuthoritativeNodata,
    /// Local authoritative zone answered NXDOMAIN: the name does not exist in
    /// the zone — resolver string `"authoritative_nxdomain"`.
    AuthoritativeNxdomain,
    /// Dropped by the per-client query rate limiter — resolver string
    /// `"rate_limited"`.
    RateLimited,
    /// DNS-rebinding protection refused an answer that pointed at a private IP
    /// for an external domain — resolver string `"rebinding_blocked"`.
    RebindingBlocked,
    /// Recursive resolution failed — resolver string `"recursor_failed"`.
    RecursorFailed,
    /// Resolution failed (parse fallback for unrecognised strings).
    Error,
}

impl DnsQueryResult {
    /// Every variant, for exhaustive iteration.
    ///
    /// Kept honest by [`slot`](Self::slot): that match is exhaustive, so a new
    /// variant fails to compile until it is given a slot here, and
    /// `all_is_complete` asserts the slots and this array agree. Without that
    /// pairing this array would be a comment, not a guarantee — and every test
    /// that iterates it would quietly stop covering the new variant.
    pub const ALL: [Self; 15] = [
        Self::Forwarded,
        Self::CacheHit,
        Self::Blocked,
        Self::BlockedSkipped,
        Self::Rewritten,
        Self::Recursive,
        Self::UpstreamError,
        Self::Negative,
        Self::Authoritative,
        Self::AuthoritativeNodata,
        Self::AuthoritativeNxdomain,
        Self::RateLimited,
        Self::RebindingBlocked,
        Self::RecursorFailed,
        Self::Error,
    ];

    /// This variant's index in [`ALL`](Self::ALL).
    ///
    /// The only reason this exists: the match is exhaustive, so adding a
    /// variant to the enum without adding it to `ALL` is a compile error here
    /// rather than a silently under-covering test suite.
    #[must_use]
    pub const fn slot(self) -> usize {
        match self {
            Self::Forwarded => 0,
            Self::CacheHit => 1,
            Self::Blocked => 2,
            Self::BlockedSkipped => 3,
            Self::Rewritten => 4,
            Self::Recursive => 5,
            Self::UpstreamError => 6,
            Self::Negative => 7,
            Self::Authoritative => 8,
            Self::AuthoritativeNodata => 9,
            Self::AuthoritativeNxdomain => 10,
            Self::RateLimited => 11,
            Self::RebindingBlocked => 12,
            Self::RecursorFailed => 13,
            Self::Error => 14,
        }
    }

    /// Return the canonical `snake_case` string that the DNS resolver writes to
    /// the `dns_query_log.result` column. Inverse of [`DnsQueryResult::parse`].
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Forwarded => "forwarded",
            Self::CacheHit => "cache_hit",
            Self::Blocked => "blocked",
            Self::BlockedSkipped => "blocked_skipped",
            Self::Rewritten => "rewritten",
            Self::Recursive => "recursive",
            Self::UpstreamError => "upstream_error",
            Self::Negative => "negative",
            Self::Authoritative => "authoritative",
            Self::AuthoritativeNodata => "authoritative_nodata",
            Self::AuthoritativeNxdomain => "authoritative_nxdomain",
            Self::RateLimited => "rate_limited",
            Self::RebindingBlocked => "rebinding_blocked",
            Self::RecursorFailed => "recursor_failed",
            Self::Error => "error",
        }
    }

    /// Parse a raw DB / resolver string, returning `None` when it matches no
    /// known variant. Exact inverse of [`as_str`](Self::as_str).
    ///
    /// This is the single source of truth for the accepted strings;
    /// [`parse`](Self::parse) layers the lossy fallback on top. Keeping the
    /// two apart is what lets the tests tell "parsed via a real arm" apart
    /// from "fell through to `Error`" — the two are indistinguishable by
    /// return value alone, which is how the missing `"error"` arm hid behind a
    /// passing round-trip test.
    #[must_use]
    pub fn from_db_str(s: &str) -> Option<Self> {
        match s {
            "forwarded" => Some(Self::Forwarded),
            "cache_hit" => Some(Self::CacheHit),
            "blocked" => Some(Self::Blocked),
            "blocked_skipped" => Some(Self::BlockedSkipped),
            "rewritten" => Some(Self::Rewritten),
            "recursive" => Some(Self::Recursive),
            "upstream_error" => Some(Self::UpstreamError),
            "negative" => Some(Self::Negative),
            "authoritative" => Some(Self::Authoritative),
            "authoritative_nodata" => Some(Self::AuthoritativeNodata),
            "authoritative_nxdomain" => Some(Self::AuthoritativeNxdomain),
            "rate_limited" => Some(Self::RateLimited),
            "rebinding_blocked" => Some(Self::RebindingBlocked),
            "recursor_failed" => Some(Self::RecursorFailed),
            "error" => Some(Self::Error),
            _ => None,
        }
    }

    /// Parse a raw DB / resolver string into the correct variant.
    ///
    /// Unknown strings emit a [`tracing::warn!`] and fall back to
    /// [`DnsQueryResult::Error`] so callers always get a typed value.
    ///
    /// This warning used to fire constantly, because the resolver wrote five
    /// strings (`rate_limited`, `rebinding_blocked`, `recursor_failed`,
    /// `authoritative_nodata`, `authoritative_nxdomain`) that had no variant —
    /// so those queries were mislabelled `Error` in the UI and in the stats.
    /// The strings are variants now, and the right response to the noise was to
    /// fix the drift, not to lower the level: a fallback here means the
    /// resolver is writing a string this enum does not know, which mislabels
    /// rows and needs fixing. Keep it loud.
    ///
    /// Use [`from_db_str`](Self::from_db_str) when you need to distinguish an
    /// unknown string from a genuine `Error` row.
    #[must_use]
    pub fn parse(s: &str) -> Self {
        Self::from_db_str(s).unwrap_or_else(|| {
            tracing::warn!(result = s, "unknown DNS result string; defaulting to Error");
            Self::Error
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_upstreams_include_quad9() {
        let cfg = DnsConfig::default();
        let addrs: Vec<&str> = cfg
            .upstream_servers
            .iter()
            .map(|u| u.address.as_str())
            .collect();
        assert!(addrs.contains(&"1.1.1.1"), "Cloudflare present");
        assert!(addrs.contains(&"8.8.8.8"), "Google present");
        assert!(
            addrs.contains(&"9.9.9.9"),
            "Quad9 present on fresh installs"
        );
        let quad9 = cfg
            .upstream_servers
            .iter()
            .find(|u| u.address == "9.9.9.9")
            .expect("quad9");
        assert_eq!(quad9.name, "Quad9");
        assert_eq!(quad9.protocol, DnsProtocol::Udp);
    }

    #[test]
    fn default_forwarder_selection_is_failover() {
        let cfg = DnsConfig::default();
        assert_eq!(
            cfg.forwarder_selection_mode,
            ForwarderSelectionMode::Failover
        );
        assert_eq!(cfg.single_upstream, None);
    }

    #[test]
    fn forwarder_selection_mode_str_matches_serde() {
        // as_str() must match the snake_case wire form used for KV persistence.
        for m in [
            ForwarderSelectionMode::Failover,
            ForwarderSelectionMode::Fastest,
            ForwarderSelectionMode::Single,
        ] {
            assert_eq!(
                serde_json::to_string(&m).unwrap(),
                format!("\"{}\"", m.as_str())
            );
            // from_wire round-trips through as_str.
            assert_eq!(ForwarderSelectionMode::from_wire(m.as_str()), m);
        }
        // Unknown/legacy values fall back to the default.
        assert_eq!(
            ForwarderSelectionMode::from_wire("auto"),
            ForwarderSelectionMode::Failover
        );
    }

    #[test]
    fn parse_all_resolver_strings() {
        assert_eq!(
            DnsQueryResult::parse("forwarded"),
            DnsQueryResult::Forwarded
        );
        assert_eq!(DnsQueryResult::parse("cache_hit"), DnsQueryResult::CacheHit);
        assert_eq!(DnsQueryResult::parse("blocked"), DnsQueryResult::Blocked);
        assert_eq!(
            DnsQueryResult::parse("blocked_skipped"),
            DnsQueryResult::BlockedSkipped
        );
        assert_eq!(
            DnsQueryResult::parse("rewritten"),
            DnsQueryResult::Rewritten
        );
        assert_eq!(
            DnsQueryResult::parse("recursive"),
            DnsQueryResult::Recursive
        );
        assert_eq!(
            DnsQueryResult::parse("upstream_error"),
            DnsQueryResult::UpstreamError
        );
        assert_eq!(
            DnsQueryResult::parse("authoritative"),
            DnsQueryResult::Authoritative
        );
    }

    #[test]
    fn parse_unknown_falls_back_to_error() {
        assert_eq!(DnsQueryResult::parse("gibberish"), DnsQueryResult::Error);
        assert_eq!(DnsQueryResult::parse(""), DnsQueryResult::Error);
        // Old aliases no longer accepted — the migration removes those rows.
        assert_eq!(DnsQueryResult::parse("cached"), DnsQueryResult::Error);
        assert_eq!(DnsQueryResult::parse("local"), DnsQueryResult::Error);
    }

    #[test]
    fn as_str_round_trips_through_parse() {
        for v in DnsQueryResult::ALL {
            assert_eq!(DnsQueryResult::parse(v.as_str()), v);
        }
    }

    /// `Error` round-trips through a real match arm, not the unknown-string
    /// fallback.
    ///
    /// Without this, `as_str_round_trips_through_parse` passes vacuously for
    /// `Error`: the fallback also returns `Error`, so a missing `"error"` arm
    /// satisfies the assertion while silently routing every `"error"` row
    /// through the "unknown DNS result string" path.
    #[test]
    fn every_variant_parses_without_hitting_the_fallback() {
        for v in DnsQueryResult::ALL {
            assert_eq!(
                DnsQueryResult::from_db_str(v.as_str()),
                Some(v),
                "`{}` has no match arm — it would only survive the round-trip \
                 via the unknown-string fallback",
                v.as_str()
            );
        }
    }

    #[test]
    fn from_db_str_rejects_unknown_strings() {
        assert_eq!(DnsQueryResult::from_db_str("gibberish"), None);
        assert_eq!(DnsQueryResult::from_db_str(""), None);
    }

    /// `ALL` really does hold every variant, exactly once.
    ///
    /// `slot()` is an exhaustive match, so a new variant cannot compile without
    /// being given a slot; this asserts that slot lines up with a distinct
    /// entry in `ALL`. Together they make `ALL` a guarantee rather than a
    /// promise — every test that iterates it covers the new variant on day one.
    #[test]
    fn all_is_complete() {
        for (i, v) in DnsQueryResult::ALL.into_iter().enumerate() {
            assert_eq!(v.slot(), i, "ALL[{i}] = {v:?} is in the wrong slot");
        }
    }
}

/// A single entry in the DNS query log.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
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

/// Action returned by the DNS filter for a given query.
///
/// Returned by `DnsFilter::check` to tell the server how to respond.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case", tag = "action")]
pub enum FilterAction {
    /// Allow the query to proceed (cache lookup + upstream resolution).
    Pass,
    /// Block the query — server returns NXDOMAIN.
    Block,
    /// Synthesize a response with the given IP address (`$dnsrewrite`).
    Rewrite {
        #[schema(value_type = String)]
        ip: IpAddr,
    },
}

/// Aggregated DNS statistics over a time window.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
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
