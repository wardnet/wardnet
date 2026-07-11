/**
 * Inbound (multi-peer) WireGuard remote-access grants (issues #809-#811).
 *
 * A grant lets an already-managed device connect back in from off the LAN.
 * Distinct from `RemoteAccessService` (types/remote-access.ts), which covers
 * wardnet-cloud DDNS/TLS enrollment for reaching the box itself, not
 * per-device inbound tunnels.
 */

/** Request body for `PUT /api/inbound-wg/config`. */
export interface InboundWgConfigRequest {
  /** Whether the inbound WireGuard server should be running. */
  enabled: boolean;
  /** UDP port the server listens on for inbound peer handshakes. */
  listen_port: number;
}

/** Response for `PUT /api/inbound-wg/config`. */
export interface InboundWgConfigResponse {
  /** The applied enabled state. */
  enabled: boolean;
  /** The applied listen port. */
  listen_port: number;
  /**
   * The server's public key once a keypair exists (generated on first
   * enable). `null` until the server has been enabled at least once.
   */
  server_public_key: string | null;
}

/**
 * Request body for `POST /api/inbound-wg/peers`.
 *
 * A remote-access grant targets an already-managed device (issue #810).
 */
export interface AddInboundWgPeerRequest {
  /**
   * The device to grant remote access to. Must already exist (discovered on
   * the LAN at least once) and must not already have a credential.
   */
  device_id: string;
}

/**
 * Response for `POST /api/inbound-wg/peers`.
 *
 * Carries the full WireGuard client config (`.conf`) exactly once, assembled
 * daemon-side. It embeds the freshly generated private key, which is never
 * persisted server-side, so it must be scanned/saved now to configure the
 * peer. This is the only endpoint that ever exposes private key material, and
 * it is admin-gated.
 */
export interface AddInboundWgPeerResponse {
  id: string;
  name: string;
  /** Base64 WireGuard public key (stored server-side). */
  public_key: string;
  /** The peer's allocated /32 inside the inbound tunnel subnet. */
  allowed_ip: string;
  /**
   * The complete WireGuard client config (`[Interface]` + `[Peer]`), ready to
   * render as a QR code or download as a `.conf`. Embeds the private key and
   * is returned exactly once. `null` when no reachable endpoint is known yet
   * (no DDNS hostname or public IP), in which case a usable config cannot be
   * assembled.
   */
  client_config: string | null;
}

/** A single inbound-WireGuard peer, without any private key material. */
export interface InboundWgPeerSummary {
  id: string;
  name: string;
  public_key: string;
  allowed_ip: string;
  enabled: boolean;
  created_at: string;
  /** The device this credential grants remote access to. `null` only for
   *  pre-#810 rows written before the device link existed. */
  device_id: string | null;
}

/** Response for `GET /api/inbound-wg/peers`. */
export interface ListInboundWgPeersResponse {
  peers: InboundWgPeerSummary[];
}

/**
 * Request body for `PATCH /api/inbound-wg/peers/{id}`.
 *
 * Pauses or resumes a peer without deleting its credential — distinct from
 * `removePeer`, which revokes it permanently and requires a fresh keypair
 * (and QR scan) to re-grant.
 */
export interface SetInboundWgPeerEnabledRequest {
  enabled: boolean;
}
