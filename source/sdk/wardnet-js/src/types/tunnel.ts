/**
 * The current status of a WireGuard tunnel.
 *
 * - `down` — kernel interface not configured (initial, after tear-down or delete).
 * - `connecting` — kernel interface configured; no handshake observed yet
 *   (initial bring-up).
 * - `up` — recent (≤ 3 min) handshake observed.
 * - `reconnecting` — was `up`, last handshake stale (> 3 min) or absent;
 *   the iface is still configured and WireGuard keepalive is retrying.
 */
export type TunnelStatus = "up" | "down" | "connecting" | "reconnecting";

/** A WireGuard tunnel configuration and its live state. */
export interface Tunnel {
  id: string;
  label: string;
  country_code: string;
  provider: string | null;
  interface_name: string;
  endpoint: string;
  status: TunnelStatus;
  last_handshake: string | null;
  bytes_tx: number;
  bytes_rx: number;
  created_at: string;
  /**
   * When true, devices routed through this tunnel resolve DNS via wardnet's
   * DNS server (so the ad-blocking filter still runs) and wardnet forwards
   * those queries to the tunnel's DNS server with `SO_BINDTODEVICE`. When
   * false, the per-tunnel DNS server (if any) is ignored and the
   * system-wide upstream pool is used. See issue #342.
   */
  override_default_dns: boolean;
}
