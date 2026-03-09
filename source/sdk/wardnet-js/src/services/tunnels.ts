import type { WardnetClient } from "../client.js";
import type {
  CreateTunnelRequest,
  CreateTunnelResponse,
  DeleteTunnelResponse,
  ListTunnelsResponse,
} from "../types/api.js";

/** Tunnel management service for the Wardnet daemon. */
export class TunnelService {
  constructor(private readonly client: WardnetClient) {}

  /** List all configured tunnels (admin only). */
  async list(): Promise<ListTunnelsResponse> {
    return this.client.request<ListTunnelsResponse>("/tunnels");
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
