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
}
