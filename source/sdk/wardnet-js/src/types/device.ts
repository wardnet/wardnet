/** DHCP status for a device. */
export type DhcpStatus = "lease" | "reservation" | "external";

/**
 * How a device is currently reachable on the network: over the LAN, or via
 * the inbound WireGuard server. A live status (last-observation-wins), not a
 * record of how the device was first discovered. See issue #810.
 */
export type DeviceConnectionMode = "lan" | "remote";

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

/**
 * Provenance of a device's manufacturer name (issue #1099).
 *
 * - `ieee` — the registrant on record; stated as fact.
 * - `catalog` — our curated mapping for an OUI the IEEE lists as `Private`;
 *   rendered as "likely <vendor>" because we are asserting something the
 *   registrant chose not to publish.
 * - `signal` — inferred from something the device announced or answered.
 */
export type ManufacturerSource = "ieee" | "catalog" | "signal";

/** A discovered network device. */
export interface Device {
  id: string;
  mac: string;
  name: string | null;
  hostname: string | null;
  manufacturer: string | null;
  /**
   * Where {@link Device.manufacturer} came from, which is what licenses the UI
   * to state it as fact or hedge it. `null` exactly when `manufacturer` is
   * `null`. See issue #1099.
   */
  manufacturer_source: ManufacturerSource | null;
  /**
   * Whether {@link Device.mac} is locally administered (a privacy/randomized
   * address). Deliberately separate from `manufacturer`: it says how the device
   * presents itself, not who built it.
   */
  is_randomized: boolean;
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
  /** How the device is currently reachable (LAN vs. inbound WireGuard). */
  connection_mode: DeviceConnectionMode;
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
