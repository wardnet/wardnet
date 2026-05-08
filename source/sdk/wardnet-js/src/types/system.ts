/** Response for GET /api/system/status. */
export interface SystemStatusResponse {
  /** Diagnostic git-derived version. See `InfoResponse.version`. */
  version: string;
  /** Public-facing CalVer. See `InfoResponse.release_version`. */
  release_version: string;
  uptime_seconds: number;
  device_count: number;
  tunnel_count: number;
  db_size_bytes: number;
  cpu_usage_percent: number;
  memory_used_bytes: number;
  memory_total_bytes: number;
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
