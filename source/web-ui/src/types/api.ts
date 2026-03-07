export interface LoginRequest {
  username: string;
  password: string;
}

export interface LoginResponse {
  message: string;
}

export interface Device {
  id: string;
  mac: string;
  name: string | null;
  hostname: string | null;
  device_type: string;
  first_seen: string;
  last_seen: string;
  last_ip: string;
  admin_locked: boolean;
}

export interface RoutingTarget {
  type: "tunnel" | "direct" | "default";
  tunnel_id?: string;
}

export interface DeviceMeResponse {
  device: Device | null;
  current_rule: RoutingTarget | null;
  admin_locked: boolean;
}

export interface SetMyRuleRequest {
  target: RoutingTarget;
}

export interface SetMyRuleResponse {
  message: string;
  target: RoutingTarget;
}

export interface SystemStatusResponse {
  version: string;
  uptime_seconds: number;
  device_count: number;
  tunnel_count: number;
  db_size_bytes: number;
}

export interface ApiError {
  error: string;
  detail?: string;
}
