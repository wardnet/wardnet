import type { WardnetClient } from "../client.js";
import { apiClient, type ApiClient } from "../internal/client.js";
import type {
  ApprovalParams,
  CreateAccessRequestRequest,
  DeviceAccessRequest,
  AccessRequestStatus,
} from "../types/api.js";

/**
 * Device access requests — the household "ask the admin" inbox.
 *
 * Device-scoped methods (`*My*`) resolve the caller by source IP and need no
 * auth. Admin methods require an admin session / API key.
 *
 * What approval *does* depends on the request's kind: approving a
 * `private_dns` request mints the device's grant, while `allow` / `block` only
 * record the decision — the admin still applies the DNS filter rule.
 */
export class AccessRequestService {
  private readonly api: ApiClient;

  constructor(client: WardnetClient) {
    this.api = apiClient(client);
  }

  /** Ask an admin for a domain rule or Private DNS (device, by IP). */
  async createMine(body: CreateAccessRequestRequest): Promise<DeviceAccessRequest> {
    return this.api.post("/devices/me/access-requests", { body });
  }

  /** List the calling device's own access requests (device, by IP). */
  async listMine(): Promise<DeviceAccessRequest[]> {
    return this.api.get("/devices/me/access-requests");
  }

  /** Admin: list all access requests, optionally filtered by status. */
  async list(status?: AccessRequestStatus): Promise<DeviceAccessRequest[]> {
    return this.api.get("/access-requests", { query: { status } });
  }

  /** Admin: approve or reject an access request. */
  async decide(
    id: string,
    status: AccessRequestStatus,
    approval?: ApprovalParams,
  ): Promise<DeviceAccessRequest> {
    return this.api.patch("/access-requests/{id}", {
      path: { id },
      body: { status, approval },
    });
  }
}
