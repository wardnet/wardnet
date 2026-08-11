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
  /**
   * Whether an admin has decided to control this device's configuration
   * (issue #1181).
   *
   * Set by *any* admin configuration act — naming, locking, a routing rule or
   * profile, DNS filter settings, DNS capture, a Private-DNS grant, a Remote
   * peer credential, a DHCP reservation, a zone exception, an explicit zone
   * reassignment. Deliberately **not** derived from `name`: a device can be
   * configured without ever being named, so rendering managed-ness from the
   * name alone is wrong in both directions.
   *
   * Latching — it never clears on its own, only through
   * {@link DeviceService.release}. Only unmanaged devices are subject to
   * device retention (deleted after 30 days away).
   */
  managed: boolean;
}

/**
 * The kind of an identification signal (issue #1099).
 *
 * - `dhcp_hostname` — DHCP option 12, the hostname the device asked for.
 * - `dhcp_param_list` — DHCP option 55; the *ordering* is a device-class
 *   fingerprint.
 * - `dhcp_vendor_class` — DHCP option 60, often a literal brand string.
 * - `mdns_service` — a service type the device advertised over mDNS.
 * - `probed_port` — a TCP port that answered an admin-triggered probe.
 */
export type DeviceSignalKind =
  "dhcp_hostname" | "dhcp_param_list" | "dhcp_vendor_class" | "mdns_service" | "probed_port";

/**
 * A single observed fact that helps identify a device (issue #1099).
 *
 * Multi-valued per device: each signal is independent evidence rather than a
 * field that overwrites the last one.
 */
export interface DeviceSignal {
  kind: DeviceSignalKind;
  value: string;
  /**
   * `true` when the value matched a vendor in the curated catalog.
   *
   * Deliberately *not* "this signal named the device": naming is
   * first-writer-wins against a device with no manufacturer yet, so a device
   * already named by its IEEE registrant collects matching signals that changed
   * nothing — and one device can match two vendors at once.
   */
  inferred: boolean;
  observed_at: string;
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
