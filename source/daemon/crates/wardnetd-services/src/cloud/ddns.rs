//! Client for the **ddns** service — the regional report-IP + ACME DNS-01 proxy
//! — and the [`WardnetDnsProvider`] that adapts it to the [`DnsProvider`] trait.
//!
//! ddns is **regional**: its base URL comes from the region catalog
//! (`https://ddns.svc.<region>…` — the daemon dials the FQDN directly, so TLS SNI
//! is the hostname automatically). Every call is network-scoped JWT + `PoP`; the
//! network is taken from the JWT `net` claim, so paths carry no id.
//!
//! [`WardnetDnsProvider`] is the wardnet-managed sibling of the BYOD
//! [`CloudflareProvider`](crate::ddns::cloudflare::CloudflareProvider): publish A
//! / `_acme-challenge` TXT through ddns, and on teardown remove this daemon from
//! its network via [`TenantsClient`] (the wardnet plane has no DNS for the daemon
//! to delete — the cloud reconciler owns records).

use std::net::Ipv4Addr;
use std::sync::Arc;

use async_trait::async_trait;
use serde::Serialize;

use super::CloudError;
use super::identity::DaemonIdentity;
use super::request::{self, Auth};
use super::tenants::TenantsClient;
use crate::ddns::provider::DnsProvider;

/// A client for a region's ddns service at `base_url`.
pub struct DdnsClient {
    http: reqwest::Client,
    base_url: String,
}

impl DdnsClient {
    /// Build a client sharing the pooled `http` and pointed at the region's ddns
    /// `base_url` (e.g. `https://ddns.svc.prd.use1.wardnet.network`).
    #[must_use]
    pub fn new(http: reqwest::Client, base_url: String) -> Self {
        Self { http, base_url }
    }

    /// Publish the network's public **A** record (`PUT /v1/ip`).
    pub async fn report_ip(
        &self,
        identity: &DaemonIdentity,
        ip: Ipv4Addr,
    ) -> Result<(), CloudError> {
        let body = serde_json::to_vec(&ReportIpRequest { ip: ip.to_string() })
            .map_err(|e| CloudError::Upstream(e.into()))?;
        let resp = self.put("/v1/ip", identity, Some(body)).await?;
        request::ok(resp).await.map(drop)
    }

    /// Publish the `_acme-challenge` TXT value(s) (`PUT /v1/acme-challenge`).
    pub async fn set_acme_challenge(
        &self,
        identity: &DaemonIdentity,
        values: &[String],
    ) -> Result<(), CloudError> {
        let body = serde_json::to_vec(&SetAcmeChallengeRequest { values })
            .map_err(|e| CloudError::Upstream(e.into()))?;
        let resp = self.put("/v1/acme-challenge", identity, Some(body)).await?;
        request::ok(resp).await.map(drop)
    }

    /// Remove the `_acme-challenge` TXT records (`DELETE /v1/acme-challenge`).
    /// Idempotent.
    pub async fn clear_acme_challenge(&self, identity: &DaemonIdentity) -> Result<(), CloudError> {
        let resp = request::send(
            &self.http,
            &self.base_url,
            Auth::Full(identity),
            reqwest::Method::DELETE,
            "/v1/acme-challenge",
            None,
        )
        .await?;
        request::ok(resp).await.map(drop)
    }

    async fn put(
        &self,
        path: &str,
        identity: &DaemonIdentity,
        body: Option<Vec<u8>>,
    ) -> Result<reqwest::Response, CloudError> {
        request::send(
            &self.http,
            &self.base_url,
            Auth::Full(identity),
            reqwest::Method::PUT,
            path,
            body,
        )
        .await
    }
}

/// The wardnet-managed [`DnsProvider`]: publishes through [`DdnsClient`] and
/// tears down via per-daemon removal on [`TenantsClient`]. Bound at construction
/// to one network + identity.
pub struct WardnetDnsProvider {
    ddns: DdnsClient,
    tenants: Arc<TenantsClient>,
    identity: Arc<DaemonIdentity>,
    network_id: String,
}

impl WardnetDnsProvider {
    /// Build a provider bound to `network_id`, sharing the `identity` (key + JWT
    /// cache) and the global `tenants` client.
    #[must_use]
    pub fn new(
        ddns: DdnsClient,
        tenants: Arc<TenantsClient>,
        identity: Arc<DaemonIdentity>,
        network_id: String,
    ) -> Self {
        Self {
            ddns,
            tenants,
            identity,
            network_id,
        }
    }
}

#[async_trait]
impl DnsProvider for WardnetDnsProvider {
    async fn upsert_a(&self, ip: Ipv4Addr) -> anyhow::Result<()> {
        Ok(self.ddns.report_ip(&self.identity, ip).await?)
    }

    async fn set_txt(&self, values: &[String]) -> anyhow::Result<()> {
        Ok(self.ddns.set_acme_challenge(&self.identity, values).await?)
    }

    async fn delete_txt(&self) -> anyhow::Result<()> {
        Ok(self.ddns.clear_acme_challenge(&self.identity).await?)
    }

    async fn teardown(&self) -> anyhow::Result<()> {
        // The wardnet plane owns no daemon-deletable DNS; "forget my presence"
        // is removing this daemon from its network. Best-effort by the trait
        // contract — the caller wipes local state regardless.
        Ok(self
            .tenants
            .remove_daemon(&self.identity, &self.network_id)
            .await?)
    }
}

#[derive(Serialize)]
struct ReportIpRequest {
    ip: String,
}

#[derive(Serialize)]
struct SetAcmeChallengeRequest<'a> {
    values: &'a [String],
}
