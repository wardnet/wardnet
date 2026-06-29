import type { WardnetClient } from "../client.js";
import type {
  ConfigureCloudflareRequest,
  DdnsCheckResponse,
  DdnsEnrollRequest,
  DdnsEnrollmentCodeRequest,
  DdnsRegisterRequest,
  DdnsRegisterResponse,
  DdnsResolutionCheckResponse,
  DdnsStatusResponse,
  TlsStatusResponse,
} from "../types/remote-access.js";

/**
 * Remote-access service: the wardnet-cloud enrollment flow (request code →
 * enroll → check slug → register network), BYOD-Cloudflare configuration, and
 * TLS provisioning status. Drives the setup wizard's "Remote access" step and
 * the dashboard provisioning indicator. All endpoints are admin-authenticated.
 */
export class RemoteAccessService {
  constructor(private readonly client: WardnetClient) {}

  /**
   * Step 1 of the wardnet flow: request a one-time enrollment code be emailed
   * to the wardnet account. Stores nothing; the operator then submits the code
   * to {@link enroll}. Resolves on `204` (emailed if the account exists).
   */
  async requestEnrollmentCode(body: DdnsEnrollmentCodeRequest): Promise<void> {
    await this.client.request<void>("/ddns/enrollment-code", {
      method: "POST",
      body: JSON.stringify(body),
    });
  }

  /**
   * Step 2 of the wardnet flow: enroll this daemon against the emailed code,
   * binding its cloud identity to the tenant. Afterwards check slug
   * availability and {@link register} a network.
   */
  async enroll(body: DdnsEnrollRequest): Promise<void> {
    await this.client.request<void>("/ddns/enroll", {
      method: "POST",
      body: JSON.stringify(body),
    });
  }

  /** Check whether a vanity slug is available (requires a prior enroll). */
  async checkSlug(slug: string): Promise<DdnsCheckResponse> {
    const path = `/ddns/check?slug=${encodeURIComponent(slug)}`;
    return this.client.request<DdnsCheckResponse>(path);
  }

  /**
   * Final step of the wardnet flow: register a network under `slug` on the
   * lowest-latency region; issuance starts in the background.
   */
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
