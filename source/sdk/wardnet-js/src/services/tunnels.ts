import type { WardnetClient } from "../client.js";
import type {
  CreateTunnelRequest,
  CreateTunnelResponse,
  DeleteTunnelResponse,
  ListTunnelsResponse,
  RebuildTunnelResponse,
  TunnelDetailResponse,
  TunnelDevicesResponse,
  TunnelTestResponse,
  UpdateTunnelDnsOverrideRequest,
  UpdateTunnelDnsOverrideResponse,
} from "../types/api.js";

/** Tunnel management service for the Wardnet daemon. */
export class TunnelService {
  constructor(private readonly client: WardnetClient) {}

  /** List all configured tunnels (admin only). */
  async list(): Promise<ListTunnelsResponse> {
    return this.client.request<ListTunnelsResponse>("/tunnels");
  }

  /** Get one tunnel by ID (admin only). */
  async getById(id: string): Promise<TunnelDetailResponse> {
    return this.client.request<TunnelDetailResponse>(`/tunnels/${id}`);
  }

  /** List the devices currently routed through a tunnel (admin only). */
  async listDevices(id: string): Promise<TunnelDevicesResponse> {
    return this.client.request<TunnelDevicesResponse>(`/tunnels/${id}/devices`);
  }

  /** Import a tunnel from a WireGuard .conf file (admin only). */
  async create(body: CreateTunnelRequest): Promise<CreateTunnelResponse> {
    return this.client.request<CreateTunnelResponse>("/tunnels", {
      method: "POST",
      body: JSON.stringify(body),
    });
  }

  /** Delete a tunnel and its configuration (admin only). */
  async delete(id: string): Promise<DeleteTunnelResponse> {
    return this.client.request<DeleteTunnelResponse>(`/tunnels/${id}`, {
      method: "DELETE",
    });
  }

  /**
   * Probe the tunnel for its exit IP, country, and latency (admin only).
   * The daemon brings the tunnel up if needed, runs a single HTTP probe
   * through the tunnel, then restores the prior up/down state.
   */
  async test(id: string): Promise<TunnelTestResponse> {
    return this.client.request<TunnelTestResponse>(`/tunnels/${id}/test`, {
      method: "POST",
    });
  }

  /**
   * Toggle the per-tunnel `override_default_dns` flag. When `true`, devices
   * routed through this tunnel resolve DNS via wardnet (so the ad-blocking
   * filter still runs) and wardnet forwards those queries to the tunnel's
   * DNS server bound to the tunnel interface. When `false`, the system-wide
   * upstream pool is used. See issue #342.
   */
  async setDnsOverride(
    id: string,
    body: UpdateTunnelDnsOverrideRequest,
  ): Promise<UpdateTunnelDnsOverrideResponse> {
    return this.client.request<UpdateTunnelDnsOverrideResponse>(`/tunnels/${id}/dns-override`, {
      method: "PUT",
      body: JSON.stringify(body),
    });
  }

  /**
   * Tear down and re-apply the WireGuard interface from persisted config
   * (admin only). Use when a tunnel is stale or devices cannot route through
   * it.
   */
  async rebuild(id: string): Promise<RebuildTunnelResponse> {
    return this.client.request<RebuildTunnelResponse>(`/tunnels/${id}/rebuild`, {
      method: "POST",
    });
  }
}
