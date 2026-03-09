import type { WardnetClient } from "../client.js";
import type {
  SetupStatusResponse,
  SetupRequest,
  SetupResponse,
} from "../types/setup.js";

/** Setup wizard service for initial admin account creation. */
export class SetupService {
  constructor(private readonly client: WardnetClient) {}

  /** Check whether initial setup has been completed. */
  async getStatus(): Promise<SetupStatusResponse> {
    return this.client.request<SetupStatusResponse>("/setup/status");
  }

  /** Create the first admin account (only works if setup not yet completed). */
  async setup(body: SetupRequest): Promise<SetupResponse> {
    return this.client.request<SetupResponse>("/setup", {
      method: "POST",
      body: JSON.stringify(body),
    });
  }
}
