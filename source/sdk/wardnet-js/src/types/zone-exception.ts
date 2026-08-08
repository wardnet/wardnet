/**
 * Cross-zone exceptions (epic #244, issue #737).
 *
 * An admin-granted allowance for one endpoint (a device or a whole zone) to
 * reach another across an otherwise-isolated zone boundary — e.g. a phone
 * casting to a TV in the IoT zone. The daemon emits the allow-rules ahead of
 * the cross-subnet default-deny.
 */

/** Whether an exception endpoint names a single device or a whole zone. */
export type ExceptionEndpointKind = "device" | "zone";

/** One side of an exception: a device (`/32`) or a zone (its subnet). */
export interface ExceptionEndpoint {
  kind: ExceptionEndpointKind;
  /** Device id or zone id, matching `kind`. */
  id: string;
}

/** Transport protocol for a port match. */
export type Proto = "tcp" | "udp";

/** A port or inclusive port range for one protocol (`from === to` = single). */
export interface PortSpec {
  proto: Proto;
  from: number;
  to: number;
}

/**
 * A built-in named service set.
 * - `casting` = mDNS + SSDP/DLNA + Chromecast + AirPlay control ports
 *   (receiver-pull: only the sender's control channel crosses the boundary).
 * - `mirroring` = **all** ports, TCP and UDP, between the two endpoints
 *   (sender-push: media ports are negotiated dynamically). Device-to-device
 *   only — the daemon rejects a zone-scoped mirroring exception.
 * - `smart_home` = vendor LAN-control IoT: Govee, Tuya, LIFX, ESPHome, local
 *   MQTT. Deliberately excludes TCP 80/443 — use an explicit port list for
 *   vendors whose local control is plain HTTP.
 */
export type ServiceSet = "casting" | "mirroring" | "smart_home";

/** Either a named preset or an explicit custom port list. */
export type ServiceSpec =
  { type: "preset"; set: ServiceSet } | { type: "ports"; ports: PortSpec[] };

/** A cross-zone exception. */
export interface ZoneException {
  id: string;
  from: ExceptionEndpoint;
  to: ExceptionEndpoint;
  service: ServiceSpec;
  /** When true, either side may initiate (the far side isn't only a responder). */
  bidirectional: boolean;
  created_at: string;
  updated_at: string;
}
