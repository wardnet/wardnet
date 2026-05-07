import type { WardnetClient } from "../client.js";
import type {
  DhcpSelfProbeResponse,
  DiscoverGatewayMacRequest,
  DiscoverGatewayMacResponse,
  NetworkStatusResponse,
} from "../types/network.js";

/** LAN interface inspection + active probes — admin-only. */
export class NetworkService {
  constructor(private readonly client: WardnetClient) {}

  /**
   * Read the LAN interface's current address + default gateway and
   * classify whether the IP came from DHCP or a Wardnet-managed
   * static config.
   */
  async getStatus(): Promise<NetworkStatusResponse> {
    return this.client.request<NetworkStatusResponse>("/network/status");
  }

  /**
   * Discover (or accept) the upstream router MAC.
   *
   * With an empty body the daemon ARP-probes the LAN gateway and
   * returns the responder's MAC. Supply `body.mac` to skip the
   * probe and persist an operator-typed value (validated). Supply
   * `body.target_ip` to override the probe target.
   */
  async discoverGatewayMac(
    body: DiscoverGatewayMacRequest = {},
  ): Promise<DiscoverGatewayMacResponse> {
    return this.client.request<DiscoverGatewayMacResponse>("/network/discover-gateway-mac", {
      method: "POST",
      body: JSON.stringify(body),
    });
  }

  /**
   * Send a DHCPDISCOVER from a synthetic MAC and report which DHCP
   * servers responded.
   *
   * Used by the wizard's primary-mode step 3 to verify Wardnet now
   * owns the LAN's DHCP after the operator disabled it on the
   * upstream router.
   */
  async dhcpSelfProbe(): Promise<DhcpSelfProbeResponse> {
    return this.client.request<DhcpSelfProbeResponse>("/network/dhcp-self-probe", {
      method: "POST",
    });
  }
}
