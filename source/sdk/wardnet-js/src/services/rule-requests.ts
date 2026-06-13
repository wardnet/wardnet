import type { WardnetClient } from "../client.js";
import type {
  CreateRuleRequestRequest,
  DeviceRuleRequest,
  RuleRequestStatus,
} from "../types/api.js";

/**
 * Device rule requests — the "ask the admin" inbox.
 *
 * Device-scoped methods (`*My*`) resolve the caller by source IP and need no
 * auth. Admin methods require an admin session / API key.
 */
export class RuleRequestService {
  constructor(private readonly client: WardnetClient) {}

  /** Submit a request to block or allow a domain (device, by IP). */
  async createMine(body: CreateRuleRequestRequest): Promise<DeviceRuleRequest> {
    return this.client.request<DeviceRuleRequest>("/devices/me/rule-requests", {
      method: "POST",
      body: JSON.stringify(body),
    });
  }

  /** List the calling device's own rule requests (device, by IP). */
  async listMine(): Promise<DeviceRuleRequest[]> {
    return this.client.request<DeviceRuleRequest[]>("/devices/me/rule-requests");
  }

  /** Admin: list all rule requests, optionally filtered by status. */
  async list(status?: RuleRequestStatus): Promise<DeviceRuleRequest[]> {
    const qs = status ? `?status=${encodeURIComponent(status)}` : "";
    return this.client.request<DeviceRuleRequest[]>(`/rule-requests${qs}`);
  }

  /** Admin: approve or reject a rule request. */
  async decide(id: string, status: RuleRequestStatus): Promise<DeviceRuleRequest> {
    return this.client.request<DeviceRuleRequest>(`/rule-requests/${id}`, {
      method: "PATCH",
      body: JSON.stringify({ status }),
    });
  }
}
