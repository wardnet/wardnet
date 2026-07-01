import type { WardnetClient } from "../client.js";
import type {
  CreateNetworkZoneRequest,
  CreateNetworkZoneResponse,
  DeleteNetworkZoneResponse,
  DeviceDetailResponse,
  GetNetworkZoneResponse,
  ListNetworkZonesResponse,
  UpdateNetworkZoneRequest,
  UpdateNetworkZoneResponse,
} from "../types/api.js";

/**
 * Network Zones management — device policy buckets (epic #244, issue #735).
 * All operations are admin-only.
 */
export class NetworkZonesService {
  constructor(private readonly client: WardnetClient) {}

  /** List all zones with their member counts. */
  async list(): Promise<ListNetworkZonesResponse> {
    return this.client.request<ListNetworkZonesResponse>("/network/zones");
  }

  /** Fetch a single zone with its member count. */
  async getById(id: string): Promise<GetNetworkZoneResponse> {
    return this.client.request<GetNetworkZoneResponse>(`/network/zones/${id}`);
  }

  /** Create a new (manual) zone. */
  async create(body: CreateNetworkZoneRequest): Promise<CreateNetworkZoneResponse> {
    return this.client.request<CreateNetworkZoneResponse>("/network/zones", {
      method: "POST",
      body: JSON.stringify(body),
    });
  }

  /**
   * Partially update a zone. Set `is_default`/`is_default_for_new` to `true`
   * to promote this zone; `false` is rejected.
   */
  async update(id: string, body: UpdateNetworkZoneRequest): Promise<UpdateNetworkZoneResponse> {
    return this.client.request<UpdateNetworkZoneResponse>(`/network/zones/${id}`, {
      method: "PUT",
      body: JSON.stringify(body),
    });
  }

  /** Delete a zone (rejected for system / default / non-empty zones). */
  async delete(id: string): Promise<DeleteNetworkZoneResponse> {
    return this.client.request<DeleteNetworkZoneResponse>(`/network/zones/${id}`, {
      method: "DELETE",
    });
  }

  /** Reassign a device to a different zone. Returns the updated device detail. */
  async assignDevice(deviceId: string, zoneId: string): Promise<DeviceDetailResponse> {
    return this.client.request<DeviceDetailResponse>(`/devices/${deviceId}/zone`, {
      method: "PUT",
      body: JSON.stringify({ zone_id: zoneId }),
    });
  }
}
