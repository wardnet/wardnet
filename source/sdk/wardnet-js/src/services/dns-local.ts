import type { WardnetClient } from "../client.js";
import type {
  ListZonesResponse,
  GetZoneResponse,
  CreateZoneRequest,
  CreateZoneResponse,
  UpdateZoneRequest,
  UpdateZoneResponse,
  DeleteZoneResponse,
  ListRecordsResponse,
  GetRecordResponse,
  CreateRecordRequest,
  CreateRecordResponse,
  UpdateRecordRequest,
  UpdateRecordResponse,
  DeleteRecordResponse,
  ListForwardingRulesResponse,
  GetForwardingRuleResponse,
  CreateForwardingRuleRequest,
  CreateForwardingRuleResponse,
  UpdateForwardingRuleRequest,
  UpdateForwardingRuleResponse,
  DeleteForwardingRuleResponse,
} from "../types/dns-local.js";

/**
 * Local DNS management — authoritative zones, custom records, and forwarding
 * rules (conditional forwarding). All endpoints are admin only.
 */
export class DnsLocalService {
  constructor(private readonly client: WardnetClient) {}

  // --- Zones ---

  async listZones(): Promise<ListZonesResponse> {
    return this.client.request<ListZonesResponse>("/dns/local/zones");
  }

  async getZone(id: string): Promise<GetZoneResponse> {
    return this.client.request<GetZoneResponse>(`/dns/local/zones/${id}`);
  }

  async createZone(body: CreateZoneRequest): Promise<CreateZoneResponse> {
    return this.client.request<CreateZoneResponse>("/dns/local/zones", {
      method: "POST",
      body: JSON.stringify(body),
    });
  }

  async updateZone(id: string, body: UpdateZoneRequest): Promise<UpdateZoneResponse> {
    return this.client.request<UpdateZoneResponse>(`/dns/local/zones/${id}`, {
      method: "PUT",
      body: JSON.stringify(body),
    });
  }

  async deleteZone(id: string): Promise<DeleteZoneResponse> {
    return this.client.request<DeleteZoneResponse>(`/dns/local/zones/${id}`, {
      method: "DELETE",
    });
  }

  /** List the records assigned to a zone. */
  async listZoneRecords(zoneId: string): Promise<ListRecordsResponse> {
    return this.client.request<ListRecordsResponse>(`/dns/local/zones/${zoneId}/records`);
  }

  // --- Records ---

  async listRecords(): Promise<ListRecordsResponse> {
    return this.client.request<ListRecordsResponse>("/dns/local/records");
  }

  async getRecord(id: string): Promise<GetRecordResponse> {
    return this.client.request<GetRecordResponse>(`/dns/local/records/${id}`);
  }

  async createRecord(body: CreateRecordRequest): Promise<CreateRecordResponse> {
    return this.client.request<CreateRecordResponse>("/dns/local/records", {
      method: "POST",
      body: JSON.stringify(body),
    });
  }

  async updateRecord(id: string, body: UpdateRecordRequest): Promise<UpdateRecordResponse> {
    return this.client.request<UpdateRecordResponse>(`/dns/local/records/${id}`, {
      method: "PUT",
      body: JSON.stringify(body),
    });
  }

  async deleteRecord(id: string): Promise<DeleteRecordResponse> {
    return this.client.request<DeleteRecordResponse>(`/dns/local/records/${id}`, {
      method: "DELETE",
    });
  }

  // --- Forwarding rules ---

  async listForwardingRules(): Promise<ListForwardingRulesResponse> {
    return this.client.request<ListForwardingRulesResponse>("/dns/local/forwarding");
  }

  async getForwardingRule(id: string): Promise<GetForwardingRuleResponse> {
    return this.client.request<GetForwardingRuleResponse>(`/dns/local/forwarding/${id}`);
  }

  async createForwardingRule(
    body: CreateForwardingRuleRequest,
  ): Promise<CreateForwardingRuleResponse> {
    return this.client.request<CreateForwardingRuleResponse>("/dns/local/forwarding", {
      method: "POST",
      body: JSON.stringify(body),
    });
  }

  async updateForwardingRule(
    id: string,
    body: UpdateForwardingRuleRequest,
  ): Promise<UpdateForwardingRuleResponse> {
    return this.client.request<UpdateForwardingRuleResponse>(`/dns/local/forwarding/${id}`, {
      method: "PUT",
      body: JSON.stringify(body),
    });
  }

  async deleteForwardingRule(id: string): Promise<DeleteForwardingRuleResponse> {
    return this.client.request<DeleteForwardingRuleResponse>(`/dns/local/forwarding/${id}`, {
      method: "DELETE",
    });
  }
}
