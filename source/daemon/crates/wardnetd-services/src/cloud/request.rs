//! Shared HTTP plumbing for the cloud clients: building (optionally
//! PoP-signed) requests, classifying responses into [`CloudError`], and
//! decoding JSON bodies.
//!
//! The same bytes are signed and sent — a body is serialized once and that
//! exact slice both feeds the `PoP` body-hash and goes on the wire, so the
//! cloud's `hex-sha256(body)` always matches.

use chrono::Utc;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use reqwest::{Method, Response, StatusCode};

use super::identity::DaemonIdentity;
use super::{CloudError, pop};

/// How a request is authenticated.
#[derive(Clone, Copy)]
pub(crate) enum Auth<'a> {
    /// No credentials (bootstrap endpoints: code request, enroll).
    Public,
    /// `PoP` signature only, no bearer — the token-mint endpoint, which *issues*
    /// the JWT and so cannot present one.
    Pop(&'a DaemonIdentity),
    /// Bearer JWT **and** `PoP` signature — every steady-state authenticated call.
    Full(&'a DaemonIdentity),
}

/// Send a request to `{base_url}{path_and_query}`, attaching auth per [`Auth`].
///
/// `path_and_query` is used **verbatim** both for the URL and for the signed
/// payload, so callers must pre-encode any query string (our slugs/ids are
/// already `[a-z0-9-]`). Returns the raw response; use [`ok`] to classify status.
pub(crate) async fn send(
    http: &reqwest::Client,
    base_url: &str,
    auth: Auth<'_>,
    method: Method,
    path_and_query: &str,
    body: Option<Vec<u8>>,
) -> Result<Response, CloudError> {
    let body_bytes = body.unwrap_or_default();
    let has_body = !body_bytes.is_empty();
    let mut req = http.request(method.clone(), format!("{base_url}{path_and_query}"));

    let signer = match auth {
        Auth::Public => None,
        Auth::Pop(id) => Some((id, false)),
        Auth::Full(id) => Some((id, true)),
    };
    if let Some((id, bearer)) = signer {
        if bearer {
            // May fail with `EntitlementLost` when the subscription has lapsed.
            let token = id.token().await?;
            req = req.header(AUTHORIZATION, format!("Bearer {token}"));
        }
        let timestamp = Utc::now().timestamp();
        let signature = pop::sign(
            id.signing_key(),
            method.as_str(),
            path_and_query,
            timestamp,
            &body_bytes,
        );
        req = req
            .header(pop::TIMESTAMP_HEADER, timestamp.to_string())
            .header(pop::SIGNATURE_HEADER, signature);
    }

    if has_body {
        req = req
            .header(CONTENT_TYPE, "application/json")
            .body(body_bytes);
    }

    req.send().await.map_err(|e| CloudError::Upstream(e.into()))
}

/// Classify a response: success passes through; a 4xx is a caller-fixable
/// [`CloudError::BadRequest`] (carrying the body detail); anything else is
/// [`CloudError::Upstream`]. The token-mint `403`→`EntitlementLost` case is
/// handled by the caller *before* this, since only there is a 403 about
/// entitlement.
pub(crate) async fn ok(response: Response) -> Result<Response, CloudError> {
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }
    let detail = response.text().await.unwrap_or_default();
    Err(if status.is_client_error() {
        CloudError::BadRequest(format!("HTTP {status}: {detail}"))
    } else {
        CloudError::Upstream(anyhow::anyhow!("HTTP {status}: {detail}"))
    })
}

/// Whether a response carries the given status.
pub(crate) fn is(response: &Response, status: StatusCode) -> bool {
    response.status() == status
}

/// Decode a JSON response body, mapping a parse failure to [`CloudError::Upstream`].
pub(crate) async fn json<T: serde::de::DeserializeOwned>(
    response: Response,
) -> Result<T, CloudError> {
    let status = response.status();
    response
        .json()
        .await
        .map_err(|e| CloudError::Upstream(anyhow::anyhow!("non-JSON body (HTTP {status}): {e}")))
}
