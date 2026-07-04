/**
 * How the LAN interface acquired its current IP address.
 *
 * Returned by `GET /api/network/status`. `static` means
 * `install.sh --static-ip` wrote `/etc/dhcpcd.conf.d/wardnet.conf`;
 * `dhcp` means the interface is still using whatever the upstream
 * router handed out; `unknown` means the inspector couldn't classify
 * it (treated like `dhcp` for remediation, but called out separately).
 */
export type DhcpSource = "static" | "dhcp" | "unknown";

/** Response for GET /api/network/status. */
export interface NetworkStatusResponse {
  interface: string;
  ip: string;
  /** May be null when the inspector can't read a default route. */
  gateway: string | null;
  dhcp_source: DhcpSource;
  /**
   * Upstream router MAC persisted by the wizard's discover step, if
   * any — lets the review step display it without re-probing.
   */
  router_mac: string | null;
}

/** How the upstream router MAC was obtained. */
export type RouterMacSource = "arp" | "manual";

/**
 * Request body for POST /api/network/discover-gateway-mac.
 *
 * All fields optional. With `mac` set, the daemon skips the ARP probe
 * and just persists the operator-supplied value. With `target_ip`
 * set, the daemon probes that IP instead of the gateway from
 * `GET /api/network/status`. Empty body → probe the gateway.
 */
export interface DiscoverGatewayMacRequest {
  mac?: string | null;
  target_ip?: string | null;
}

/** Response for POST /api/network/discover-gateway-mac. */
export interface DiscoverGatewayMacResponse {
  mac: string;
  source: RouterMacSource;
}

/**
 * Response for POST /api/network/dhcp-self-probe.
 *
 * `wardnet_responded === true && foreign_responded === false` means
 * the LAN is correctly owned by Wardnet's DHCP server.
 * `foreign_responded === true` means another DHCP server is still
 * answering — the wizard re-shows the disable-DHCP guide and points
 * at `foreign_server_ip`.
 */
export interface DhcpSelfProbeResponse {
  wardnet_responded: boolean;
  foreign_responded: boolean;
  foreign_server_ip: string | null;
}
