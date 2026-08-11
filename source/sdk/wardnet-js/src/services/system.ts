import { type WardnetClient } from "../client.js";
import { apiClient, type ApiClient } from "../internal/client.js";
import type {
  SetDefaultPolicyRequest,
  SetDefaultPolicyResponse,
  SystemStatusResponse,
} from "../types/system.js";

/** System information service for the Wardnet daemon. */
export class SystemService {
  private readonly api: ApiClient;

  constructor(client: WardnetClient) {
    this.api = apiClient(client);
  }

  /** Get system status including version, uptime, and counts (admin only). */
  async getStatus(): Promise<SystemStatusResponse> {
    return this.api.get("/system/status");
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
    await this.api.post("/system/restart");
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
    await this.api.post("/system/reboot");
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
    await this.api.post("/system/shutdown");
  }

  /** Read the global default routing policy (`"direct"` or a tunnel UUID). */
  async getDefaultPolicy(): Promise<SetDefaultPolicyResponse> {
    return this.api.get("/system/default-policy");
  }

  /**
   * Set the global default routing policy.
   *
   * `policy` is either the literal string `"direct"` or a tunnel UUID.
   * The server rejects anything else with a 400 response.
   */
  async setDefaultPolicy(body: SetDefaultPolicyRequest): Promise<SetDefaultPolicyResponse> {
    return this.api.put("/system/default-policy", { body });
  }

  /**
   * Acknowledge the most recent unclean-shutdown event so the
   * dashboard banner is dismissed.
   *
   * Idempotent — repeated calls just refresh the timestamp. The
   * banner reappears automatically on the next unclean shutdown
   * because the new event timestamp is newer than the stored
   * acknowledgement.
   *
   * Throws [`WardnetApiError`] on a non-2xx response.
   */
  async acknowledgeShutdown(): Promise<void> {
    await this.api.post("/system/shutdown/acknowledge");
  }
}
