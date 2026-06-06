//! The `DnsProvider` abstraction — the publish side of dynamic DNS.
//!
//! A provider is **bound to its target at construction**: the wardnet
//! [`BridgeProvider`](super::bridge::BridgeProvider) carries the install id +
//! signing material + bridge endpoint, and the
//! [`CloudflareProvider`](super::cloudflare::CloudflareProvider) carries the
//! record name + zone + token. The trait methods therefore only take the
//! *dynamic* payload — the IP to publish or the challenge value to set — never
//! the record name. This keeps both impls honest (neither has to ignore an
//! argument it can't use) and makes provider switching a matter of rebuilding
//! the bound provider from current config.
//!
//! Only "publish A record" and "publish / clear `_acme-challenge` TXT" are
//! provider-specific. Steady-state DDNS (C6) exercises [`DnsProvider::upsert_a`];
//! [`DnsProvider::set_txt`] / [`DnsProvider::delete_txt`] exist for the ACME
//! DNS-01 flow wired in a later commit.

use std::net::Ipv4Addr;

use async_trait::async_trait;

/// Publishes DNS records for a single wardnet installation through one backend.
///
/// Implementations are bound to a concrete target (install or domain) when
/// constructed; see the [module docs](self). Every method performs a network
/// call to the backend and returns the backend's error verbatim on failure.
#[async_trait]
pub trait DnsProvider: Send + Sync {
    /// Create or update the installation's public **A** record so it points at
    /// `ip`. Idempotent: calling it with an unchanged IP is a no-op upsert.
    async fn upsert_a(&self, ip: Ipv4Addr) -> anyhow::Result<()>;

    /// Create or update the `_acme-challenge` **TXT** record for the
    /// installation with `value` (a raw DNS-01 key authorization digest).
    async fn set_txt(&self, value: &str) -> anyhow::Result<()>;

    /// Remove the `_acme-challenge` TXT record. Idempotent — succeeds even if
    /// no record is currently set.
    async fn delete_txt(&self) -> anyhow::Result<()>;

    /// Remove this installation's published presence from the backend — the
    /// "forget my domain" half of a teardown. The **bridge** provider calls
    /// `DELETE /v1/installs/{id}`, which drops the upstream A + ACME TXT records
    /// and the install row in one shot; the **Cloudflare** provider deletes its
    /// A record (and any lingering `_acme-challenge` TXT). Best-effort by
    /// contract: the caller ([`DdnsService::teardown`](super::DdnsService)) wipes
    /// local config + secrets regardless, so a dead-backend error here must not
    /// trap the operator in a configured state — it is logged and swallowed.
    /// Idempotent: an already-absent record/install is success.
    async fn teardown(&self) -> anyhow::Result<()>;
}
