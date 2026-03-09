// Client
export { WardnetClient, WardnetApiError } from "./client.js";
export type { WardnetClientOptions } from "./client.js";

// Services
export { AuthService } from "./services/auth.js";
export { DeviceService } from "./services/devices.js";
export { TunnelService } from "./services/tunnels.js";
export { SystemService } from "./services/system.js";
export { SetupService } from "./services/setup.js";
export { InfoService } from "./services/info.js";

// Types — devices
export type {
  Device,
  DeviceType,
  RoutingTarget,
  RuleCreator,
  RoutingRule,
} from "./types/device.js";

// Types — tunnels
export type { Tunnel, TunnelStatus } from "./types/tunnel.js";

// Types — auth
export type { LoginRequest, LoginResponse } from "./types/auth.js";

// Types — system
export type { SystemStatusResponse } from "./types/system.js";

// Types — setup
export type {
  SetupStatusResponse,
  SetupRequest,
  SetupResponse,
} from "./types/setup.js";

// Types — info
export type { InfoResponse } from "./types/info.js";

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
} from "./types/api.js";
