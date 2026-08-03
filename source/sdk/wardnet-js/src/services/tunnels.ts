import type { WardnetClient } from "../client.js";
import { apiClient, type ApiClient } from "../internal/client.js";
import type {
  CreateTunnelRequest,
  CreateTunnelResponse,
  DeleteTunnelResponse,
  ListTunnelsResponse,
  RebuildTunnelResponse,
  TunnelDetailResponse,
  TunnelDevicesResponse,
  TunnelSpeedTestHistoryResponse,
  TunnelTestResponse,
  UpdateTunnelDnsOverrideRequest,
  UpdateTunnelDnsOverrideResponse,
} from "../types/api.js";
import type { JobDispatchedResponse } from "../types/jobs.js";

/** Tunnel management service for the Wardnet daemon. */
export class TunnelService {
  private readonly api: ApiClient;

  constructor(client: WardnetClient) {
    this.api = apiClient(client);
  }

  /** List all configured tunnels (admin only). */
  async list(): Promise<ListTunnelsResponse> {
    return this.api.get("/tunnels");
  }

  /** Get one tunnel by ID (admin only). */
  async getById(id: string): Promise<TunnelDetailResponse> {
    return this.api.get("/tunnels/{id}", { path: { id } });
  }

  /** List the devices currently routed through a tunnel (admin only). */
  async listDevices(id: string): Promise<TunnelDevicesResponse> {
    const res = await this.api.get("/tunnels/{id}/devices", { path: { id } });
    // This endpoint returns the bare `Device` projection (no `dhcp_status` /
    // `current_rule`), while the public type reuses the fuller `Device`.
    // Preserve the long-standing public shape with a narrow cast.
    return { devices: res.devices as TunnelDevicesResponse["devices"] };
  }

  /** Import a tunnel from a WireGuard .conf file (admin only). */
  async create(body: CreateTunnelRequest): Promise<CreateTunnelResponse> {
    return this.api.post("/tunnels", { body });
  }

  /** Delete a tunnel and its configuration (admin only). */
  async delete(id: string): Promise<DeleteTunnelResponse> {
    return this.api.del("/tunnels/{id}", { path: { id } });
  }

  /**
   * Probe the tunnel for its exit IP, country, and latency (admin only).
   * The daemon brings the tunnel up if needed, runs a single HTTP probe
   * through the tunnel, then restores the prior up/down state.
   */
  async test(id: string): Promise<TunnelTestResponse> {
    return this.api.post("/tunnels/{id}/test", { path: { id } });
  }

  /**
   * Toggle the per-tunnel `override_default_dns` flag. When `true`, devices
   * routed through this tunnel resolve DNS via wardnet (so the ad-blocking
   * filter still runs) and wardnet forwards those queries to the tunnel's
   * DNS server bound to the tunnel interface. When `false`, the system-wide
   * upstream pool is used. See issue #342.
   */
  async setDnsOverride(
    id: string,
    body: UpdateTunnelDnsOverrideRequest,
  ): Promise<UpdateTunnelDnsOverrideResponse> {
    return this.api.put("/tunnels/{id}/dns-override", { path: { id }, body });
  }

  /**
   * Tear down and re-apply the WireGuard interface from persisted config
   * (admin only). Use when a tunnel is stale or devices cannot route through
   * it.
   */
  async rebuild(id: string): Promise<RebuildTunnelResponse> {
    return this.api.post("/tunnels/{id}/rebuild", { path: { id } });
  }

  /**
   * Start a speed test for a tunnel (admin only). Returns a job id (202);
   * poll `jobs.get(job_id)` for progress. The background job measures
   * throughput, latency and jitter twice — once over the direct (WAN) path
   * and once through the tunnel — so the result shows how much of the line
   * the VPN preserves. A concurrent run on the same tunnel returns 409.
   */
  async startSpeedTest(id: string): Promise<JobDispatchedResponse> {
    return this.api.post("/tunnels/{id}/speed-test", { path: { id } });
  }

  /**
   * List recent speed test results for a tunnel, newest first (admin only).
   * Each result carries both the direct (WAN) and tunnel measurements.
   */
  async getSpeedTestResults(id: string): Promise<TunnelSpeedTestHistoryResponse> {
    return this.api.get("/tunnels/{id}/speed-test/results", { path: { id } });
  }
}
