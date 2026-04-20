/** Discriminator for what kind of background work a job represents. */
export type JobKind = "blocklist_refresh";

/** Lifecycle state of a background job. */
export type JobStatus = "PENDING" | "RUNNING" | "SUCCEED" | "TERMINATED_WITH_ERRORS";

/** Snapshot of a background job, returned by `GET /api/jobs/:id`. */
export interface Job {
  id: string;
  kind: JobKind;
  status: JobStatus;
  /** Completion percentage, 0..=100. */
  percentage_done: number;
  /** Populated when `status == "TERMINATED_WITH_ERRORS"`. */
  error: string | null;
  created_at: string;
  updated_at: string;
}

/** Response body returned when a handler dispatches a background job
 *  instead of performing the work inline. Status code is `202 Accepted`. */
export interface JobDispatchedResponse {
  job_id: string;
}

/** True when the job has reached a terminal state. Pollers should stop once
 *  they observe this — subsequent calls may return 404 after the GC TTL. */
export function isTerminal(status: JobStatus): boolean {
  return status === "SUCCEED" || status === "TERMINATED_WITH_ERRORS";
}
