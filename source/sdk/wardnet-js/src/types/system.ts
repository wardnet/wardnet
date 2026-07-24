/**
 * Classification of the previous daemon shutdown:
 * - `unknown` — no prior shutdown record (first-ever boot or fresh DB).
 * - `graceful` — daemon recorded a graceful exit before the previous run ended.
 * - `unclean` — the previous run was interrupted by a crash, kill -9, or power loss.
 */
export type LastShutdownState = "unknown" | "graceful" | "unclean";

/**
 * Status of the previous daemon shutdown.
 *
 * `at` is the timestamp the previous run last touched the database
 * (graceful exit time, or last heartbeat for unclean events). `at` is
 * `null` only for `unknown`. `acknowledged_at` is set when an admin
 * dismisses the unclean-shutdown banner via
 * `POST /api/system/shutdown/acknowledge`. The banner is hidden iff
 * `acknowledged_at >= at`.
 */
export interface LastShutdownStatus {
  state: LastShutdownState;
  at: string | null;
  acknowledged_at: string | null;
}

/** Response for GET /api/system/status. */
export interface SystemStatusResponse {
  /** Diagnostic git-derived version. See `InfoResponse.version`. */
  version: string;
  /** Public-facing CalVer. See `InfoResponse.release_version`. */
  release_version: string;
  uptime_seconds: number;
  device_count: number;
  tunnel_count: number;
  tunnel_active_count: number;
  db_size_bytes: number;
  cpu_usage_percent: number;
  memory_used_bytes: number;
  memory_total_bytes: number;
  disk_free_bytes: number;
  disk_total_bytes: number;
  /** Classification of the previous daemon shutdown plus its acknowledgement. */
  last_shutdown: LastShutdownStatus;
}

/**
 * Request body for PUT /api/system/default-policy.
 *
 * `policy` is either the literal string `"direct"` or a tunnel UUID.
 */
export interface SetDefaultPolicyRequest {
  policy: string;
}

/** Response for GET / PUT /api/system/default-policy. */
export interface SetDefaultPolicyResponse {
  policy: string;
}

/**
 * A structured, admin-facing problem report raised by a daemon component,
 * paired with a plain-language message and a remediation hint. Mirrors the
 * daemon's `ApiDiagnostic`.
 */
export interface SystemDiagnostic {
  /** ISO 8601 timestamp of when the underlying condition occurred. */
  timestamp: string;
  /** Stable machine-readable class identifier, e.g. `tunnel_start_failed`. */
  code: string;
  /** One of `error`, `warning`, `info`. */
  severity: string;
  /** The subsystem that raised it. */
  component: string;
  /** Plain-language description of what happened. */
  message: string;
  /** What the admin can do about it. */
  hint: string;
}

/**
 * Response for GET /api/system/errors — the most recent admin-facing
 * diagnostics from the daemon (currently the last 15 entries).
 */
export interface RecentErrorsResponse {
  errors: SystemDiagnostic[];
}
