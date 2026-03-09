/** The current status of a WireGuard tunnel. */
export type TunnelStatus = "up" | "down" | "connecting";

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
