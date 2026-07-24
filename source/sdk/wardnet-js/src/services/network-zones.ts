import type { WardnetClient } from "../client.js";
import { apiClient, type ApiClient } from "../internal/client.js";
import type {
  CreateNetworkZoneRequest,
  CreateNetworkZoneResponse,
  DeleteNetworkZoneResponse,
  DeviceDetailResponse,
  GetNetworkZoneResponse,
  ListNetworkZonesResponse,
  QuarantineNewDevicesResponse,
  SetQuarantineNewDevicesRequest,
  UpdateNetworkZoneRequest,
  UpdateNetworkZoneResponse,
} from "../types/api.js";

/**
 * Network Zones management — device policy buckets (epic #244, issue #735).
 * All operations are admin-only.
 */
export class NetworkZonesService {
  private readonly api: ApiClient;

  constructor(client: WardnetClient) {
    this.api = apiClient(client);
  }

  /** List all zones with their member counts. */
  async list(): Promise<ListNetworkZonesResponse> {
    return this.api.get("/network/zones");
  }

  /** Fetch a single zone with its member count. */
  async getById(id: string): Promise<GetNetworkZoneResponse> {
    return this.api.get("/network/zones/{id}", { path: { id } });
  }

  /** Create a new (manual) zone. */
  async create(body: CreateNetworkZoneRequest): Promise<CreateNetworkZoneResponse> {
    return this.api.post("/network/zones", { body });
  }

  /**
   * Partially update a zone. Set `is_default`/`is_default_for_new` to `true`
   * to promote this zone; `false` is rejected.
   */
  async update(id: string, body: UpdateNetworkZoneRequest): Promise<UpdateNetworkZoneResponse> {
    return this.api.put("/network/zones/{id}", { path: { id }, body });
  }

  /** Delete a zone (rejected for system / default / non-empty zones). */
  async delete(id: string): Promise<DeleteNetworkZoneResponse> {
    return this.api.del("/network/zones/{id}", { path: { id } });
  }

  /** Reassign a device to a different zone. Returns the updated device detail. */
  async assignDevice(deviceId: string, zoneId: string): Promise<DeviceDetailResponse> {
    return this.api.put("/devices/{id}/zone", {
      path: { id: deviceId },
      body: { zone_id: zoneId },
    });
  }

  /** Read the new-device quarantine toggle (issue #738). */
  async getQuarantineNewDevices(): Promise<QuarantineNewDevicesResponse> {
    return this.api.get("/network/quarantine-new-devices");
  }

  /** Enable or disable new-device quarantine (issue #738). */
  async setQuarantineNewDevices(
    body: SetQuarantineNewDevicesRequest,
  ): Promise<QuarantineNewDevicesResponse> {
    return this.api.put("/network/quarantine-new-devices", { body });
  }
}
