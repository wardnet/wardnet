import type { WardnetClient } from "../client.js";
import type { InfoResponse } from "../types/info.js";

/** Public info service — no authentication required. */
export class InfoService {
  constructor(private readonly client: WardnetClient) {}

  /** Get basic server info (version + uptime). */
  async getInfo(): Promise<InfoResponse> {
    return this.client.request<InfoResponse>("/info");
  }
}
