/** Response for GET /api/info (unauthenticated). */
export interface InfoResponse {
  version: string;
  uptime_seconds: number;
}
