/** DNS transport protocol. */
export type DnsProtocol = "udp" | "tcp" | "tls" | "https";

/** DNS resolution mode. */
export type DnsResolutionMode = "forwarding" | "recursive";

/**
 * How the configured upstreams are used on the forwarding path (only
 * meaningful in `"forwarding"` mode):
 * - `"failover"` — use servers in listed order; fall back to the next only if
 *   one fails (list order = priority). The default.
 * - `"fastest"` — forward to all upstreams and route to the fastest-responding
 *   one (order ignored).
 * - `"single"` — forward exclusively to {@link DnsConfig.single_upstream}.
 */
export type ForwarderSelectionMode = "failover" | "fastest" | "single";

/** Rolling-average latency telemetry for one configured upstream. */
export interface UpstreamLatency {
  /** The upstream's `address`, matching {@link UpstreamDns.address}. */
  address: string;
  /** Rolling-average round-trip time in ms, or absent until the first probe. */
  avg_latency_ms?: number;
  /** Whether the most recent probe reached the upstream. */
  reachable: boolean;
}

/** A configured upstream DNS server. */
export interface UpstreamDns {
  address: string;
  name: string;
  protocol: DnsProtocol;
  port?: number;
  /**
   * TLS server name (SNI) for DoT/DoH. Required when `protocol` is
   * `"tls"` or `"https"`: `address` is the IP to dial, this is the
   * hostname the upstream's certificate must match.
   */
  tls_server_name?: string;
}

/** DNS server configuration. */
export interface DnsConfig {
  enabled: boolean;
  resolution_mode: DnsResolutionMode;
  upstream_servers: UpstreamDns[];
  /** How the configured upstreams are used on the forwarding path. */
  forwarder_selection_mode: ForwarderSelectionMode;
  /**
   * The chosen single upstream's `address`. Present iff
   * `forwarder_selection_mode` is `"single"`, and always one of the
   * `upstream_servers` addresses.
   */
  single_upstream?: string;
  cache_size: number;
  cache_ttl_min_secs: number;
  cache_ttl_max_secs: number;
  dnssec_enabled: boolean;
  rebinding_protection: boolean;
  rate_limit_per_second: number;
  /**
   * How long a single upstream gets to answer before the forwarder moves on
   * to the next one. Bounds one rung of the ladder, not the whole query.
   */
  upstream_timeout_ms: number;
  /**
   * Wall-clock ceiling on a forwarded query, across every upstream tried.
   * Kept below a client stub resolver's own patience (~5s) so the client is
   * still listening when the answer arrives.
   */
  forward_deadline_ms: number;
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
  resolution_mode?: DnsResolutionMode;
  upstream_servers?: UpstreamDns[];
  forwarder_selection_mode?: ForwarderSelectionMode;
  /**
   * The chosen single upstream's `address`. Only consulted when the resulting
   * mode is `"single"`; ignored (forced clear) in the other modes. Omit to
   * leave the current selection unchanged.
   */
  single_upstream?: string;
  cache_size?: number;
  cache_ttl_min_secs?: number;
  cache_ttl_max_secs?: number;
  dnssec_enabled?: boolean;
  rebinding_protection?: boolean;
  rate_limit_per_second?: number;
  /** Must not exceed `forward_deadline_ms`. */
  upstream_timeout_ms?: number;
  forward_deadline_ms?: number;
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
  /** Rolling-average latency per configured upstream (from the prober). */
  upstream_latencies: UpstreamLatency[];
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
  | "negative"
  | "authoritative"
  | "authoritative_nodata"
  | "authoritative_nxdomain"
  | "rate_limited"
  | "rebinding_blocked"
  | "recursor_failed"
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
  /**
   * Keyset cursor: return the newest entries with an id below this one. Omit
   * for the first page, then pass the previous response's `next_cursor`.
   *
   * There is deliberately no offset. An offset makes the daemon walk and
   * discard every row already read, so page cost grows with depth; a cursor
   * makes every page the same seek.
   */
  before?: number;
  domain?: string;
  client_ip?: string;
  /**
   * Filter by the device attributed at query time (stable across DHCP
   * reassignment, unlike `client_ip`). Rows from unknown sources have no
   * device and only match `client_ip`.
   */
  device_id?: string;
  result?: DnsQueryResult;
}

export interface ListQueryLogResponse {
  entries: DnsQueryLogEntry[];
  /**
   * Whether a further page exists, derived by over-fetching one row beyond
   * the requested limit.
   *
   * There is deliberately no total count: counting the query log has no
   * `LIMIT` to stop at and was measured at ~300 ms per page load. Render a
   * row count from `entries.length` instead.
   */
  has_more: boolean;
  /**
   * Cursor to pass as `before` for the next page. Absent exactly when
   * `has_more` is false — the daemon omits the field rather than sending
   * `null`, so `before: response.next_cursor` round-trips under `strict`.
   *
   * Paging backwards is the caller's job: keep the cursors already used.
   */
  next_cursor?: number;
}
