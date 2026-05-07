import { type WardnetClient } from "../client.js";
import type {
  SetDefaultPolicyRequest,
  SetDefaultPolicyResponse,
  SystemStatusResponse,
} from "../types/system.js";

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
    await this.postNoContent("/system/restart");
  }

  /**
   * Ask the host (Pi) to reboot.
   *
   * Resolves once the server has accepted the request (HTTP 204);
   * the actual reboot fires a few hundred milliseconds later via
   * `systemctl reboot --no-block`. Callers should treat the daemon
   * as unreachable for the next 30–60 seconds and surface an
   * appropriate progress UI to the user.
   *
   * Throws [`WardnetApiError`] on a non-2xx response — for example
   * if the request was not authenticated (401), the user is not an
   * admin (403), or the polkit migration has not been applied so
   * logind refused the action (500).
   */
  async reboot(): Promise<void> {
    await this.postNoContent("/system/reboot");
  }

  /**
   * Ask the host (Pi) to power off.
   *
   * Resolves once the server has accepted the request (HTTP 204);
   * the actual poweroff fires a few hundred milliseconds later via
   * `systemctl poweroff --no-block`. Internet for managed devices
   * stays unavailable until the operator powers the Pi back on
   * manually — there is no automatic recovery.
   *
   * Throws [`WardnetApiError`] on a non-2xx response.
   */
  async shutdown(): Promise<void> {
    await this.postNoContent("/system/shutdown");
  }

  /** Read the global default routing policy (`"direct"` or a tunnel UUID). */
  async getDefaultPolicy(): Promise<SetDefaultPolicyResponse> {
    return this.client.request<SetDefaultPolicyResponse>("/system/default-policy");
  }

  /**
   * Set the global default routing policy.
   *
   * `policy` is either the literal string `"direct"` or a tunnel UUID.
   * The server rejects anything else with a 400 response.
   */
  async setDefaultPolicy(body: SetDefaultPolicyRequest): Promise<SetDefaultPolicyResponse> {
    return this.client.request<SetDefaultPolicyResponse>("/system/default-policy", {
      method: "PUT",
      body: JSON.stringify(body),
    });
  }

  /**
   * Shared POST helper for endpoints that return `204 No Content`.
   *
   * Routes through `client.request<void>` so subclassed clients
   * (e.g. `AuthedClient` in the e2e harness, which overrides
   * `request` to attach a `Bearer` token) get a chance to inject
   * their headers. The client handles the empty body for 204.
   */
  private async postNoContent(path: string): Promise<void> {
    await this.client.request<void>(path, { method: "POST" });
  }
}
