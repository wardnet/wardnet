import type { WardnetClient } from "../client.js";
import { apiClient, type ApiClient } from "../internal/client.js";
import type { InfoResponse } from "../types/info.js";

/** Public info service — no authentication required. */
export class InfoService {
  private readonly api: ApiClient;

  constructor(client: WardnetClient) {
    this.api = apiClient(client);
  }

  /** Get basic server info (version + uptime). */
  async getInfo(): Promise<InfoResponse> {
    return this.api.get("/info");
  }
}
