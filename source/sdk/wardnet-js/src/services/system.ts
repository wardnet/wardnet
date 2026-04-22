import { WardnetApiError, type WardnetClient } from "../client.js";
import type { ApiError } from "../types/api.js";
import type { SystemStatusResponse } from "../types/system.js";

/** System information service for the Wardnet daemon. */
export class SystemService {
  constructor(private readonly client: WardnetClient) {}

  /** Get system status including version, uptime, and counts (admin only). */
  async getStatus(): Promise<SystemStatusResponse> {
    return this.client.request<SystemStatusResponse>("/system/status");
  }

  /**
   * Ask the daemon to exit so the supervisor restarts it.
   *
   * Resolves once the server has scheduled the restart (HTTP 204);
   * the daemon then exits a few hundred milliseconds later. Callers
   * should expect the next request to fail for several seconds while
   * the process comes back up.
   *
   * On a Pi install systemd (`Restart=always` on `wardnetd.service`)
   * brings the daemon back. On the dev mock the operator re-runs
   * `make run-dev`.
   */
  async restart(): Promise<void> {
    const res = await fetch(`${this.client.baseUrl}/system/restart`, {
      method: "POST",
      credentials: "include",
    });
    if (!res.ok) {
      const requestId = res.headers.get("X-Request-Id") ?? undefined;
      const body = (await res.json().catch(() => ({ error: res.statusText }))) as ApiError;
      throw new WardnetApiError(res.status, res.statusText, body, requestId);
    }
  }
}
