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
  /**
   * Whether this box is currently entitled to the premium app surfaces (the
   * user PWA and admin mobile app) — on the wardnet DDNS provider and not
   * suspended. Used to self-gate an already-installed PWA served from its
   * service-worker precache, which never re-hits the server-side serving
   * gate.
   */
  entitled: boolean;
}
