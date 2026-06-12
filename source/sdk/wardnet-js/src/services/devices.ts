import type { WardnetClient } from "../client.js";
import type { RoutingTarget } from "../types/device.js";
import type {
  DeviceDetailResponse,
  DeviceMeResponse,
  DnsCaptureSettingsRequest,
  DnsCaptureSettingsResponse,
  DnsEventsAckRequest,
  ListDevicesResponse,
  SetMyRuleResponse,
  UpdateDeviceRequest,
} from "../types/api.js";

/** Device management service for the Wardnet daemon. */
export class DeviceService {
  constructor(private readonly client: WardnetClient) {}

  /** List all devices (admin only). */
  async list(): Promise<ListDevicesResponse> {
    return this.client.request<ListDevicesResponse>("/devices");
  }

  /** Get a device by ID with its current routing rule (admin only). */
  async getById(id: string): Promise<DeviceDetailResponse> {
    return this.client.request<DeviceDetailResponse>(`/devices/${id}`);
  }

  /** Get the calling device's info based on source IP (no auth required). */
  async getMe(): Promise<DeviceMeResponse> {
    return this.client.request<DeviceMeResponse>("/devices/me");
  }

  /** Set the calling device's routing rule (no auth required, blocked if admin-locked). */
  async setMyRule(target: RoutingTarget): Promise<SetMyRuleResponse> {
    return this.client.request<SetMyRuleResponse>("/devices/me/rule", {
      method: "PUT",
      body: JSON.stringify({ target }),
    });
  }

  /** Update a device's name and/or type (admin only). */
  async update(id: string, body: UpdateDeviceRequest): Promise<DeviceDetailResponse> {
    return this.client.request<DeviceDetailResponse>(`/devices/${id}`, {
      method: "PUT",
      body: JSON.stringify(body),
    });
  }

  /** Get DNS capture settings and storage stats for a device (admin only). */
  async getDnsCaptureSettings(id: string): Promise<DnsCaptureSettingsResponse> {
    return this.client.request<DnsCaptureSettingsResponse>(`/devices/${id}/dns-capture`);
  }

  /** Update DNS capture settings for a device (admin only). */
  async updateDnsCaptureSettings(
    id: string,
    body: DnsCaptureSettingsRequest,
  ): Promise<DnsCaptureSettingsResponse> {
    return this.client.request<DnsCaptureSettingsResponse>(`/devices/${id}/dns-capture`, {
      method: "PATCH",
      body: JSON.stringify(body),
    });
  }

  /** Acknowledge receipt of DNS events up to and including `upToId`. */
  async ackDnsEvents(upToId: number): Promise<void> {
    return this.client.request<void>("/devices/me/dns-events/ack", {
      method: "POST",
      body: JSON.stringify({ up_to_id: upToId } satisfies DnsEventsAckRequest),
    });
  }
}
