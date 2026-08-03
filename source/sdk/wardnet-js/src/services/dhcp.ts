import type { WardnetClient } from "../client.js";
import { apiClient, type ApiClient } from "../internal/client.js";
import type {
  DhcpConfigResponse,
  UpdateDhcpConfigRequest,
  PreviewDhcpConfigRequest,
  PreviewDhcpConfigResponse,
  ToggleDhcpRequest,
  ListDhcpLeasesResponse,
  ListDhcpReservationsResponse,
  CreateDhcpReservationRequest,
  CreateDhcpReservationResponse,
  DeleteDhcpReservationResponse,
  DhcpStatusResponse,
  RevokeDhcpLeaseResponse,
} from "../types/dhcp.js";

/** DHCP server management service for the Wardnet daemon. */
export class DhcpService {
  private readonly api: ApiClient;

  constructor(client: WardnetClient) {
    this.api = apiClient(client);
  }

  /** Get the current DHCP configuration (admin only). */
  async getConfig(): Promise<DhcpConfigResponse> {
    return this.api.get("/dhcp/config");
  }

  /** Update the DHCP pool configuration (admin only). */
  async updateConfig(body: UpdateDhcpConfigRequest): Promise<DhcpConfigResponse> {
    return this.api.put("/dhcp/config", { body });
  }

  /**
   * Dry-run a pool-range change: report the active leases that would be
   * revoked if the pool were changed to the given range (admin only). Mutates
   * nothing — used to warn before saving.
   */
  async previewConfig(body: PreviewDhcpConfigRequest): Promise<PreviewDhcpConfigResponse> {
    return this.api.post("/dhcp/config/preview", { body });
  }

  /** Enable or disable the DHCP server (admin only). */
  async toggle(body: ToggleDhcpRequest): Promise<DhcpConfigResponse> {
    return this.api.post("/dhcp/config/toggle", { body });
  }

  /** List all active DHCP leases (admin only). */
  async listLeases(): Promise<ListDhcpLeasesResponse> {
    return this.api.get("/dhcp/leases");
  }

  /** Revoke an active DHCP lease (admin only). */
  async revokeLease(id: string): Promise<RevokeDhcpLeaseResponse> {
    return this.api.del("/dhcp/leases/{id}", { path: { id } });
  }

  /** List all static DHCP reservations (admin only). */
  async listReservations(): Promise<ListDhcpReservationsResponse> {
    return this.api.get("/dhcp/reservations");
  }

  /** Create a new static MAC-to-IP reservation (admin only). */
  async createReservation(
    body: CreateDhcpReservationRequest,
  ): Promise<CreateDhcpReservationResponse> {
    return this.api.post("/dhcp/reservations", { body });
  }

  /** Delete a static DHCP reservation (admin only). */
  async deleteReservation(id: string): Promise<DeleteDhcpReservationResponse> {
    return this.api.del("/dhcp/reservations/{id}", { path: { id } });
  }

  /** Get DHCP server status and pool usage (admin only). */
  async status(): Promise<DhcpStatusResponse> {
    return this.api.get("/dhcp/status");
  }
}
