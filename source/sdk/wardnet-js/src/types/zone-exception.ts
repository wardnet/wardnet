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
 * - `casting` = mDNS + AirPlay + Chromecast + DLNA.
 * - `mirroring` = screen-mirroring (AirPlay / Miracast) ports.
 */
export type ServiceSet = "casting" | "mirroring";

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
