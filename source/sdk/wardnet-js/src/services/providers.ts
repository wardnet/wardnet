import type { WardnetClient } from "../client.js";
import { apiClient, type ApiClient } from "../internal/client.js";
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
  private readonly api: ApiClient;

  constructor(client: WardnetClient) {
    this.api = apiClient(client);
  }

  /** List all registered VPN providers (admin only). */
  async list(): Promise<ListProvidersResponse> {
    return this.api.get("/providers");
  }

  /** List countries where a provider has servers (admin only). */
  async listCountries(providerId: string): Promise<ListCountriesResponse> {
    return this.api.get("/providers/{id}/countries", { path: { id: providerId } });
  }

  /** Validate credentials against a provider (admin only). */
  async validateCredentials(
    providerId: string,
    body: ValidateCredentialsRequest,
  ): Promise<ValidateCredentialsResponse> {
    return this.api.post("/providers/{id}/validate", { path: { id: providerId }, body });
  }

  /** List available servers from a provider (admin only). */
  async listServers(providerId: string, body: ListServersRequest): Promise<ListServersResponse> {
    return this.api.post("/providers/{id}/servers", { path: { id: providerId }, body });
  }

  /** Set up a tunnel through a provider (admin only). */
  async setupTunnel(
    providerId: string,
    body: SetupProviderRequest,
  ): Promise<SetupProviderResponse> {
    return this.api.post("/providers/{id}/setup", { path: { id: providerId }, body });
  }
}
