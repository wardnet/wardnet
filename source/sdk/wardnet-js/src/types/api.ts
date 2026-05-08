import type { Device, DeviceType, RoutingTarget } from "./device.js";
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

/** Response for GET /api/tunnels/:id. */
export interface TunnelDetailResponse {
  tunnel: Tunnel;
}

/** Range selector for GET /api/tunnels/:id/metrics. */
export type TunnelMetricsRange = "1h" | "6h" | "24h" | "48h" | "12mo";

/** One throughput point. `bytes_tx`/`bytes_rx` are deltas over the interval ending at `ts`. */
export interface TunnelMetricsPoint {
  ts: string;
  bytes_tx: number;
  bytes_rx: number;
}

/** Response for GET /api/tunnels/:id/metrics. */
export interface TunnelMetricsResponse {
  range: TunnelMetricsRange;
  /** Sample interval (5 min for intraday, 86400 for daily). */
  interval_secs: number;
  points: TunnelMetricsPoint[];
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
