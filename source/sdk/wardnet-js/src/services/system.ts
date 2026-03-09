import type { WardnetClient } from "../client.js";
import type { SystemStatusResponse } from "../types/system.js";

/** System information service for the Wardnet daemon. */
export class SystemService {
  constructor(private readonly client: WardnetClient) {}

  /** Get system status including version, uptime, and counts (admin only). */
  async getStatus(): Promise<SystemStatusResponse> {
    return this.client.request<SystemStatusResponse>("/system/status");
  }
}
