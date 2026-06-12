// Local DNS — authoritative zones, custom records, forwarding rules (issue
// #217). Mirrors the Rust `wardnet-common` domain + API DTOs. CRUD surface
// under /api/dns/local/...

/** DNS record type for custom local records. Wire form is SCREAMING_SNAKE_CASE. */
export type DnsRecordType = "A" | "AAAA" | "CNAME" | "TXT" | "MX" | "SRV";

/**
 * Provenance of a custom local DNS record. `manual` = admin-created,
 * `dhcp` = auto-registered from a DHCP lease ({hostname}.lan), `system` =
 * daemon-seeded.
 */
export type DnsRecordSource = "manual" | "dhcp" | "system";

/**
 * Provenance of an authoritative local DNS zone. `manual` = admin-created and
 * freely deletable; `system` = daemon-seeded (the `.lan` zone) and cannot be
 * deleted (the API rejects it). Zones are never DHCP-sourced.
 */
export type DnsZoneSource = "manual" | "system";

/** An authoritative local DNS zone (e.g. "lan", "home"). */
export interface DnsZone {
  id: string;
  name: string;
  enabled: boolean;
  source: DnsZoneSource;
  created_at: string;
  updated_at: string;
}

/** A user-defined local DNS record. */
export interface CustomDnsRecord {
  id: string;
  /** Owning zone, or null for an unzoned record. */
  zone_id?: string | null;
  domain: string;
  record_type: DnsRecordType;
  value: string;
  ttl: number;
  enabled: boolean;
  source: DnsRecordSource;
  created_at: string;
  updated_at: string;
}

/** A conditional-forwarding rule (domain → upstream override). */
export interface ConditionalForwardingRule {
  id: string;
  domain: string;
  upstream: string;
  enabled: boolean;
  created_at: string;
}

// --- Zones ---

export interface ListZonesResponse {
  zones: DnsZone[];
}

export interface GetZoneResponse {
  zone: DnsZone;
}

export interface CreateZoneRequest {
  name: string;
  enabled: boolean;
}

export interface CreateZoneResponse {
  zone: DnsZone;
  message: string;
}

export interface UpdateZoneRequest {
  name?: string;
  enabled?: boolean;
}

export interface UpdateZoneResponse {
  zone: DnsZone;
  message: string;
}

export interface DeleteZoneResponse {
  message: string;
}

// --- Records ---

export interface ListRecordsResponse {
  records: CustomDnsRecord[];
}

export interface GetRecordResponse {
  record: CustomDnsRecord;
}

export interface CreateRecordRequest {
  /** Optional owning zone; null/omitted creates an unzoned record. */
  zone_id?: string | null;
  domain: string;
  record_type: DnsRecordType;
  value: string;
  ttl: number;
  enabled: boolean;
}

export interface UpdateRecordRequest {
  /** Omitted = leave zone link unchanged, null = clear, value = reassign. */
  zone_id?: string | null;
  domain?: string;
  record_type?: DnsRecordType;
  value?: string;
  ttl?: number;
  enabled?: boolean;
}

export interface CreateRecordResponse {
  record: CustomDnsRecord;
  message: string;
}

export interface UpdateRecordResponse {
  record: CustomDnsRecord;
  message: string;
}

export interface DeleteRecordResponse {
  message: string;
}

// --- Forwarding rules ---

export interface ListForwardingRulesResponse {
  rules: ConditionalForwardingRule[];
}

export interface GetForwardingRuleResponse {
  rule: ConditionalForwardingRule;
}

export interface CreateForwardingRuleRequest {
  domain: string;
  upstream: string;
  enabled: boolean;
}

export interface UpdateForwardingRuleRequest {
  domain?: string;
  upstream?: string;
  enabled?: boolean;
}

export interface CreateForwardingRuleResponse {
  rule: ConditionalForwardingRule;
  message: string;
}

export interface UpdateForwardingRuleResponse {
  rule: ConditionalForwardingRule;
  message: string;
}

export interface DeleteForwardingRuleResponse {
  message: string;
}
