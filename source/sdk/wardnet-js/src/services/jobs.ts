import type { WardnetClient } from "../client.js";
import { apiClient, type ApiClient } from "../internal/client.js";
import type { Job } from "../types/jobs.js";

/** Service for polling the status of background jobs dispatched by async
 *  endpoints (e.g. blocklist refresh). */
export class JobsService {
  private readonly api: ApiClient;

  constructor(client: WardnetClient) {
    this.api = apiClient(client);
  }

  /** Look up a job by id. Throws `WardnetApiError` with status 404 if the
   *  job is unknown (never dispatched, or GC'd after its TTL). */
  async get(id: string): Promise<Job> {
    return this.api.get("/jobs/{id}", { path: { id } });
  }
}
