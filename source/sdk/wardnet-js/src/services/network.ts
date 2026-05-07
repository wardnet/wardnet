import type { WardnetClient } from "../client.js";
import type { NetworkStatusResponse } from "../types/network.js";

/** LAN interface inspection — read-only state for the setup wizard + Settings. */
export class NetworkService {
  constructor(private readonly client: WardnetClient) {}

  /**
   * Read the LAN interface's current address + default gateway and
   * classify whether the IP came from DHCP or a Wardnet-managed
   * static config. Admin-only.
   */
  async getStatus(): Promise<NetworkStatusResponse> {
    return this.client.request<NetworkStatusResponse>("/network/status");
  }
}
