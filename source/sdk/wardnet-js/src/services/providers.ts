import type { WardnetClient } from "../client.js";
import type {
  ListCountriesResponse,
  ListProvidersResponse,
  ListServersRequest,
  ListServersResponse,
  SetupProviderRequest,
  SetupProviderResponse,
  ValidateCredentialsRequest,
  ValidateCredentialsResponse,
} from "../types/api.js";

/** VPN provider management service for the Wardnet daemon. */
export class ProviderService {
  constructor(private readonly client: WardnetClient) {}

  /** List all registered VPN providers (admin only). */
  async list(): Promise<ListProvidersResponse> {
    return this.client.request<ListProvidersResponse>("/providers");
  }

  /** List countries where a provider has servers (admin only). */
  async listCountries(providerId: string): Promise<ListCountriesResponse> {
    return this.client.request<ListCountriesResponse>(`/providers/${providerId}/countries`);
  }

  /** Validate credentials against a provider (admin only). */
  async validateCredentials(
    providerId: string,
    body: ValidateCredentialsRequest,
  ): Promise<ValidateCredentialsResponse> {
    return this.client.request<ValidateCredentialsResponse>(`/providers/${providerId}/validate`, {
      method: "POST",
      body: JSON.stringify(body),
    });
  }

  /** List available servers from a provider (admin only). */
  async listServers(providerId: string, body: ListServersRequest): Promise<ListServersResponse> {
    return this.client.request<ListServersResponse>(`/providers/${providerId}/servers`, {
      method: "POST",
      body: JSON.stringify(body),
    });
  }

  /** Set up a tunnel through a provider (admin only). */
  async setupTunnel(
    providerId: string,
    body: SetupProviderRequest,
  ): Promise<SetupProviderResponse> {
    return this.client.request<SetupProviderResponse>(`/providers/${providerId}/setup`, {
      method: "POST",
      body: JSON.stringify(body),
    });
  }
}
