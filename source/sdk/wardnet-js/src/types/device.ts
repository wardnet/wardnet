/** DHCP status for a device. */
export type DhcpStatus = "lease" | "reservation" | "external";

/** The type/category of a network device. */
export type DeviceType =
  | "tv"
  | "phone"
  | "laptop"
  | "tablet"
  | "game_console"
  | "settop_box"
  | "iot"
  | "router"
  | "managed_switch"
  | "server"
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
  /** The Network Zone this device belongs to (exactly one). See issue #735. */
  zone_id: string;
  dns_capture_enabled: boolean;
  dns_capture_cap_count: number;
  dns_capture_cap_days: number;
  dhcp_status: DhcpStatus;
  /**
   * The device's current routing target. `null` when the device has no rule of
   * its own and follows the gateway default policy; `{ type: "default" }` is an
   * explicit persisted default choice.
   */
  current_rule: RoutingTarget | null;
}

/** Where a device's traffic is routed. */
export type RoutingTarget =
  { type: "tunnel"; tunnel_id: string } | { type: "direct" } | { type: "default" };

/** Who created the routing rule. */
export type RuleCreator = "admin" | "user";

/** A per-device routing rule. */
export interface RoutingRule {
  device_id: string;
  target: RoutingTarget;
  created_by: RuleCreator;
}
