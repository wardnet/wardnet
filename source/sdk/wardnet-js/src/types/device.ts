/** The type/category of a network device. */
export type DeviceType =
  | "tv"
  | "phone"
  | "laptop"
  | "tablet"
  | "game_console"
  | "settop_box"
  | "iot"
  | "unknown";

/** A discovered network device. */
export interface Device {
  id: string;
  mac: string;
  name: string | null;
  hostname: string | null;
  manufacturer: string | null;
  device_type: DeviceType;
  first_seen: string;
  last_seen: string;
  last_ip: string;
  admin_locked: boolean;
}

/** Where a device's traffic is routed. */
export type RoutingTarget =
  | { type: "tunnel"; tunnel_id: string }
  | { type: "direct" }
  | { type: "default" };

/** Who created the routing rule. */
export type RuleCreator = "admin" | "user";

/** A per-device routing rule. */
export interface RoutingRule {
  device_id: string;
  target: RoutingTarget;
  created_by: RuleCreator;
}
