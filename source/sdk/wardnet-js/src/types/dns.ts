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
