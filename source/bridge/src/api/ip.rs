use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use chrono::Utc;
use serde::Deserialize;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::auth::AuthenticatedInstall;
use crate::error::ApiError;
use crate::state::AppState;

/// Register the IP-update route.
pub fn register(router: OpenApiRouter<AppState>) -> OpenApiRouter<AppState> {
    router.routes(routes!(update_ip))
}

/// Request body for `PUT /v1/installs/{id}/ip`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct UpdateIpRequest {
    /// Current public IPv4 address of the Pi, e.g. `"203.0.113.42"`.
    pub ip: String,
}

#[utoipa::path(
    put,
    path = "/v1/installs/{id}/ip",
    tag = "installs",
    description = "Update the public IP address of a registered installation. \
                   The bridge creates or updates the `<name>.my.<region>.wardnet.network` \
                   A record in Cloudflare. \
                   \n\n\
                   Requires `Authorization: Bearer`, `X-Wardnet-Timestamp`, and \
                   `X-Wardnet-Signature` headers. The install ID in the path must \
                   match the install identified by the bearer token.",
    params(
        ("id" = String, Path, description = "Installation UUID"),
    ),
    request_body = UpdateIpRequest,
    responses(
        (status = 204, description = "IP updated"),
        (status = 400, description = "Invalid IP address"),
        (status = 401, description = "Authentication required or invalid"),
        (status = 403, description = "Bearer token does not own this install ID"),
        (status = 404, description = "Install not found"),
        (status = 500, description = "Internal server error"),
    ),
)]
pub async fn update_ip(
    State(state): State<AppState>,
    Path(id): Path<String>,
    AuthenticatedInstall(install): AuthenticatedInstall,
    Json(body): Json<UpdateIpRequest>,
) -> Result<StatusCode, ApiError> {
    // Authorisation: the bearer token must own this install ID.
    if install.id != id {
        return Err(ApiError::Forbidden(
            "bearer token does not match the requested install ID".to_string(),
        ));
    }

    // Validate IPv4 and reject private/reserved ranges.
    //
    // Accepting a private address would let an attacker point the DNS A record
    // at an RFC 1918 / loopback address, enabling SSRF-style attacks against
    // the Cloudflare resolver or any service that resolves the FQDN.
    let addr_v4: std::net::Ipv4Addr = body
        .ip
        .parse()
        .map_err(|_| ApiError::BadRequest(format!("'{}' is not a valid IPv4 address", body.ip)))?;

    if is_reserved_ipv4(addr_v4) {
        return Err(ApiError::BadRequest(format!(
            "'{}' is a private or reserved address and cannot be used as a public IP",
            body.ip,
        )));
    }

    let fqdn = state.config().install_fqdn(&install.name);

    // Upsert the A record — create on first call, update on subsequent calls.
    let record_id = state
        .dns()
        .upsert_a_record(&fqdn, &body.ip, install.cf_a_record_id.as_deref())
        .await
        .map_err(|e| {
            tracing::error!(install_id = %id, error = %e, "Cloudflare A-record upsert failed");
            ApiError::Internal(e)
        })?;

    state
        .installs()
        .update_ip(&id, &body.ip, &record_id, &Utc::now().to_rfc3339())
        .await
        .map_err(ApiError::Internal)?;

    tracing::info!(install_id = %id, ip = %body.ip, fqdn = %fqdn, "IP record updated");
    Ok(StatusCode::NO_CONTENT)
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Returns `true` when `addr` falls within a private, loopback, link-local,
/// broadcast, documentation, or otherwise non-routable range.
///
/// An installation's public IP must be a globally routable unicast address.
/// Accepting a private address would allow pointing a wardnet subdomain at an
/// RFC 1918 address, enabling SSRF through any downstream service that
/// resolves that name.
fn is_reserved_ipv4(addr: std::net::Ipv4Addr) -> bool {
    addr.is_private()           // 10/8, 172.16/12, 192.168/16
        || addr.is_loopback()   // 127/8
        || addr.is_link_local() // 169.254/16
        || addr.is_broadcast()  // 255.255.255.255
        || addr.is_documentation() // 192.0.2/24, 198.51.100/24, 203.0.113/24
        || addr.is_unspecified() // 0.0.0.0
        || {
            // Shared address space (RFC 6598): 100.64.0.0/10
            let octets = addr.octets();
            octets[0] == 100 && (octets[1] & 0b1100_0000) == 64
        }
}

#[cfg(test)]
mod tests;
