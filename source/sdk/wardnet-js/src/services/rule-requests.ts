import type { WardnetClient } from "../client.js";
import { apiClient, type ApiClient } from "../internal/client.js";
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
  private readonly api: ApiClient;

  constructor(client: WardnetClient) {
    this.api = apiClient(client);
  }

  /** Submit a request to block or allow a domain (device, by IP). */
  async createMine(body: CreateRuleRequestRequest): Promise<DeviceRuleRequest> {
    return this.api.post("/devices/me/rule-requests", { body });
  }

  /** List the calling device's own rule requests (device, by IP). */
  async listMine(): Promise<DeviceRuleRequest[]> {
    return this.api.get("/devices/me/rule-requests");
  }

  /** Admin: list all rule requests, optionally filtered by status. */
  async list(status?: RuleRequestStatus): Promise<DeviceRuleRequest[]> {
    return this.api.get("/rule-requests", { query: { status } });
  }

  /** Admin: approve or reject a rule request. */
  async decide(id: string, status: RuleRequestStatus): Promise<DeviceRuleRequest> {
    return this.api.patch("/rule-requests/{id}", { path: { id }, body: { status } });
  }
}
