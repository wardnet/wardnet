//! The **Bring-Your-Own-Domain** Cloudflare DDNS provider.
//!
//! The daemon talks to the *user's own* Cloudflare account directly (no bridge)
//! to manage records for a domain they control. The provider is bound at
//! construction to `{record_name, zone_id, api_token}`; the API token must be
//! scoped to **DNS:Edit** on that single zone.
//!
//! Unlike the bridge — which keys records by install id and tracks the
//! Cloudflare record id server-side — the daemon stores no record id, so each
//! mutation **lists by name+type first** and then creates or updates. This keeps
//! the provider stateless beyond its bound identity.
//!
//! The HTTP client is **injected** so every provider shares one connection pool
//! and reuses TLS sessions; the `Authorization: Bearer` header is attached
//! per-request rather than baked into a per-provider client.

use std::net::Ipv4Addr;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use super::provider::DnsProvider;

/// Cloudflare v4 REST API base URL.
pub(crate) const CF_API_BASE: &str = "https://api.cloudflare.com/client/v4";
/// TTL for the published A record (120 s is the Cloudflare free-tier minimum).
const A_RECORD_TTL: u32 = 120;
/// Short TTL for ACME TXT records so stale challenges expire quickly.
const TXT_RECORD_TTL: u32 = 60;

/// A Cloudflare provider bound to one DNS record in one zone.
///
/// Construct with [`CloudflareProvider::new_with_base_url`], passing the shared
/// [`reqwest::Client`]. The token is sent per-request via `Bearer` auth.
pub struct CloudflareProvider {
    http: reqwest::Client,
    base_url: String,
    api_token: String,
    zone_id: String,
    /// The user's fully-qualified domain, e.g. `home.example.com`. The ACME
    /// challenge record is `_acme-challenge.<record_name>`.
    record_name: String,
}

impl CloudflareProvider {
    /// Build a provider for `record_name` in `zone_id`, authenticated with
    /// `api_token`, sharing `http` and pointed at `base_url`. Production passes
    /// [`CF_API_BASE`]; tests point this at a wiremock server.
    #[must_use]
    pub fn new_with_base_url(
        http: reqwest::Client,
        api_token: &str,
        zone_id: &str,
        record_name: &str,
        base_url: &str,
    ) -> Self {
        Self {
            http,
            base_url: base_url.to_owned(),
            api_token: api_token.to_owned(),
            zone_id: zone_id.to_owned(),
            record_name: record_name.to_owned(),
        }
    }

    fn records_url(&self) -> String {
        format!("{}/zones/{}/dns_records", self.base_url, self.zone_id)
    }

    fn record_url(&self, record_id: &str) -> String {
        format!(
            "{}/zones/{}/dns_records/{record_id}",
            self.base_url, self.zone_id
        )
    }

    /// Return the existing record id for `(name, record_type)`, if any.
    async fn find_record_id(
        &self,
        record_type: &str,
        name: &str,
    ) -> anyhow::Result<Option<String>> {
        let response = self
            .http
            .get(self.records_url())
            .bearer_auth(&self.api_token)
            .query(&[("type", record_type), ("name", name)])
            .send()
            .await?;
        let parsed: CfResponse<Vec<DnsRecordResult>> = parse_json(response).await?;
        Ok(parsed.result.into_iter().flatten().next().map(|r| r.id))
    }

    /// Create or update `(name, record_type)` so its content is `content`.
    async fn upsert(
        &self,
        record_type: &str,
        name: &str,
        content: &str,
        ttl: u32,
    ) -> anyhow::Result<()> {
        let body = DnsRecordBody {
            r#type: record_type,
            name,
            content,
            ttl,
            proxied: false,
        };
        let response = match self.find_record_id(record_type, name).await? {
            Some(id) => {
                self.http
                    .put(self.record_url(&id))
                    .bearer_auth(&self.api_token)
                    .json(&body)
                    .send()
                    .await?
            }
            None => {
                self.http
                    .post(self.records_url())
                    .bearer_auth(&self.api_token)
                    .json(&body)
                    .send()
                    .await?
            }
        };
        let _: CfResponse<DnsRecordResult> = parse_json(response).await?;
        Ok(())
    }

    fn acme_name(&self) -> String {
        format!("_acme-challenge.{}", self.record_name)
    }
}

#[async_trait]
impl DnsProvider for CloudflareProvider {
    async fn upsert_a(&self, ip: Ipv4Addr) -> anyhow::Result<()> {
        self.upsert("A", &self.record_name, &ip.to_string(), A_RECORD_TTL)
            .await
    }

    async fn set_txt(&self, value: &str) -> anyhow::Result<()> {
        self.upsert("TXT", &self.acme_name(), value, TXT_RECORD_TTL)
            .await
    }

    async fn delete_txt(&self) -> anyhow::Result<()> {
        // ACME publishes a single `_acme-challenge` TXT, so deleting the one
        // matching record is sufficient. Idempotent: absence is success.
        let name = self.acme_name();
        if let Some(id) = self.find_record_id("TXT", &name).await? {
            let response = self
                .http
                .delete(self.record_url(&id))
                .bearer_auth(&self.api_token)
                .send()
                .await?;
            let _: CfResponse<DnsRecordResult> = parse_json(response).await?;
        }
        Ok(())
    }
}

// ── Cloudflare API types ───────────────────────────────────────────────────────

#[derive(Serialize)]
struct DnsRecordBody<'a> {
    r#type: &'a str,
    name: &'a str,
    content: &'a str,
    ttl: u32,
    proxied: bool,
}

#[derive(Deserialize)]
struct DnsRecordResult {
    id: String,
}

#[derive(Deserialize)]
struct CfError {
    message: String,
}

#[derive(Deserialize)]
struct CfResponse<T> {
    success: bool,
    #[serde(default)]
    errors: Vec<CfError>,
    result: Option<T>,
}

/// Parse a Cloudflare envelope, surfacing `success: false` as an error.
async fn parse_json<T: serde::de::DeserializeOwned>(
    response: reqwest::Response,
) -> anyhow::Result<CfResponse<T>> {
    let status = response.status();
    let parsed: CfResponse<T> = response
        .json()
        .await
        .map_err(|e| anyhow::anyhow!("Cloudflare returned non-JSON body (HTTP {status}): {e}"))?;
    if !parsed.success {
        let messages: Vec<_> = parsed.errors.iter().map(|e| e.message.as_str()).collect();
        anyhow::bail!(
            "Cloudflare API error (HTTP {status}): {}",
            messages.join("; ")
        );
    }
    Ok(parsed)
}
