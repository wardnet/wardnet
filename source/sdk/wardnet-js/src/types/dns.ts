/** DNS transport protocol. */
export type DnsProtocol = "udp" | "tcp" | "tls" | "https";

/** DNS resolution mode. */
export type DnsResolutionMode = "forwarding" | "recursive";

/** A configured upstream DNS server. */
export interface UpstreamDns {
  address: string;
  name: string;
  protocol: DnsProtocol;
  port?: number;
}

/** DNS server configuration. */
export interface DnsConfig {
  enabled: boolean;
  resolution_mode: DnsResolutionMode;
  upstream_servers: UpstreamDns[];
  cache_size: number;
  cache_ttl_min_secs: number;
  cache_ttl_max_secs: number;
  dnssec_enabled: boolean;
  rebinding_protection: boolean;
  rate_limit_per_second: number;
  /** Global emergency stop for DNS filtering. Renamed from `ad_blocking_enabled`. */
  dns_filtering_enabled: boolean;
  query_log_enabled: boolean;
  query_log_retention_days: number;
}

// API request/response types

export interface DnsConfigResponse {
  config: DnsConfig;
}

export interface UpdateDnsConfigRequest {
  resolution_mode?: string;
  upstream_servers?: UpstreamDns[];
  cache_size?: number;
  cache_ttl_min_secs?: number;
  cache_ttl_max_secs?: number;
  dnssec_enabled?: boolean;
  rebinding_protection?: boolean;
  rate_limit_per_second?: number;
  dns_filtering_enabled?: boolean;
  query_log_enabled?: boolean;
  query_log_retention_days?: number;
}

export interface ToggleDnsRequest {
  enabled: boolean;
}

export interface DnsStatusResponse {
  enabled: boolean;
  running: boolean;
  cache_size: number;
  cache_capacity: number;
  cache_hit_rate: number;
}

export interface DnsCacheFlushResponse {
  message: string;
  entries_cleared: number;
}

// ---------------------------------------------------------------------------
// Query log + stats
// ---------------------------------------------------------------------------

/** Result classification for a DNS query.
 *
 *  Each value is the canonical snake_case string written by the DNS resolver to
 *  the database. Both the paginated REST endpoint (`GET /api/dns/log`) and the
 *  live-stream WebSocket (`/api/dns/log/stream`) serialise this enum, so a
 *  given DB row will render the same badge regardless of which path served it.
 *
 *  `blocked_skipped` is recorded when a query *would* have been blocked but the
 *  per-device kill switch (or global emergency stop) suppressed the block. */
export type DnsQueryResult =
  | "forwarded"
  | "cache_hit"
  | "blocked"
  | "blocked_skipped"
  | "rewritten"
  | "recursive"
  | "upstream_error"
  | "authoritative"
  | "error";

/** A single entry in the persisted DNS query log. */
export interface DnsQueryLogEntry {
  id: number;
  timestamp: string;
  client_ip: string;
  domain: string;
  query_type: string;
  result: DnsQueryResult;
  upstream?: string | null;
  latency_ms: number;
  device_id?: string | null;
}

/** Live event broadcast over `/api/dns/log/stream`. Mirrors a query log row. */
export interface QueryLogEvent {
  timestamp: string;
  client_ip: string;
  domain: string;
  query_type: string;
  result: DnsQueryResult;
  upstream?: string | null;
  latency_ms: number;
  device_id?: string | null;
}

export interface ListQueryLogParams {
  limit?: number;
  offset?: number;
  domain?: string;
  client_ip?: string;
  result?: DnsQueryResult;
}

export interface ListQueryLogResponse {
  entries: DnsQueryLogEntry[];
  total: number;
}
