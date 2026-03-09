import type { WardnetClient } from "../client.js";
import type { RoutingTarget } from "../types/device.js";
import type {
  DeviceDetailResponse,
  DeviceMeResponse,
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
}
