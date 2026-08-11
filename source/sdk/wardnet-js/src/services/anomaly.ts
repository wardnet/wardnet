import type { WardnetClient } from "../client.js";
import { apiClient, type ApiClient } from "../internal/client.js";
import type {
  ListAnomaliesParams,
  ListAnomaliesResponse,
  ReevaluateAnomaliesResponse,
} from "../types/anomaly.js";

/**
 * Anomalies — typed admin-facing conditions with an open/resolved lifecycle.
 *
 * Admin only.
 */
export class AnomalyService {
  private readonly api: ApiClient;

  constructor(client: WardnetClient) {
    this.api = apiClient(client);
  }

  /**
   * List anomalies, newest first. Defaults to the ones still open, which is
   * what a dashboard wants.
   *
   * Pass `subject_id` to ask what is currently wrong with one entity — a
   * tunnel view uses this to surface that tunnel's error, since the open
   * anomaly *is* the entity's current error.
   */
  async list(params: ListAnomaliesParams = {}): Promise<ListAnomaliesResponse> {
    return this.api.get("/anomalies", { query: { ...params } });
  }

  /**
   * Re-check every open anomaly against the world now, instead of waiting for
   * the next scheduled pass, and close the ones whose condition no longer
   * holds. Resolving an anomaly that was alerted also sends the matching
   * recovery notification.
   */
  async reevaluate(): Promise<ReevaluateAnomaliesResponse> {
    return this.api.post("/anomalies/reevaluate");
  }
}
