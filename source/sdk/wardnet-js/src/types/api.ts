import type { Device, DeviceType, RoutingTarget } from "./device.js";
import type { TunnelStatus } from "./tunnel.js";
import type {
  CountryInfo,
  ProviderCredentials,
  ProviderInfo,
  ServerFilter,
  ServerInfo,
} from "./provider.js";
import type { Tunnel } from "./tunnel.js";

/** Standard API error response. */
export interface ApiError {
  error: string;
  detail?: string;
  /** Server-generated request ID for correlating with server logs. */
  request_id?: string;
}

/** Minimal tunnel info for self-service routing selection. */
export interface TunnelSummary {
  id: string;
  label: string;
  country_code: string;
  status: TunnelStatus;
  last_handshake: string | null;
}

/** Response for GET /api/devices/me. */
export interface DeviceMeResponse {
  device: Device | null;
  current_rule: RoutingTarget | null;
  admin_locked: boolean;
  available_tunnels: TunnelSummary[];
}

/** Request body for PUT /api/devices/me/rule. */
export interface SetMyRuleRequest {
  target: RoutingTarget;
}

/** Response body for PUT /api/devices/me/rule. */
export interface SetMyRuleResponse {
  message: string;
  target: RoutingTarget;
}

/** Response for GET /api/devices (admin). */
export interface ListDevicesResponse {
  devices: Device[];
}

/** Response for GET /api/devices/:id (admin). */
export interface DeviceDetailResponse {
  device: Device;
  current_rule: RoutingTarget | null;
}

/** Request body for PUT /api/devices/:id (admin). */
export interface UpdateDeviceRequest {
  name?: string;
  device_type?: DeviceType;
  routing_target?: RoutingTarget;
  admin_locked?: boolean;
}

/** Request body for POST /api/tunnels. */
export interface CreateTunnelRequest {
  label: string;
  country_code: string;
  provider?: string;
  config: string;
}

/** Response for POST /api/tunnels. */
export interface CreateTunnelResponse {
  tunnel: Tunnel;
  message: string;
}

/** Response for GET /api/tunnels. */
export interface ListTunnelsResponse {
  tunnels: Tunnel[];
}

/** Response for DELETE /api/tunnels/:id. */
export interface DeleteTunnelResponse {
  message: string;
}

/** Response for POST /api/tunnels/:id/rebuild. */
export interface RebuildTunnelResponse {
  ok: boolean;
}

/** Response for GET /api/tunnels/:id. */
export interface TunnelDetailResponse {
  tunnel: Tunnel;
}

/** Response for GET /api/tunnels/:id/devices. */
export interface TunnelDevicesResponse {
  devices: Device[];
}

/** Result payload of POST /api/tunnels/:id/test. */
export interface TunnelTestResult {
  tunnel_id: string;
  /** Public IP observed at the tunnel exit. */
  exit_ip: string;
  /** ISO-3166 alpha-2 country code reported by the probe service. */
  country_code: string;
  /** Round-trip latency of the probe call, in milliseconds. */
  latency_ms: number;
}

/** Response envelope for POST /api/tunnels/:id/test. */
export interface TunnelTestResponse {
  result: TunnelTestResult;
}

/**
 * One completed speed test: the direct (WAN) baseline and the tunnel result,
 * measured back-to-back so retention (how much of the line the VPN keeps) is
 * an apples-to-apples comparison.
 */
export interface TunnelSpeedTestResult {
  id: string;
  tunnel_id: string;
  /** Download throughput over the direct (unbound/WAN) path, megabits/s. */
  direct_throughput_mbps: number;
  /** Download throughput through the tunnel interface, megabits/s. */
  tunnel_throughput_mbps: number;
  /** Median round-trip latency over the direct path, milliseconds. */
  direct_latency_ms: number;
  /** Median round-trip latency through the tunnel, milliseconds. */
  tunnel_latency_ms: number;
  /** Latency jitter (sample stddev) over the direct path, milliseconds. */
  direct_jitter_ms: number;
  /** Latency jitter (sample stddev) through the tunnel, milliseconds. */
  tunnel_jitter_ms: number;
  /** ISO 8601 timestamp of when the test completed. */
  tested_at: string;
}

/** Response for GET /api/tunnels/:id/speed-test/results — newest first. */
export interface TunnelSpeedTestHistoryResponse {
  results: TunnelSpeedTestResult[];
}

/** Request body for PUT /api/tunnels/:id/dns-override. */
export interface UpdateTunnelDnsOverrideRequest {
  /**
   * `true` routes tunneled-device DNS through wardnet (so the ad-blocking
   * filter still runs) using the tunnel's DNS server as upstream with
   * `SO_BINDTODEVICE`. `false` falls back to the system-wide upstream
   * pool. See issue #342.
   */
  override_default_dns: boolean;
}

/** Response for PUT /api/tunnels/:id/dns-override. */
export interface UpdateTunnelDnsOverrideResponse {
  tunnel: Tunnel;
}

/** Response for GET /api/providers. */
export interface ListProvidersResponse {
  providers: ProviderInfo[];
}

/** Request body for POST /api/providers/:id/validate. */
export interface ValidateCredentialsRequest {
  credentials: ProviderCredentials;
}

/** Response for POST /api/providers/:id/validate. */
export interface ValidateCredentialsResponse {
  valid: boolean;
  message: string;
}

/** Request body for POST /api/providers/:id/servers. */
export interface ListServersRequest {
  credentials: ProviderCredentials;
  filter?: ServerFilter;
}

/** Response for POST /api/providers/:id/servers. */
export interface ListServersResponse {
  servers: ServerInfo[];
}

/** Response for GET /api/providers/:id/countries. */
export interface ListCountriesResponse {
  countries: CountryInfo[];
}

/** Request body for POST /api/providers/:id/setup. */
export interface SetupProviderRequest {
  credentials: ProviderCredentials;
  country?: string;
  label?: string;
  server_id?: string;
  hostname?: string;
}

/** Response for POST /api/providers/:id/setup. */
export interface SetupProviderResponse {
  tunnel: Tunnel;
  server: ServerInfo;
  message: string;
}

/** Request body for PATCH /api/devices/:id/dns-capture. */
export interface DnsCaptureSettingsRequest {
  enabled?: boolean;
  cap_count?: number;
  cap_days?: number;
}

/** Request body for PATCH /api/devices/me/dns-capture (self-service toggle). */
export interface DeviceCaptureToggleRequest {
  enabled: boolean;
}

/** Response for GET/PATCH /api/devices/:id/dns-capture. */
export interface DnsCaptureSettingsResponse {
  enabled: boolean;
  cap_count: number;
  cap_days: number;
  row_count: number;
  size_bytes: number;
}

/** What a device rule request is asking for. */
export type RuleRequestKind = "block" | "allow";

/** Lifecycle state of a device rule request. */
export type RuleRequestStatus = "pending" | "approved" | "rejected";

/** A device's request for the admin to block or allow a domain. */
export interface DeviceRuleRequest {
  id: string;
  device_id: string;
  kind: RuleRequestKind;
  domain: string;
  reason: string | null;
  status: RuleRequestStatus;
  created_at: string;
  decided_at: string | null;
  decided_by: string | null;
}

/** Request body for POST /api/devices/me/rule-requests. */
export interface CreateRuleRequestRequest {
  kind: RuleRequestKind;
  domain: string;
  reason?: string | null;
}

/** Request body for PATCH /api/rule-requests/:id (admin decision). */
export interface DecideRuleRequestRequest {
  status: RuleRequestStatus;
}

export interface DnsEventItem {
  id: number;
  domain: string;
  status: string;
  captured_at: string;
}

export interface DnsEventsAckRequest {
  up_to_id: number;
}
