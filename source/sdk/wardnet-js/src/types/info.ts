/** Response for GET /api/info (unauthenticated). */
export interface InfoResponse {
  /**
   * Diagnostic version string — git-derived
   * `MAJOR.MINOR.PATCH[-dev.N+gHASH]`. Carries dev-suffix on non-tag
   * builds so logs and `--version` output identify the exact commit.
   * Use `release_version` for anything user-facing.
   */
  version: string;
  /**
   * Public-facing CalVer (`YYYY.MM.DD`). Stable across dev rebuilds —
   * this is the string the web UI displays and the auto-update runner
   * compares against the published manifest.
   */
  release_version: string;
  uptime_seconds: number;
}
