import type { WardnetClient } from "../client.js";
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
  constructor(private readonly client: WardnetClient) {}

  /** Get the current DNS configuration (admin only). */
  async getConfig(): Promise<DnsConfigResponse> {
    return this.client.request<DnsConfigResponse>("/dns/config");
  }

  /** Update the DNS configuration (admin only). */
  async updateConfig(body: UpdateDnsConfigRequest): Promise<DnsConfigResponse> {
    return this.client.request<DnsConfigResponse>("/dns/config", {
      method: "PUT",
      body: JSON.stringify(body),
    });
  }

  /** Enable or disable the DNS server (admin only). */
  async toggle(body: ToggleDnsRequest): Promise<DnsConfigResponse> {
    return this.client.request<DnsConfigResponse>("/dns/config/toggle", {
      method: "POST",
      body: JSON.stringify(body),
    });
  }

  /** Get DNS server status and cache metrics (admin only). */
  async status(): Promise<DnsStatusResponse> {
    return this.client.request<DnsStatusResponse>("/dns/status");
  }

  /** Flush the DNS cache (admin only). */
  async flushCache(): Promise<DnsCacheFlushResponse> {
    return this.client.request<DnsCacheFlushResponse>("/dns/cache/flush", {
      method: "POST",
    });
  }

  // --- Query log ---

  /** Paginated DNS query log (admin only). */
  async listQueryLog(params: ListQueryLogParams = {}): Promise<ListQueryLogResponse> {
    // Hand-built query string — the SDK targets a tsconfig without the
    // DOM lib, so `URLSearchParams` isn't available here.
    const parts: string[] = [];
    const enc = encodeURIComponent;
    if (params.limit !== undefined) parts.push(`limit=${params.limit}`);
    if (params.offset !== undefined) parts.push(`offset=${params.offset}`);
    if (params.domain) parts.push(`domain=${enc(params.domain)}`);
    if (params.client_ip) parts.push(`client_ip=${enc(params.client_ip)}`);
    if (params.result) parts.push(`result=${enc(params.result)}`);
    const path = parts.length === 0 ? "/dns/log" : `/dns/log?${parts.join("&")}`;
    return this.client.request<ListQueryLogResponse>(path);
  }
}
