import type { WardnetClient } from "../client.js";
import { apiClient, type ApiClient } from "../internal/client.js";
import type {
  InstallUpdateRequest,
  InstallUpdateResponse,
  RollbackResponse,
  UpdateCheckResponse,
  UpdateConfigRequest,
  UpdateConfigResponse,
  UpdateHistoryResponse,
  UpdateStatusResponse,
} from "../types/update.js";

/**
 * Auto-update service — status, manual check, install, rollback, config.
 *
 * All methods require admin authentication. The background runner on the
 * daemon side performs its own periodic checks; this service is the surface
 * for manual admin actions (`/update/check`, `/update/install`, etc.) and
 * for the Settings UI to read state.
 */
export class UpdateService {
  private readonly api: ApiClient;

  constructor(client: WardnetClient) {
    this.api = apiClient(client);
  }

  /** Current update subsystem snapshot (admin only). */
  async status(): Promise<UpdateStatusResponse> {
    return this.api.get("/update/status");
  }

  /** Force a manifest refresh against the active channel (admin only). */
  async check(): Promise<UpdateCheckResponse> {
    return this.api.post("/update/check");
  }

  /**
   * Start an install. If `version` is omitted, installs the latest known
   * release on the active channel. Idempotent — calling twice while an
   * install is already in flight returns the same handle.
   */
  async install(body: InstallUpdateRequest = {}): Promise<InstallUpdateResponse> {
    return this.api.post("/update/install", { body });
  }

  /** Swap back to `<live>.old` (admin only). Fails if no rollback is staged. */
  async rollback(): Promise<RollbackResponse> {
    return this.api.post("/update/rollback");
  }

  /** Toggle auto-update / switch channel (admin only). */
  async updateConfig(body: UpdateConfigRequest): Promise<UpdateConfigResponse> {
    return this.api.put("/update/config", { body });
  }

  /** Recent install history entries (admin only). */
  async history(limit = 20): Promise<UpdateHistoryResponse> {
    return this.api.get("/update/history", { query: { limit } });
  }
}
