import type { WardnetClient } from "../client.js";
import { apiClient, type ApiClient } from "../internal/client.js";
import type {
  DnsConfigResponse,
  UpdateDnsConfigRequest,
  ToggleDnsRequest,
  DnsStatusResponse,
  DnsCacheFlushResponse,
  ListQueryLogParams,
  ListQueryLogResponse,
} from "../types/dns.js";

/** DNS server management — config, status, cache, query log, stats.
 *
 *  Filtering (profiles, blocklists, allowlist, rules, per-device settings) lives
 *  in `DnsFilterService`. */
export class DnsService {
  private readonly api: ApiClient;

  constructor(client: WardnetClient) {
    this.api = apiClient(client);
  }

  /** Get the current DNS configuration (admin only). */
  async getConfig(): Promise<DnsConfigResponse> {
    return this.api.get("/dns/config");
  }

  /** Update the DNS configuration (admin only). */
  async updateConfig(body: UpdateDnsConfigRequest): Promise<DnsConfigResponse> {
    return this.api.put("/dns/config", { body });
  }

  /** Enable or disable the DNS server (admin only). */
  async toggle(body: ToggleDnsRequest): Promise<DnsConfigResponse> {
    return this.api.post("/dns/config/toggle", { body });
  }

  /** Get DNS server status and cache metrics (admin only). */
  async status(): Promise<DnsStatusResponse> {
    return this.api.get("/dns/status");
  }

  /** Flush the DNS cache (admin only). */
  async flushCache(): Promise<DnsCacheFlushResponse> {
    return this.api.post("/dns/cache/flush");
  }

  // --- Query log ---

  /** Paginated DNS query log (admin only). */
  async listQueryLog(params: ListQueryLogParams = {}): Promise<ListQueryLogResponse> {
    return this.api.get("/dns/log", { query: params });
  }
}
