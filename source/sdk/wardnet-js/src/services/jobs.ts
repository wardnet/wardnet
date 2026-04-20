import type { WardnetClient } from "../client.js";
import type { Job } from "../types/jobs.js";

/** Service for polling the status of background jobs dispatched by async
 *  endpoints (e.g. blocklist refresh). */
export class JobsService {
  constructor(private readonly client: WardnetClient) {}

  /** Look up a job by id. Throws `WardnetApiError` with status 404 if the
   *  job is unknown (never dispatched, or GC'd after its TTL). */
  async get(id: string): Promise<Job> {
    return this.client.request<Job>(`/jobs/${id}`, { method: "GET" });
  }
}
