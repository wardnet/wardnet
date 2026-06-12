use async_trait::async_trait;

/// Abstraction over a DNS hosting provider.
///
/// The bridge constructs fully-qualified domain names from its configuration
/// (e.g. `happy-einstein.my.wardnet.services`) and passes them here.
/// Implementations are provider-specific (Cloudflare, Route 53, Hetzner DNS,
/// …) and live under `src/cloudflare/`, `src/route53/`, etc.
///
/// Every mutating operation returns a provider-opaque **record ID** that the
/// bridge stores in the database. The ID is passed back on subsequent upserts
/// so the implementation can choose `PUT` (update) vs `POST` (create) as
/// appropriate, and on deletes so it can construct the right endpoint.
///
/// # Invariants
///
/// - Implementations must be `Send + Sync` (called from async contexts).
/// - `upsert_*` must be idempotent: calling twice with the same arguments
///   must leave exactly one DNS record and return a valid record ID.
/// - `delete_record` must be idempotent: deleting a non-existent ID must
///   succeed (or return a benign error the caller can ignore).
#[async_trait]
pub trait DnsProvider: Send + Sync {
    /// Create or update an A record.
    ///
    /// - `fqdn` — fully-qualified DNS name, e.g. `"happy-einstein.my.wardnet.services"`.
    /// - `ip` — IPv4 address string, e.g. `"203.0.113.42"`.
    /// - `existing_record_id` — pass the previously-returned record ID to
    ///   update an existing record; pass `None` to create a new one.
    ///
    /// Returns the provider-assigned record ID.
    async fn upsert_a_record(
        &self,
        fqdn: &str,
        ip: &str,
        existing_record_id: Option<&str>,
    ) -> anyhow::Result<String>;

    /// Create or update a TXT record.
    ///
    /// Used for ACME DNS-01 challenge values.
    ///
    /// - `fqdn` — fully-qualified DNS name, e.g.
    ///   `"_acme-challenge.happy-einstein.my.wardnet.services"`.
    /// - `content` — the raw TXT record value (no quoting needed; the
    ///   implementation adds any provider-required quoting).
    /// - `existing_record_id` — as in [`upsert_a_record`].
    ///
    /// Returns the provider-assigned record ID.
    async fn upsert_txt_record(
        &self,
        fqdn: &str,
        content: &str,
        existing_record_id: Option<&str>,
    ) -> anyhow::Result<String>;

    /// Delete a DNS record by its provider-assigned ID.
    ///
    /// The caller is responsible for passing the ID that was returned by a
    /// previous `upsert_*` call and stored in the database.
    async fn delete_record(&self, record_id: &str) -> anyhow::Result<()>;
}
