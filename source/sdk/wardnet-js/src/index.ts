// Client
export { WardnetClient, WardnetApiError } from "./client.js";
export type { WardnetClientOptions } from "./client.js";

// Services
export { AuthService } from "./services/auth.js";
export { DeviceService } from "./services/devices.js";
export { TunnelService } from "./services/tunnels.js";
export { ProviderService } from "./services/providers.js";
export { SystemService } from "./services/system.js";
export { SetupService } from "./services/setup.js";
export { InfoService } from "./services/info.js";
export { DhcpService } from "./services/dhcp.js";
export { LogService } from "./services/logs.js";
export type { LogEntry, LogFilter, LogStreamCallbacks } from "./services/logs.js";

// Types — devices
export type {
  Device,
  DeviceType,
  DhcpStatus,
  RoutingTarget,
  RuleCreator,
  RoutingRule,
} from "./types/device.js";

// Types — tunnels
export type { Tunnel, TunnelStatus } from "./types/tunnel.js";

// Types — providers
export type {
  ProviderAuthMethod,
  ProviderInfo,
  ProviderCredentials,
  CountryInfo,
  ServerFilter,
  ServerInfo,
} from "./types/provider.js";

// Types — auth
export type { LoginRequest, LoginResponse } from "./types/auth.js";

// Types — system
export type { SystemStatusResponse } from "./types/system.js";

// Types — setup
export type { SetupStatusResponse, SetupRequest, SetupResponse } from "./types/setup.js";

// Types — info
export type { InfoResponse } from "./types/info.js";

// Types — DHCP
export type {
  DhcpConfig,
  DhcpLease,
  DhcpLeaseStatus,
  DhcpReservation,
  DhcpConfigResponse,
  UpdateDhcpConfigRequest,
  ToggleDhcpRequest,
  ListDhcpLeasesResponse,
  ListDhcpReservationsResponse,
  CreateDhcpReservationRequest,
  CreateDhcpReservationResponse,
  DeleteDhcpReservationResponse,
  DhcpStatusResponse,
  RevokeDhcpLeaseResponse,
} from "./types/dhcp.js";

// Types — API DTOs
export type {
  ApiError,
  DeviceMeResponse,
  SetMyRuleRequest,
  SetMyRuleResponse,
  ListDevicesResponse,
  DeviceDetailResponse,
  UpdateDeviceRequest,
  CreateTunnelRequest,
  CreateTunnelResponse,
  ListTunnelsResponse,
  DeleteTunnelResponse,
  ListProvidersResponse,
  ValidateCredentialsRequest,
  ValidateCredentialsResponse,
  ListServersRequest,
  ListServersResponse,
  ListCountriesResponse,
  SetupProviderRequest,
  SetupProviderResponse,
  TunnelSummary,
} from "./types/api.js";
