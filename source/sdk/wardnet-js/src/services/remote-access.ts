import type { WardnetClient } from "../client.js";
import { apiClient, type ApiClient } from "../internal/client.js";
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
  private readonly api: ApiClient;

  constructor(client: WardnetClient) {
    this.api = apiClient(client);
  }

  /**
   * Step 1 of the wardnet flow: request a one-time enrollment code be emailed
   * to the wardnet account. Stores nothing; the operator then submits the code
   * to {@link enroll}. Resolves on `204` (emailed if the account exists).
   */
  async requestEnrollmentCode(body: DdnsEnrollmentCodeRequest): Promise<void> {
    await this.api.post("/ddns/enrollment-code", { body });
  }

  /**
   * Step 2 of the wardnet flow: enroll this daemon against the emailed code,
   * binding its cloud identity to the tenant. Afterwards check slug
   * availability and {@link register} a network.
   */
  async enroll(body: DdnsEnrollRequest): Promise<void> {
    await this.api.post("/ddns/enroll", { body });
  }

  /** Check whether a vanity slug is available (requires a prior enroll). */
  async checkSlug(slug: string): Promise<DdnsCheckResponse> {
    return this.api.get("/ddns/check", { query: { slug } });
  }

  /**
   * Final step of the wardnet flow: register a network under `slug` on the
   * lowest-latency region; issuance starts in the background.
   */
  async register(body: DdnsRegisterRequest): Promise<DdnsRegisterResponse> {
    return this.api.post("/ddns/register", { body });
  }

  /** Configure BYOD-Cloudflare; issuance starts in the background. */
  async configureCloudflare(body: ConfigureCloudflareRequest): Promise<DdnsRegisterResponse> {
    return this.api.post("/ddns/cloudflare", { body });
  }

  /** Read the current DDNS configuration. */
  async ddnsStatus(): Promise<DdnsStatusResponse> {
    return this.api.get("/ddns/status");
  }

  /**
   * Check whether public DNS resolves the active FQDN to the published IP
   * (queries external resolvers, bypassing the local split-horizon override).
   */
  async resolutionCheck(): Promise<DdnsResolutionCheckResponse> {
    return this.api.get("/ddns/resolution-check");
  }

  /** Read the current TLS provisioning status (phase + cert details). */
  async tlsStatus(): Promise<TlsStatusResponse> {
    return this.api.get("/tls/status");
  }

  /**
   * Disable remote access: remove the published record + provider identity, drop
   * the certificate, and revert `:443` to the unprovisioned placeholder.
   */
  async teardown(): Promise<void> {
    await this.api.del("/ddns");
  }
}
