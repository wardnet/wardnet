import type { WardnetClient } from "../client.js";
import type {
  DnsConfigResponse,
  UpdateDnsConfigRequest,
  ToggleDnsRequest,
  DnsStatusResponse,
  DnsCacheFlushResponse,
} from "../types/dns.js";

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
}
