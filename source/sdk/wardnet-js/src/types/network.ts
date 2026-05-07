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
