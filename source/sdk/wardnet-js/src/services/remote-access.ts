import type { WardnetClient } from "../client.js";
import type {
  ConfigureCloudflareRequest,
  DdnsCheckResponse,
  DdnsRegisterRequest,
  DdnsRegisterResponse,
  DdnsResolutionCheckResponse,
  DdnsStatusResponse,
  TlsStatusResponse,
} from "../types/remote-access.js";

/**
 * Remote-access service: DDNS registration (bridge / BYOD-Cloudflare) and TLS
 * provisioning status. Drives the setup wizard's "Remote access" step and the
 * dashboard provisioning indicator. All endpoints are admin-authenticated.
 */
export class RemoteAccessService {
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

  /**
   * Check whether public DNS resolves the active FQDN to the published IP
   * (queries external resolvers, bypassing the local split-horizon override).
   */
  async resolutionCheck(): Promise<DdnsResolutionCheckResponse> {
    return this.client.request<DdnsResolutionCheckResponse>("/ddns/resolution-check");
  }

  /** Read the current TLS provisioning status (phase + cert details). */
  async tlsStatus(): Promise<TlsStatusResponse> {
    return this.client.request<TlsStatusResponse>("/tls/status");
  }

  /**
   * Disable remote access: remove the published record + provider identity, drop
   * the certificate, and revert `:443` to the unprovisioned placeholder.
   */
  async teardown(): Promise<void> {
    await this.client.request<void>("/ddns", { method: "DELETE" });
  }
}
