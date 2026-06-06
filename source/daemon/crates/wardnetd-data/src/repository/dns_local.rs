//! Repository for the authoritative local-DNS subsystem (issue #217).
//!
//! Owns CRUD over the three local-DNS tables seeded by the
//! `20260414000000_dns.sql` migration: `dns_zones` (authoritative zones,
//! `.lan` seeded for DHCP integration), `dns_custom_records` (user-defined
//! records, `UNIQUE(domain, record_type)`, `zone_id → dns_zones(id)
//! ON DELETE SET NULL`), and `dns_conditional_rules` (domain → upstream).
//!
//! This trait sits beside `DnsRepository` (DNS server config + query log,
//! after the Stage 7 split) and `DnsFilterRepository` (the per-device DNS
//! filtering subsystem), mirroring the latter's one-cohesive-trait shape.

use async_trait::async_trait;
use uuid::Uuid;

use wardnet_common::dns::{
    ConditionalForwardingRule, CustomDnsRecord, DnsRecordSource, DnsRecordType, DnsZone,
    DnsZoneSource,
};

/// Insert struct for an authoritative local DNS zone.
#[derive(Debug, Clone)]
pub struct ZoneRow {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub source: DnsZoneSource,
}

/// Partial update for a zone. `None` leaves a field unchanged.
#[derive(Debug, Clone, Default)]
pub struct ZoneUpdate {
    pub name: Option<String>,
    pub enabled: Option<bool>,
}

/// Insert struct for a custom local DNS record.
#[derive(Debug, Clone)]
pub struct RecordRow {
    pub id: String,
    pub zone_id: Option<Uuid>,
    pub domain: String,
    pub record_type: DnsRecordType,
    pub value: String,
    pub ttl: u32,
    pub enabled: bool,
    pub source: DnsRecordSource,
}

/// Upsert struct for a custom local DNS record keyed on
/// `(domain, record_type)`. Unlike [`RecordRow`] it carries no `id`: a fresh
/// id is generated on insert, while a conflicting row keeps its existing id.
/// Used by the DHCP `.lan` auto-registration path.
#[derive(Debug, Clone)]
pub struct UpsertRecordRow {
    pub zone_id: Option<Uuid>,
    pub value: String,
    pub ttl: u32,
    pub enabled: bool,
    pub source: DnsRecordSource,
}

/// Partial update for a custom record. `None` leaves a field unchanged;
/// `zone_id` is `Option<Option<Uuid>>` so `Some(None)` clears it to NULL
/// (mirroring `update_profile_fields`'s `description`), while `None` leaves
/// the existing zone link untouched.
#[derive(Debug, Clone, Default)]
pub struct RecordUpdate {
    pub zone_id: Option<Option<Uuid>>,
    pub domain: Option<String>,
    pub record_type: Option<DnsRecordType>,
    pub value: Option<String>,
    pub ttl: Option<u32>,
    pub enabled: Option<bool>,
}

/// Insert struct for a conditional forwarding rule.
#[derive(Debug, Clone)]
pub struct RuleRow {
    pub id: String,
    pub domain: String,
    pub upstream: String,
    pub enabled: bool,
}

/// Partial update for a conditional forwarding rule. `None` leaves a field
/// unchanged.
#[derive(Debug, Clone, Default)]
pub struct RuleUpdate {
    pub domain: Option<String>,
    pub upstream: Option<String>,
    pub enabled: Option<bool>,
}

#[async_trait]
pub trait DnsLocalRepository: Send + Sync {
    // ── Zones ───────────────────────────────────────────────────────────

    async fn list_zones(&self) -> anyhow::Result<Vec<DnsZone>>;
    async fn get_zone(&self, id: Uuid) -> anyhow::Result<Option<DnsZone>>;
    async fn create_zone(&self, row: &ZoneRow) -> anyhow::Result<DnsZone>;
    /// Update mutable fields on a zone. Returns the updated zone, or `None` if no row matched.
    async fn update_zone(&self, id: Uuid, update: &ZoneUpdate) -> anyhow::Result<Option<DnsZone>>;
    /// Delete a zone. Returns `Ok(false)` if no row matched. Records pointing
    /// at it have their `zone_id` cleared to NULL (`ON DELETE SET NULL`).
    async fn delete_zone(&self, id: Uuid) -> anyhow::Result<bool>;

    // ── Custom records ──────────────────────────────────────────────────

    async fn list_records(&self) -> anyhow::Result<Vec<CustomDnsRecord>>;
    /// Records assigned to `zone_id`. Records with a NULL `zone_id` are not
    /// returned by this query.
    async fn list_records_by_zone(&self, zone_id: Uuid) -> anyhow::Result<Vec<CustomDnsRecord>>;
    async fn get_record(&self, id: Uuid) -> anyhow::Result<Option<CustomDnsRecord>>;
    async fn create_record(&self, row: &RecordRow) -> anyhow::Result<CustomDnsRecord>;
    /// Insert-or-update a record keyed on `(domain, record_type)`.
    ///
    /// On insert a fresh id is generated. On conflict the existing row's id is
    /// preserved and `zone_id`, `value`, `ttl`, `enabled`, `source`,
    /// `updated_at` are overwritten — **but only when the existing row's
    /// `source` matches the incoming `source`**. This stops a DHCP-sourced
    /// upsert from clobbering an admin-created (`manual`) or daemon-seeded
    /// (`system`) record (a LAN client could otherwise repoint `nas.lan` at
    /// its own IP via DHCP option-12). A blocked overwrite returns `Ok(None)`;
    /// a successful insert/update returns `Ok(Some(record))`.
    async fn upsert_record(
        &self,
        domain: &str,
        record_type: DnsRecordType,
        row: &UpsertRecordRow,
    ) -> anyhow::Result<Option<CustomDnsRecord>>;
    /// Update mutable fields on a record. Returns the updated record, or `None` if no row matched.
    async fn update_record(
        &self,
        id: Uuid,
        update: &RecordUpdate,
    ) -> anyhow::Result<Option<CustomDnsRecord>>;
    async fn delete_record(&self, id: Uuid) -> anyhow::Result<bool>;

    // ── Conditional forwarding rules ────────────────────────────────────

    async fn list_rules(&self) -> anyhow::Result<Vec<ConditionalForwardingRule>>;
    async fn get_rule(&self, id: Uuid) -> anyhow::Result<Option<ConditionalForwardingRule>>;
    async fn create_rule(&self, row: &RuleRow) -> anyhow::Result<ConditionalForwardingRule>;
    /// Update mutable fields on a rule. Returns the updated rule, or `None` if no row matched.
    async fn update_rule(
        &self,
        id: Uuid,
        update: &RuleUpdate,
    ) -> anyhow::Result<Option<ConditionalForwardingRule>>;
    async fn delete_rule(&self, id: Uuid) -> anyhow::Result<bool>;
}
