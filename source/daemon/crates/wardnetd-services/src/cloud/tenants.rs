//! Client for the **tenants** service — the global identity/account authority.
//!
//! Covers the daemon-facing slice of the tenants API: the bootstrap pair
//! (request an emailed enrollment code, then enroll a key), token minting, and
//! the network plane (slug availability, network registration, per-daemon
//! removal). Account/SPA/OAuth endpoints are not the daemon's concern.
//!
//! tenants is **global** (one deployment) behind the global north-south
//! **gateway** (cloud ADR-0014 / inforge ADR-0032), so its base URL is a single
//! constant, unlike the per-region [`DdnsClient`](super::ddns::DdnsClient). The
//! gateway routes by the first path segment, so every path here carries the
//! `/tenants/` prefix — and, because the gateway is path-preserving, that full
//! prefixed path is exactly what the cloud verifies the `PoP` signature against.

use serde::{Deserialize, Serialize};

use super::CloudError;
use super::identity::DaemonIdentity;
use super::request::{self, Auth};

/// Gateway path-routing prefix: the first path segment selecting the tenants
/// service. Prepended once, in [`TenantsClient::send`], so every call is
/// prefixed (and therefore `PoP`-signed) structurally rather than per literal.
const SERVICE_PREFIX: &str = "/tenants";

/// Code-request endpoint: the converged `POST /v1/verification-codes` resource
/// from wardnet-cloud #20 (supersedes `POST /v1/enrollment-codes`). Kept as a
/// single constant so a path change stays one line.
const ENROLLMENT_CODE_PATH: &str = "/v1/verification-codes";
/// The `purpose` discriminator bound into the requested code (prevents a code
/// issued for enrollment being spent on signup/reset).
const ENROLLMENT_CODE_PURPOSE: &str = "enrollment";

/// The outcome of a successful network registration, surfaced to the wizard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkRegistration {
    /// Server-assigned network UUID — the `net` the daemon's network-scoped JWT
    /// is bound to, and the id used for per-daemon removal.
    pub network_id: String,
    /// The vanity slug, forming `<slug>.my.wardnet.services`.
    pub slug: String,
    /// The region that owns this network's DNS/tunnel.
    pub region: String,
    /// `provisioning` | `active` | `deprovisioning` at registration time.
    pub provisioning_state: String,
}

/// A client for the tenants service at `base_url`.
pub struct TenantsClient {
    http: reqwest::Client,
    base_url: String,
}

impl TenantsClient {
    /// Build a client sharing the pooled `http` and pointed at `base_url`
    /// (production: the global gateway `api.wardnet.network`; tests: a wiremock
    /// URL).
    #[must_use]
    pub fn new(http: reqwest::Client, base_url: String) -> Self {
        Self { http, base_url }
    }

    /// Send `{SERVICE_PREFIX}{path_and_query}` — the single funnel every
    /// tenants call goes through, so a new endpoint cannot forget the gateway
    /// prefix (a valid signature over an un-prefixed path would misroute at
    /// the gateway). The prefixed string is built once and passed whole to
    /// [`request::send`], preserving its "sign exactly what you send"
    /// invariant.
    async fn send(
        &self,
        auth: Auth<'_>,
        method: reqwest::Method,
        path_and_query: &str,
        body: Option<Vec<u8>>,
    ) -> Result<reqwest::Response, CloudError> {
        request::send(
            &self.http,
            &self.base_url,
            auth,
            method,
            &format!("{SERVICE_PREFIX}{path_and_query}"),
            body,
        )
        .await
    }

    // ── bootstrap plane (no JWT) ────────────────────────────────────────────

    /// Request a one-time enrollment code be emailed to `email`. Public,
    /// per-IP-rate-limited on the cloud side.
    pub async fn request_enrollment_code(&self, email: &str) -> Result<(), CloudError> {
        let body = serde_json::to_vec(&CodeRequest {
            email,
            purpose: ENROLLMENT_CODE_PURPOSE,
        })
        .map_err(|e| CloudError::Upstream(e.into()))?;
        let resp = self
            .send(
                Auth::Public,
                reqwest::Method::POST,
                ENROLLMENT_CODE_PATH,
                Some(body),
            )
            .await?;
        request::ok(resp).await.map(drop)
    }

    /// Enroll the daemon's public key against `code`, binding the key to a
    /// tenant. Returns the bound `tenant_id`. Auth is the code itself.
    pub async fn enroll(&self, code: &str, public_key_b64: &str) -> Result<String, CloudError> {
        let body = serde_json::to_vec(&EnrollRequest {
            code,
            public_key: public_key_b64,
        })
        .map_err(|e| CloudError::Upstream(e.into()))?;
        let resp = self
            .send(
                Auth::Public,
                reqwest::Method::POST,
                "/v1/enroll",
                Some(body),
            )
            .await?;
        let parsed: EnrollResponse = request::json(request::ok(resp).await?).await?;
        Ok(parsed.tenant_id)
    }

    /// Mint an identity JWT for `identity`, authenticated by `PoP` over the request
    /// (no bearer — this endpoint *issues* the bearer). The token is tenant- or
    /// network-scoped per the key's current binding. A `403` means the
    /// subscription is not active: the identity is flagged unentitled and
    /// [`CloudError::EntitlementLost`] is returned.
    ///
    /// Called by [`DaemonIdentity::token`]; not for direct use.
    pub(crate) async fn mint_token(&self, identity: &DaemonIdentity) -> Result<String, CloudError> {
        let body = serde_json::to_vec(&TokenRequest {
            public_key: identity.public_key_b64(),
        })
        .map_err(|e| CloudError::Upstream(e.into()))?;
        let resp = self
            .send(
                Auth::Pop(identity),
                reqwest::Method::POST,
                "/v1/token",
                Some(body),
            )
            .await?;
        if request::is(&resp, reqwest::StatusCode::FORBIDDEN) {
            identity.mark_unentitled();
            return Err(CloudError::EntitlementLost);
        }
        // A non-`403` success means the subscription is active, so mark the box
        // entitled *before* parsing the body: a malformed (e.g. truncated) `200`
        // must still clear a suspended flag, otherwise the box can never
        // self-heal (every re-probe would hit the same parse error).
        let ok = request::ok(resp).await?;
        identity.mark_entitled();
        let parsed: TokenResponse = request::json(ok).await?;
        Ok(parsed.token)
    }

    // ── network plane (JWT + PoP) ───────────────────────────────────────────

    /// Whether `slug` is well-formed, unreserved, and free.
    pub async fn availability(
        &self,
        identity: &DaemonIdentity,
        slug: &str,
    ) -> Result<bool, CloudError> {
        let resp = self
            .send(
                Auth::Full(identity),
                reqwest::Method::GET,
                &format!("/v1/availability?slug={slug}"),
                None,
            )
            .await?;
        let parsed: AvailabilityResponse = request::json(request::ok(resp).await?).await?;
        Ok(parsed.available)
    }

    /// Register a network under `slug` in `region`. The returned
    /// [`NetworkRegistration`] carries the `network_id` the daemon then mints a
    /// network-scoped token against.
    pub async fn register_network(
        &self,
        identity: &DaemonIdentity,
        slug: &str,
        display_name: Option<&str>,
        region: &str,
    ) -> Result<NetworkRegistration, CloudError> {
        let body = serde_json::to_vec(&RegisterNetworkRequest {
            slug,
            display_name,
            region,
        })
        .map_err(|e| CloudError::Upstream(e.into()))?;
        let resp = self
            .send(
                Auth::Full(identity),
                reqwest::Method::POST,
                "/v1/networks",
                Some(body),
            )
            .await?;
        let view: NetworkView = request::json(request::ok(resp).await?).await?;
        Ok(NetworkRegistration {
            network_id: view.id,
            slug: view.slug,
            region: view.region,
            provisioning_state: view.provisioning_state,
        })
    }

    /// Remove **this** daemon from `network_id`, leaving the network and its
    /// peers intact. Idempotent. Consumes the per-daemon removal endpoint
    /// defined by wardnet-cloud issue (built here ahead of that endpoint
    /// landing); a `404`/missing endpoint surfaces as [`CloudError::BadRequest`]
    /// and is swallowed best-effort by the caller.
    pub async fn remove_daemon(
        &self,
        identity: &DaemonIdentity,
        network_id: &str,
    ) -> Result<(), CloudError> {
        let resp = self
            .send(
                Auth::Full(identity),
                reqwest::Method::DELETE,
                &format!("/v1/networks/{network_id}/daemons/self"),
                None,
            )
            .await?;
        request::ok(resp).await.map(drop)
    }
}

// ── wire types (mirror the tenants contract) ────────────────────────────────────

#[derive(Serialize)]
struct CodeRequest<'a> {
    email: &'a str,
    purpose: &'a str,
}

#[derive(Serialize)]
struct EnrollRequest<'a> {
    code: &'a str,
    public_key: &'a str,
}

#[derive(Deserialize)]
struct EnrollResponse {
    tenant_id: String,
}

#[derive(Serialize)]
struct TokenRequest<'a> {
    public_key: &'a str,
}

#[derive(Deserialize)]
struct TokenResponse {
    token: String,
}

#[derive(Deserialize)]
struct AvailabilityResponse {
    available: bool,
}

#[derive(Serialize)]
struct RegisterNetworkRequest<'a> {
    slug: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    display_name: Option<&'a str>,
    region: &'a str,
}

#[derive(Deserialize)]
struct NetworkView {
    id: String,
    slug: String,
    region: String,
    provisioning_state: String,
}
