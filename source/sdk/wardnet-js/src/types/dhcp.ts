/** DHCP pool configuration. */
export interface DhcpConfig {
  enabled: boolean;
  /** Wardnet's own LAN IP — auto-detected, advertised as gateway and DNS. */
  gateway_ip: string;
  pool_start: string;
  pool_end: string;
  subnet_mask: string;
  upstream_dns: string[];
  lease_duration_secs: number;
  router_ip: string | null;
}

/** Status of a DHCP lease. */
export type DhcpLeaseStatus = "active" | "expired" | "released";

/** An active or historical DHCP lease. */
export interface DhcpLease {
  id: string;
  mac_address: string;
  ip_address: string;
  hostname: string | null;
  lease_start: string;
  lease_end: string;
  status: DhcpLeaseStatus;
  device_id: string | null;
  created_at: string;
  updated_at: string;
}

/** A static MAC-to-IP reservation. */
export interface DhcpReservation {
  id: string;
  mac_address: string;
  ip_address: string;
  hostname: string | null;
  description: string | null;
  created_at: string;
  updated_at: string;
}

/** Response for GET /api/dhcp/config. */
export interface DhcpConfigResponse {
  config: DhcpConfig;
}

/** Request body for PUT /api/dhcp/config. */
export interface UpdateDhcpConfigRequest {
  pool_start: string;
  pool_end: string;
  subnet_mask: string;
  upstream_dns: string[];
  lease_duration_secs: number;
  router_ip?: string;
}

/** Request body for POST /api/dhcp/config/toggle. */
export interface ToggleDhcpRequest {
  enabled: boolean;
}

/** Response for GET /api/dhcp/leases. */
export interface ListDhcpLeasesResponse {
  leases: DhcpLease[];
}

/** Response for GET /api/dhcp/reservations. */
export interface ListDhcpReservationsResponse {
  reservations: DhcpReservation[];
}

/** Request body for POST /api/dhcp/reservations. */
export interface CreateDhcpReservationRequest {
  mac_address: string;
  ip_address: string;
  hostname?: string;
  description?: string;
}

/** Response for POST /api/dhcp/reservations. */
export interface CreateDhcpReservationResponse {
  reservation: DhcpReservation;
  message: string;
}

/** Response for DELETE /api/dhcp/reservations/:id. */
export interface DeleteDhcpReservationResponse {
  message: string;
}

/** Response for GET /api/dhcp/status. */
export interface DhcpStatusResponse {
  enabled: boolean;
  running: boolean;
  active_lease_count: number;
  pool_total: number;
  pool_used: number;
}

/** Response for DELETE /api/dhcp/leases/:id. */
export interface RevokeDhcpLeaseResponse {
  message: string;
}
