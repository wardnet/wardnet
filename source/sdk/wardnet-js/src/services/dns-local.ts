import type { WardnetClient } from "../client.js";
import { apiClient, type ApiClient } from "../internal/client.js";
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
  private readonly api: ApiClient;

  constructor(client: WardnetClient) {
    this.api = apiClient(client);
  }

  // --- Zones ---

  async listZones(): Promise<ListZonesResponse> {
    return this.api.get("/dns/local/zones");
  }

  async getZone(id: string): Promise<GetZoneResponse> {
    return this.api.get("/dns/local/zones/{id}", { path: { id } });
  }

  async createZone(body: CreateZoneRequest): Promise<CreateZoneResponse> {
    return this.api.post("/dns/local/zones", { body });
  }

  async updateZone(id: string, body: UpdateZoneRequest): Promise<UpdateZoneResponse> {
    return this.api.put("/dns/local/zones/{id}", { path: { id }, body });
  }

  async deleteZone(id: string): Promise<DeleteZoneResponse> {
    return this.api.del("/dns/local/zones/{id}", { path: { id } });
  }

  /** List the records assigned to a zone. */
  async listZoneRecords(zoneId: string): Promise<ListRecordsResponse> {
    return this.api.get("/dns/local/zones/{id}/records", { path: { id: zoneId } });
  }

  // --- Records ---

  async listRecords(): Promise<ListRecordsResponse> {
    return this.api.get("/dns/local/records");
  }

  async getRecord(id: string): Promise<GetRecordResponse> {
    return this.api.get("/dns/local/records/{id}", { path: { id } });
  }

  async createRecord(body: CreateRecordRequest): Promise<CreateRecordResponse> {
    return this.api.post("/dns/local/records", { body });
  }

  async updateRecord(id: string, body: UpdateRecordRequest): Promise<UpdateRecordResponse> {
    return this.api.put("/dns/local/records/{id}", { path: { id }, body });
  }

  async deleteRecord(id: string): Promise<DeleteRecordResponse> {
    return this.api.del("/dns/local/records/{id}", { path: { id } });
  }

  // --- Forwarding rules ---

  async listForwardingRules(): Promise<ListForwardingRulesResponse> {
    return this.api.get("/dns/local/forwarding");
  }

  async getForwardingRule(id: string): Promise<GetForwardingRuleResponse> {
    return this.api.get("/dns/local/forwarding/{id}", { path: { id } });
  }

  async createForwardingRule(
    body: CreateForwardingRuleRequest,
  ): Promise<CreateForwardingRuleResponse> {
    return this.api.post("/dns/local/forwarding", { body });
  }

  async updateForwardingRule(
    id: string,
    body: UpdateForwardingRuleRequest,
  ): Promise<UpdateForwardingRuleResponse> {
    return this.api.put("/dns/local/forwarding/{id}", { path: { id }, body });
  }

  async deleteForwardingRule(id: string): Promise<DeleteForwardingRuleResponse> {
    return this.api.del("/dns/local/forwarding/{id}", { path: { id } });
  }
}
