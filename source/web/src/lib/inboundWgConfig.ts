/**
 * Builds a standard WireGuard client `.conf` for an inbound-WireGuard peer
 * (issues #809-#813). The daemon returns a peer's private key exactly once,
 * at grant time — this is the only place that plaintext ever exists outside
 * the daemon, so the config is generated client-side and never round-trips
 * back to the server.
 */

export interface InboundWgClientConfigInput {
  /** Base64 WireGuard private key, from `addPeer`'s one-time response. */
  privateKey: string;
  /** The peer's allocated `/32` inside the inbound tunnel subnet. */
  allowedIp: string;
  /** Base64 server public key, from the server config. */
  serverPublicKey: string;
  /**
   * `host:port` the client dials. **Placeholder today** — see the
   * relay-endpoint gap tracked for a follow-up issue: this is the daemon's
   * own DDNS hostname (or WAN IP) plus its local listen port, not the real
   * cloud relay address a remote peer must actually reach. Callers should
   * surface this to the admin as unverified/placeholder, not as a working
   * value.
   */
  endpoint: string;
}

/** Standard `[Interface]`/`[Peer]` WireGuard client config text. */
export function buildInboundWgClientConfig({
  privateKey,
  allowedIp,
  serverPublicKey,
  endpoint,
}: InboundWgClientConfigInput): string {
  const address = allowedIp.includes("/") ? allowedIp : `${allowedIp}/32`;
  return [
    "[Interface]",
    `PrivateKey = ${privateKey}`,
    `Address = ${address}`,
    "DNS = 10.100.64.1",
    "",
    "[Peer]",
    `PublicKey = ${serverPublicKey}`,
    `Endpoint = ${endpoint}`,
    "AllowedIPs = 0.0.0.0/0, ::/0",
    "PersistentKeepalive = 25",
    "",
  ].join("\n");
}

/** Best-effort `host:port` for the placeholder Endpoint (see the caveat on
 *  {@link InboundWgClientConfigInput.endpoint}). Prefers the DDNS hostname,
 *  falling back to the last-known public IP. `null` when neither is set. */
export function placeholderEndpoint(
  listenPort: number,
  fqdn: string | null,
  lastPublicIp: string | null,
): string | null {
  const host = fqdn || lastPublicIp;
  return host ? `${host}:${listenPort}` : null;
}
