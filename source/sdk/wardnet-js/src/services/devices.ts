import type { WardnetClient } from "../client.js";
import { apiClient, type ApiClient } from "../internal/client.js";
import type { RoutingTarget } from "../types/device.js";
import type {
  DeviceDetailResponse,
  DeviceMeResponse,
  DeviceProbeResponse,
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

  /**
   * Probe a device on the vendor catalog's TCP ports and record whichever
   * answered as identification signals, naming the device if one resolves to a
   * vendor (admin only, issue #1116).
   *
   * This is the only identification signal that sends unsolicited traffic to a
   * device, so per ADR 0025 §5 it must stay bound to an explicit admin action —
   * never a background sweep over `list()`.
   *
   * Rejects with a 409 when the device is not currently on the network: its
   * last known address may since have been handed to someone else, and the
   * resulting vendor would be recorded against the wrong device permanently.
   */
  async identify(id: string): Promise<DeviceProbeResponse> {
    return this.api.post("/devices/{id}/identify", { path: { id } });
  }

  /**
   * Stop managing a device — revert every admin-set configuration to default
   * and return it to unmanaged (admin only, issue #1181).
   *
   * **Destructive.** This revokes the device's Private-DNS grant and its
   * Remote peer credential, disconnecting it. It also clears the device's
   * name, admin lock, DNS capture, routing rule and profiles, DNS-filter
   * settings, DHCP reservation, and zone exceptions, and returns it to the
   * default-for-new zone.
   *
   * Once unmanaged the device becomes subject to device retention and is
   * deleted after 30 days away. Idempotent: a retry after a partial failure
   * completes, and a failure part-way leaves the device still managed rather
   * than half-released.
   */
  async release(id: string): Promise<DeviceDetailResponse> {
    return this.api.post("/devices/{id}/release", { path: { id } });
  }

  /**
   * Assign or clear the household user a device belongs to (admin only,
   * ADR-0031 §4). Pass `null` to clear.
   *
   * **Attribution, never authentication.** The owner's role has no effect on
   * what the device may do — a device caller resolves to the `Device`
   * principal whoever owns it, including an admin. Device identity is derived
   * from the source IP, so treating ownership as a credential would collapse
   * admin access to IP spoofing.
   */
  async setOwner(id: string, ownerUserId: string | null): Promise<DeviceDetailResponse> {
    return this.api.put("/devices/{id}/owner", {
      path: { id },
      body: { owner_user_id: ownerUserId },
    });
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
