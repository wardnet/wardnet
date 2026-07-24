import type { WardnetClient } from "../client.js";
import { apiClient, type ApiClient } from "../internal/client.js";
import type { RoutingTarget } from "../types/device.js";
import type {
  DeviceDetailResponse,
  DeviceMeResponse,
  DnsCaptureSettingsRequest,
  DnsCaptureSettingsResponse,
  ListDevicesResponse,
  SetMyRuleResponse,
  UpdateDeviceRequest,
} from "../types/api.js";

/** Device management service for the Wardnet daemon. */
export class DeviceService {
  private readonly api: ApiClient;

  constructor(client: WardnetClient) {
    this.api = apiClient(client);
  }

  /** List all devices (admin only). */
  async list(): Promise<ListDevicesResponse> {
    return this.api.get("/devices");
  }

  /** Get a device by ID with its current routing rule (admin only). */
  async getById(id: string): Promise<DeviceDetailResponse> {
    return this.api.get("/devices/{id}", { path: { id } });
  }

  /** Get the calling device's info based on source IP (no auth required). */
  async getMe(): Promise<DeviceMeResponse> {
    const res = await this.api.get("/devices/me");
    // The daemon projects `device` as a bare Device here — without the
    // `dhcp_status` / `current_rule` fields the admin-facing endpoints carry.
    // The public `DeviceMeResponse` has always reused the fuller `Device`
    // type, so narrow the cast to just this field and keep the rest checked.
    return { ...res, device: res.device as DeviceMeResponse["device"] };
  }

  /** Set the calling device's routing rule (no auth required, blocked if admin-locked). */
  async setMyRule(target: RoutingTarget): Promise<SetMyRuleResponse> {
    return this.api.put("/devices/me/rule", { body: { target } });
  }

  /** Update a device's name and/or type (admin only). */
  async update(id: string, body: UpdateDeviceRequest): Promise<DeviceDetailResponse> {
    return this.api.put("/devices/{id}", { path: { id }, body });
  }

  /** Get DNS capture settings and storage stats for a device (admin only). */
  async getDnsCaptureSettings(id: string): Promise<DnsCaptureSettingsResponse> {
    return this.api.get("/devices/{id}/dns-capture", { path: { id } });
  }

  /** Update DNS capture settings for a device (admin only). */
  async updateDnsCaptureSettings(
    id: string,
    body: DnsCaptureSettingsRequest,
  ): Promise<DnsCaptureSettingsResponse> {
    return this.api.patch("/devices/{id}/dns-capture", { path: { id }, body });
  }

  /**
   * Enable or disable DNS capture for the calling device (no auth required,
   * resolved by source IP). Only the `enabled` flag changes — retention caps
   * are admin-only. Returns the device's current capture settings and stats.
   */
  async setMyCaptureEnabled(enabled: boolean): Promise<DnsCaptureSettingsResponse> {
    return this.api.patch("/devices/me/dns-capture", { body: { enabled } });
  }

  /** Acknowledge receipt of DNS events up to and including `upToId`. */
  async ackDnsEvents(upToId: number): Promise<void> {
    await this.api.post("/devices/me/dns-events/ack", { body: { up_to_id: upToId } });
  }
}
