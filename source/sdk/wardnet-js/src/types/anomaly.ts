/**
 * A class of anomaly, from the daemon's hardcoded catalogue.
 *
 * Deliberately open (`(string & {})`) so an older client keeps working when a
 * newer daemon adds a type: the union still gives autocomplete for the known
 * ones without making an unknown value a type error.
 */
export type AnomalyType =
  | "tunnel_start_failed"
  | "tunnel_unhealthy"
  | "update_failed"
  | "dhcp_conflict"
  | "route_table_lost"
  | "blocklist_refresh_failing"
  | "dns_upstream_unreachable"
  | (string & {});

/** How serious an anomaly is. */
export type AnomalySeverity = "error" | "warning" | "info";

/** Which anomalies a listing should return. */
export type AnomalyStatusFilter = "open" | "resolved" | "all";

/**
 * A typed, admin-facing condition with an open/resolved lifecycle.
 *
 * An anomaly is opened once when a condition is first observed, refreshed
 * while it persists, and resolved when it stops holding — so a problem that
 * lasts for days is one entry, not one per observation.
 */
export interface Anomaly {
  id: string;
  /** Stable machine-readable class identifier. */
  type: AnomalyType;
  severity: AnomalySeverity;
  /** The subsystem that raised it, e.g. `dns`, `tunnel`. */
  component: string;
  /**
   * The entity this anomaly is about — a tunnel id, a blocklist id — or `null`
   * for box-wide conditions. Filter a listing by it to ask what is currently
   * wrong with one entity.
   */
  subject_id: string | null;
  /** Plain-language description of what happened. */
  message: string;
  /** What the admin can do about it. */
  hint: string;
  /**
   * Detector-authored payload. Best-effort: no key is guaranteed beyond those
   * the owning detector documents. A `blocklist_refresh_failing` anomaly
   * carries `profile_id`, which is what makes a deep link to the list
   * possible.
   */
  details: Record<string, unknown> | null;
  /** ISO 8601 timestamp of when the anomaly was first raised. */
  opened_at: string;
  /** ISO 8601 timestamp of when the condition was last observed. */
  last_seen_at: string;
  /** How many times it has been observed since it opened. */
  occurrences: number;
  /** ISO 8601 timestamp, or `null` while the anomaly is still open. */
  resolved_at: string | null;
}

/** Response for GET /api/anomalies. */
export interface ListAnomaliesResponse {
  anomalies: Anomaly[];
}

/** Query parameters for GET /api/anomalies. */
export interface ListAnomaliesParams {
  /** Defaults to `open`. */
  status?: AnomalyStatusFilter;
  /** Restrict to anomalies about one entity. */
  subject_id?: string;
  /** Clamped by the daemon to 1..=200. Defaults to 50. */
  limit?: number;
}

/** Response for POST /api/anomalies/reevaluate. */
export interface ReevaluateAnomaliesResponse {
  /** How many open anomalies were examined. */
  evaluated: number;
  /** How many of those were closed as no longer holding. */
  resolved: number;
}
