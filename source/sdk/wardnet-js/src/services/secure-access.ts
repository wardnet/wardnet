import type { WardnetClient } from "../client.js";
import type {
  ConfigureCloudflareRequest,
  DdnsCheckResponse,
  DdnsRegisterRequest,
  DdnsRegisterResponse,
  DdnsStatusResponse,
  TlsStatusResponse,
} from "../types/secure-access.js";

/**
 * Secure-access service: DDNS registration (bridge / BYOD-Cloudflare) and TLS
 * provisioning status. Drives the setup wizard's "Secure access" step and the
 * dashboard provisioning indicator. All endpoints are admin-authenticated.
 */
export class SecureAccessService {
  constructor(private readonly client: WardnetClient) {}

  /** Check whether a bridge short name is available. */
  async checkName(name: string): Promise<DdnsCheckResponse> {
    const path = `/ddns/check?name=${encodeURIComponent(name)}`;
    return this.client.request<DdnsCheckResponse>(path);
  }

  /** Register on the bridge under `name`; issuance starts in the background. */
  async register(body: DdnsRegisterRequest): Promise<DdnsRegisterResponse> {
    return this.client.request<DdnsRegisterResponse>("/ddns/register", {
      method: "POST",
      body: JSON.stringify(body),
    });
  }

  /** Configure BYOD-Cloudflare; issuance starts in the background. */
  async configureCloudflare(body: ConfigureCloudflareRequest): Promise<DdnsRegisterResponse> {
    return this.client.request<DdnsRegisterResponse>("/ddns/cloudflare", {
      method: "POST",
      body: JSON.stringify(body),
    });
  }

  /** Read the current DDNS configuration. */
  async ddnsStatus(): Promise<DdnsStatusResponse> {
    return this.client.request<DdnsStatusResponse>("/ddns/status");
  }

  /** Read the current TLS provisioning status (phase + cert details). */
  async tlsStatus(): Promise<TlsStatusResponse> {
    return this.client.request<TlsStatusResponse>("/tls/status");
  }
}
