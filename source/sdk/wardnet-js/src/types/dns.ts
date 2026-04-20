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
  ad_blocking_enabled: boolean;
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
  ad_blocking_enabled?: boolean;
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
// Ad Blocking — domain types
// ---------------------------------------------------------------------------

/** A URL-sourced domain blocklist. */
export interface Blocklist {
  id: string;
  name: string;
  url: string;
  enabled: boolean;
  entry_count: number;
  last_updated: string | null;
  cron_schedule: string;
  last_error: string | null;
  last_error_at: string | null;
  created_at: string;
  updated_at: string;
}

/** An allowlist entry that overrides blocklist matches. */
export interface AllowlistEntry {
  id: string;
  domain: string;
  reason: string | null;
  created_at: string;
}

/** A user-created AdGuard-syntax filter rule. */
export interface CustomFilterRule {
  id: string;
  rule_text: string;
  enabled: boolean;
  comment: string | null;
  created_at: string;
  updated_at: string;
}

// ---------------------------------------------------------------------------
// Ad Blocking — API request/response types
// ---------------------------------------------------------------------------

export interface ListBlocklistsResponse {
  blocklists: Blocklist[];
}

export interface CreateBlocklistRequest {
  name: string;
  url: string;
  cron_schedule: string;
  enabled: boolean;
}

export interface CreateBlocklistResponse {
  blocklist: Blocklist;
  message: string;
}

export interface UpdateBlocklistRequest {
  name?: string;
  url?: string;
  cron_schedule?: string;
  enabled?: boolean;
}

export interface UpdateBlocklistResponse {
  blocklist: Blocklist;
  message: string;
}

export interface DeleteBlocklistResponse {
  message: string;
}

// Response for POST /api/dns/blocklists/{id}/update is now
// `JobDispatchedResponse` from ./jobs.ts — the handler dispatches a
// background job and the client polls `/api/jobs/:id` for progress.

export interface ListAllowlistResponse {
  entries: AllowlistEntry[];
}

export interface CreateAllowlistRequest {
  domain: string;
  reason?: string;
}

export interface CreateAllowlistResponse {
  entry: AllowlistEntry;
  message: string;
}

export interface DeleteAllowlistResponse {
  message: string;
}

export interface ListFilterRulesResponse {
  rules: CustomFilterRule[];
}

export interface CreateFilterRuleRequest {
  rule_text: string;
  comment?: string;
  enabled: boolean;
}

export interface CreateFilterRuleResponse {
  rule: CustomFilterRule;
  message: string;
}

export interface UpdateFilterRuleRequest {
  rule_text?: string;
  comment?: string;
  enabled?: boolean;
}

export interface UpdateFilterRuleResponse {
  rule: CustomFilterRule;
  message: string;
}

export interface DeleteFilterRuleResponse {
  message: string;
}
