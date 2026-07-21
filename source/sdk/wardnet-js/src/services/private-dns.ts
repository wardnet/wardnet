import type { WardnetClient } from "../client.js";
import type {
  CreatePrivateDnsGrantRequest,
  PrivateDnsGrantSummary,
  PrivateDnsMeResponse,
  PrivateDnsStatusResponse,
  SetPrivateDnsEnabledRequest,
} from "../types/private-dns.js";

/**
 * Private DNS — encrypted DNS at home and while roaming (issues #910/#914).
 *
 * The admin methods manage the feature and per-device grants. The `me` methods
 * are device-keyed (identified by the caller's source IP, like `devices/me`)
 * and back the user PWA.
 */
export class PrivateDnsService {
  constructor(private readonly client: WardnetClient) {}

  /** Read enabled state, enable prerequisites, and every grant. Admin only. */
  async getStatus(): Promise<PrivateDnsStatusResponse> {
    return this.client.request<PrivateDnsStatusResponse>("/private-dns/status");
  }

  /**
   * Enable or disable Private DNS. Enabling is Premium-gated and rejects with
   * 409 when a hard prerequisite is missing. Returns the full status.
   */
  async setEnabled(enabled: boolean): Promise<PrivateDnsStatusResponse> {
    const body: SetPrivateDnsEnabledRequest = { enabled };
    return this.client.request<PrivateDnsStatusResponse>("/private-dns", {
      method: "PUT",
      body: JSON.stringify(body),
    });
  }

  /**
   * Grant a managed device encrypted-DNS access. The response carries the full
   * minted hostname. Admin only.
   */
  async grantDevice(deviceId: string): Promise<PrivateDnsGrantSummary> {
    const body: CreatePrivateDnsGrantRequest = { device_id: deviceId };
    return this.client.request<PrivateDnsGrantSummary>("/private-dns/grants", {
      method: "POST",
      body: JSON.stringify(body),
    });
  }

  /** Revoke a device's grant by device id. Admin only. */
  async revokeDevice(deviceId: string): Promise<void> {
    await this.client.request<void>(`/private-dns/grants/${deviceId}`, {
      method: "DELETE",
    });
  }

  /** This device's own Private DNS state, identified by source IP. */
  async me(): Promise<PrivateDnsMeResponse> {
    return this.client.request<PrivateDnsMeResponse>("/private-dns/me");
  }

  /**
   * The URL of this device's signed iOS profile (`.mobileconfig`). Returns a
   * plain URL rather than fetching: Safari's install flow triggers on a real
   * navigation, so this is meant to be used as a link `href` or QR target.
   */
  profileUrl(): string {
    return `${this.client.baseUrl}/private-dns/me/profile`;
  }
}
