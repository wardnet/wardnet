import type { WardnetClient } from "../client.js";
import type {
  CreateTunnelRequest,
  CreateTunnelResponse,
  DeleteTunnelResponse,
  ListTunnelsResponse,
  TunnelDetailResponse,
  TunnelDevicesResponse,
  TunnelMetricsRange,
  TunnelMetricsResponse,
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

  /** Get throughput history for a tunnel (admin only). */
  async getMetrics(id: string, range: TunnelMetricsRange = "24h"): Promise<TunnelMetricsResponse> {
    return this.client.request<TunnelMetricsResponse>(
      `/tunnels/${id}/metrics?range=${encodeURIComponent(range)}`,
    );
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
}
