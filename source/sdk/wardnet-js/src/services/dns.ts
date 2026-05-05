import type { WardnetClient } from "../client.js";
import type {
  DnsConfigResponse,
  UpdateDnsConfigRequest,
  ToggleDnsRequest,
  DnsStatusResponse,
  DnsCacheFlushResponse,
  ListBlocklistsResponse,
  CreateBlocklistRequest,
  CreateBlocklistResponse,
  UpdateBlocklistRequest,
  UpdateBlocklistResponse,
  DeleteBlocklistResponse,
  ListAllowlistResponse,
  CreateAllowlistRequest,
  CreateAllowlistResponse,
  DeleteAllowlistResponse,
  ListFilterRulesResponse,
  CreateFilterRuleRequest,
  CreateFilterRuleResponse,
  UpdateFilterRuleRequest,
  UpdateFilterRuleResponse,
  DeleteFilterRuleResponse,
  ListQueryLogParams,
  ListQueryLogResponse,
  DnsStatsResponse,
} from "../types/dns.js";
import type { JobDispatchedResponse } from "../types/jobs.js";

/** DNS server management service for the Wardnet daemon. */
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

  // --- Blocklists ---

  /** List all blocklists (admin only). */
  async listBlocklists(): Promise<ListBlocklistsResponse> {
    return this.client.request<ListBlocklistsResponse>("/dns/blocklists");
  }

  /** Add a new blocklist (admin only). */
  async createBlocklist(body: CreateBlocklistRequest): Promise<CreateBlocklistResponse> {
    return this.client.request<CreateBlocklistResponse>("/dns/blocklists", {
      method: "POST",
      body: JSON.stringify(body),
    });
  }

  /** Update a blocklist (admin only). */
  async updateBlocklist(
    id: string,
    body: UpdateBlocklistRequest,
  ): Promise<UpdateBlocklistResponse> {
    return this.client.request<UpdateBlocklistResponse>(`/dns/blocklists/${id}`, {
      method: "PUT",
      body: JSON.stringify(body),
    });
  }

  /** Delete a blocklist (admin only). */
  async deleteBlocklist(id: string): Promise<DeleteBlocklistResponse> {
    return this.client.request<DeleteBlocklistResponse>(`/dns/blocklists/${id}`, {
      method: "DELETE",
    });
  }

  /** Force-refresh a blocklist now (admin only).
   *
   *  Dispatches a background job that fetches, parses, and stores the
   *  blocklist. Returns immediately with the job id; poll `JobsService.get`
   *  for progress and completion. */
  async updateBlocklistNow(id: string): Promise<JobDispatchedResponse> {
    return this.client.request<JobDispatchedResponse>(`/dns/blocklists/${id}/update`, {
      method: "POST",
    });
  }

  // --- Allowlist ---

  /** List all allowlist entries (admin only). */
  async listAllowlist(): Promise<ListAllowlistResponse> {
    return this.client.request<ListAllowlistResponse>("/dns/allowlist");
  }

  /** Add a domain to the allowlist (admin only). */
  async createAllowlistEntry(body: CreateAllowlistRequest): Promise<CreateAllowlistResponse> {
    return this.client.request<CreateAllowlistResponse>("/dns/allowlist", {
      method: "POST",
      body: JSON.stringify(body),
    });
  }

  /** Remove an allowlist entry (admin only). */
  async deleteAllowlistEntry(id: string): Promise<DeleteAllowlistResponse> {
    return this.client.request<DeleteAllowlistResponse>(`/dns/allowlist/${id}`, {
      method: "DELETE",
    });
  }

  // --- Custom filter rules ---

  /** List all custom filter rules (admin only). */
  async listFilterRules(): Promise<ListFilterRulesResponse> {
    return this.client.request<ListFilterRulesResponse>("/dns/rules");
  }

  /** Add a custom filter rule (admin only). */
  async createFilterRule(body: CreateFilterRuleRequest): Promise<CreateFilterRuleResponse> {
    return this.client.request<CreateFilterRuleResponse>("/dns/rules", {
      method: "POST",
      body: JSON.stringify(body),
    });
  }

  /** Update a custom filter rule (admin only). */
  async updateFilterRule(
    id: string,
    body: UpdateFilterRuleRequest,
  ): Promise<UpdateFilterRuleResponse> {
    return this.client.request<UpdateFilterRuleResponse>(`/dns/rules/${id}`, {
      method: "PUT",
      body: JSON.stringify(body),
    });
  }

  /** Delete a custom filter rule (admin only). */
  async deleteFilterRule(id: string): Promise<DeleteFilterRuleResponse> {
    return this.client.request<DeleteFilterRuleResponse>(`/dns/rules/${id}`, {
      method: "DELETE",
    });
  }

  // --- Query log + stats ---

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

  /** Aggregated DNS stats over the last `hours` hours (admin only). */
  async getStats(hours = 24): Promise<DnsStatsResponse> {
    return this.client.request<DnsStatsResponse>(`/dns/stats?hours=${hours}`);
  }
}
