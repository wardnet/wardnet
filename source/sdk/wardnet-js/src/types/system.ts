/** Response for GET /api/system/status. */
export interface SystemStatusResponse {
  version: string;
  uptime_seconds: number;
  device_count: number;
  tunnel_count: number;
  db_size_bytes: number;
  cpu_usage_percent: number;
  memory_used_bytes: number;
  memory_total_bytes: number;
}
