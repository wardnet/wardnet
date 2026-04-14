import type { WardnetClient } from "../client.js";
import type {
  DhcpConfigResponse,
  UpdateDhcpConfigRequest,
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
  constructor(private readonly client: WardnetClient) {}

  /** Get the current DHCP configuration (admin only). */
  async getConfig(): Promise<DhcpConfigResponse> {
    return this.client.request<DhcpConfigResponse>("/dhcp/config");
  }

  /** Update the DHCP pool configuration (admin only). */
  async updateConfig(body: UpdateDhcpConfigRequest): Promise<DhcpConfigResponse> {
    return this.client.request<DhcpConfigResponse>("/dhcp/config", {
      method: "PUT",
      body: JSON.stringify(body),
    });
  }

  /** Enable or disable the DHCP server (admin only). */
  async toggle(body: ToggleDhcpRequest): Promise<DhcpConfigResponse> {
    return this.client.request<DhcpConfigResponse>("/dhcp/config/toggle", {
      method: "POST",
      body: JSON.stringify(body),
    });
  }

  /** List all active DHCP leases (admin only). */
  async listLeases(): Promise<ListDhcpLeasesResponse> {
    return this.client.request<ListDhcpLeasesResponse>("/dhcp/leases");
  }

  /** Revoke an active DHCP lease (admin only). */
  async revokeLease(id: string): Promise<RevokeDhcpLeaseResponse> {
    return this.client.request<RevokeDhcpLeaseResponse>(`/dhcp/leases/${id}`, {
      method: "DELETE",
    });
  }

  /** List all static DHCP reservations (admin only). */
  async listReservations(): Promise<ListDhcpReservationsResponse> {
    return this.client.request<ListDhcpReservationsResponse>("/dhcp/reservations");
  }

  /** Create a new static MAC-to-IP reservation (admin only). */
  async createReservation(
    body: CreateDhcpReservationRequest,
  ): Promise<CreateDhcpReservationResponse> {
    return this.client.request<CreateDhcpReservationResponse>("/dhcp/reservations", {
      method: "POST",
      body: JSON.stringify(body),
    });
  }

  /** Delete a static DHCP reservation (admin only). */
  async deleteReservation(id: string): Promise<DeleteDhcpReservationResponse> {
    return this.client.request<DeleteDhcpReservationResponse>(`/dhcp/reservations/${id}`, {
      method: "DELETE",
    });
  }

  /** Get DHCP server status and pool usage (admin only). */
  async status(): Promise<DhcpStatusResponse> {
    return this.client.request<DhcpStatusResponse>("/dhcp/status");
  }
}
