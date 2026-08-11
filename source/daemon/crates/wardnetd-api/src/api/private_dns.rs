use axum::Json;
use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{StatusCode, header};
use axum::response::IntoResponse;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;
use wardnet_common::api::{
    ApiError, CreatePrivateDnsGrantRequest, PrivateDnsGrantSummary, PrivateDnsMeResponse,
    PrivateDnsPrerequisites, PrivateDnsStatusResponse, SendPrivateDnsNotificationResponse,
    SetPrivateDnsEnabledRequest,
};
use wardnet_common::auth::AuthContext;

use crate::api::middleware::{ClientIp, SessionAuth};
use crate::api::responses::{AuthErrors, Conflict, NotFound};
use crate::state::AppState;
use wardnetd_services::auth_context;
use wardnetd_services::ddns::DdnsStatus;
use wardnetd_services::error::AppError;
use wardnetd_services::private_dns::cert_ready;
use wardnetd_services::private_dns::profile::MOBILECONFIG_CONTENT_TYPE;

/// Register Private DNS routes onto the given [`OpenApiRouter`].
pub fn register(router: OpenApiRouter<AppState>) -> OpenApiRouter<AppState> {
    router
        .routes(routes!(get_status))
        .routes(routes!(set_enabled))
        .routes(routes!(create_grant))
        .routes(routes!(delete_grant))
        .routes(routes!(notify_device))
        .routes(routes!(get_me))
        .routes(routes!(get_me_profile))
}

const TAG: &str = "private-dns";

/// An internal admin context for the device-keyed endpoints: their handler has
/// already authenticated the caller by source IP, then reaches the admin-gated
/// Private DNS service to read that one device's state (mirrors how
/// `GET /api/devices/me` enriches itself from admin-only services).
fn system_admin() -> AuthContext {
    AuthContext::system()
}

/// `<token>.<domain>`, or the bare token when the domain isn't known (a grant
/// only exists while the feature is enabled, which requires the domain, so the
/// fallback is defensive).
fn hostname(token: &str, domain: Option<&str>) -> String {
    match domain {
        Some(domain) => format!("{token}.{domain}"),
        None => token.to_owned(),
    }
}

/// The wardnet domain grants live under, derived from a DDNS status: the FQDN
/// when on the wardnet provider, else `None`. Kept here so the API layer reads
/// DDNS once and derives both the domain and the `wardnet_provider` flag from
/// the same snapshot (the service's `status()` would round-trip DDNS again).
fn wardnet_domain(ddns: &DdnsStatus) -> Option<String> {
    if ddns.provider.as_deref() == Some("wardnet") {
        ddns.fqdn.clone()
    } else {
        None
    }
}

/// Assemble the admin status view: enabled state, enable prerequisites (pulled
/// from DDNS / TLS / entitlement), and every grant with its full hostname.
async fn build_status(state: &AppState) -> Result<PrivateDnsStatusResponse, AppError> {
    // One DDNS read feeds both `domain` and `wardnet_provider`; `is_enabled()`
    // is a bare config read, so a transient DDNS hiccup can't flap the flag.
    let enabled = state.private_dns_service().is_enabled().await?;
    let ddns = state.ddns_service().status().await?;

    let domain = wardnet_domain(&ddns);
    let wardnet_provider = domain.is_some();
    let certificate = cert_ready(&state.tls_service().status().await?);
    let entitled = state.is_entitled();
    // Informational readiness signal for roaming: on the wardnet provider, not
    // suspended, and publishing a public IP (so the wildcard CNAME resolves to
    // the regional edge). Not a live probe of the reverse tunnel, and never a
    // gate — the LAN path works regardless.
    let relay_connected = wardnet_provider && !ddns.suspended && ddns.last_public_ip.is_some();

    let grants = state
        .private_dns_service()
        .list_grants()
        .await?
        .into_iter()
        .map(|g| PrivateDnsGrantSummary {
            id: g.id,
            device_id: g.device_id,
            hostname: hostname(&g.token, domain.as_deref()),
            created_at: g.created_at,
        })
        .collect();

    Ok(PrivateDnsStatusResponse {
        enabled,
        domain,
        prerequisites: PrivateDnsPrerequisites {
            wardnet_provider,
            certificate,
            entitled,
            relay_connected,
        },
        grants,
    })
}

#[utoipa::path(
    get,
    path = "/api/private-dns/status",
    tag = TAG,
    description = "Report Private DNS state for the admin UI: whether it is enabled, the \
                   wardnet domain grants live under, the enable prerequisites \
                   (wardnet provider, issued certificate, active entitlement, plus the \
                   informational reverse-tunnel readiness), and every per-device grant with \
                   its full minted hostname. Admin only.",
    responses(
        (status = 200, description = "Current Private DNS status", body = PrivateDnsStatusResponse),
        AuthErrors,
    ),
)]
pub async fn get_status(
    State(state): State<AppState>,
    _auth: SessionAuth,
) -> Result<Json<PrivateDnsStatusResponse>, AppError> {
    Ok(Json(build_status(&state).await?))
}

#[utoipa::path(
    put,
    path = "/api/private-dns",
    tag = TAG,
    description = "Enable or disable Private DNS. Enabling is Premium-gated and requires the \
                   hard prerequisites — the wardnet DNS provider, an issued TLS certificate, \
                   and an active entitlement — returning 409 when any is missing \
                   (reverse-tunnel readiness is informational and never blocks). On enable the \
                   daemon seeds the split-horizon wildcard record and starts the `:853` \
                   listener; on disable it removes the record and stops it. Returns the full \
                   status. Admin only.",
    request_body = SetPrivateDnsEnabledRequest,
    responses(
        (status = 200, description = "Private DNS state applied", body = PrivateDnsStatusResponse),
        AuthErrors,
        Conflict,
    ),
)]
pub async fn set_enabled(
    State(state): State<AppState>,
    _auth: SessionAuth,
    Json(body): Json<SetPrivateDnsEnabledRequest>,
) -> Result<Json<PrivateDnsStatusResponse>, AppError> {
    state
        .private_dns_service()
        .set_enabled(body.enabled)
        .await?;
    Ok(Json(build_status(&state).await?))
}

#[utoipa::path(
    post,
    path = "/api/private-dns/grants",
    tag = TAG,
    description = "Grant a managed device encrypted-DNS access: mint its secret hostname \
                   (a high-entropy label under the wardnet wildcard, never in CT logs). \
                   Requires the feature to be enabled; returns 409 if the device already \
                   holds a grant or the feature is disabled, and 404 if the device is \
                   unknown. The response carries the full minted hostname. Admin only.",
    request_body = CreatePrivateDnsGrantRequest,
    responses(
        (status = 201, description = "Device granted", body = PrivateDnsGrantSummary),
        AuthErrors,
        Conflict,
        NotFound,
    ),
)]
pub async fn create_grant(
    State(state): State<AppState>,
    _auth: SessionAuth,
    Json(body): Json<CreatePrivateDnsGrantRequest>,
) -> Result<(StatusCode, Json<PrivateDnsGrantSummary>), AppError> {
    let grant = state
        .private_dns_service()
        .grant_device(body.device_id)
        .await?;
    // Domain is known here (granting requires the feature enabled, which
    // requires the wardnet provider), but read it back rather than assume —
    // via DDNS directly, matching how `build_status` resolves it.
    let domain = wardnet_domain(&state.ddns_service().status().await?);
    let summary = PrivateDnsGrantSummary {
        id: grant.id,
        device_id: grant.device_id,
        hostname: hostname(&grant.token, domain.as_deref()),
        created_at: grant.created_at,
    };
    Ok((StatusCode::CREATED, Json(summary)))
}

#[utoipa::path(
    delete,
    path = "/api/private-dns/grants/{device_id}",
    tag = TAG,
    description = "Revoke a device's encrypted-DNS grant by device id: delete its hostname \
                   and immediately cut off any established `DoT` session (the token is \
                   otherwise only checked at handshake). Returns 404 if the device has no \
                   grant. Admin only.",
    params(("device_id" = Uuid, Path, description = "Device ID")),
    responses(
        (status = 204, description = "Grant revoked"),
        AuthErrors,
        NotFound,
    ),
)]
pub async fn delete_grant(
    State(state): State<AppState>,
    _auth: SessionAuth,
    Path(device_id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    let Some(grant) = state.private_dns_service().device_grant(device_id).await? else {
        return Err(AppError::NotFound(format!(
            "device {device_id} has no Private DNS grant"
        )));
    };
    state.private_dns_service().revoke_grant(grant.id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/api/private-dns/grants/{device_id}/notify",
    tag = TAG,
    description = "Send a device-keyed push nudging the granted device's household member to \
                   set up Private DNS. The notification deep-links the user PWA to \
                   `/settings#private-dns`, where the Private DNS card carries the setup \
                   steps. Returns \
                   `delivered: true` when at least one push \
                   subscription for the device was targeted, and `delivered: false` (a 200, not \
                   an error) when the device holds a grant but has no subscription — the member \
                   simply hasn't enabled notifications yet. Returns 404 when the device has no \
                   grant. Admin only.",
    params(("device_id" = Uuid, Path, description = "Device ID")),
    responses(
        (status = 200, description = "Notification dispatch result", body = SendPrivateDnsNotificationResponse),
        AuthErrors,
        NotFound,
    ),
)]
pub async fn notify_device(
    State(state): State<AppState>,
    _auth: SessionAuth,
    Path(device_id): Path<Uuid>,
) -> Result<Json<SendPrivateDnsNotificationResponse>, AppError> {
    let delivered = state.private_dns_service().notify_device(device_id).await?;
    Ok(Json(SendPrivateDnsNotificationResponse { delivered }))
}

#[utoipa::path(
    operation_id = "private_dns_get_me",
    get,
    path = "/api/private-dns/me",
    tag = TAG,
    description = "The caller device's own Private DNS state, identified by source IP (like \
                   `GET /api/devices/me`): whether the feature is enabled, whether this device \
                   is granted, and its full hostname when granted. Backs the user PWA. No \
                   authentication required — the daemon trusts the source IP of the incoming \
                   TCP connection.",
    responses(
        (status = 200, description = "This device's Private DNS state", body = PrivateDnsMeResponse),
        (status = 500, description = "Internal server error", body = ApiError),
    ),
    security(()),
)]
pub async fn get_me(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
) -> Result<Json<PrivateDnsMeResponse>, AppError> {
    // Bare config read — no DDNS round-trip, so a transient DDNS error can't
    // 500 the user PWA when this device has no grant to resolve a hostname for.
    let enabled =
        auth_context::with_context(system_admin(), state.private_dns_service().is_enabled())
            .await?;

    // Resolve the caller device by source IP (self-service, like /devices/me).
    let device = state
        .device_service()
        .get_device_for_ip(&ip.to_string())
        .await?
        .device;

    let grant = match &device {
        Some(device) => {
            auth_context::with_context(
                system_admin(),
                state.private_dns_service().device_grant(device.id),
            )
            .await?
        }
        None => None,
    };

    // A hostname is only meaningful when the feature is on *and* the device is
    // granted — matching `/me/profile`, which 404s while disabled. Resolve the
    // domain lazily then, degrading to no hostname on a DDNS hiccup rather than
    // failing the whole call.
    let hostname = match (enabled, &grant) {
        (true, Some(grant)) => {
            let domain = auth_context::with_context(system_admin(), state.ddns_service().status())
                .await
                .ok()
                .and_then(|ddns| wardnet_domain(&ddns));
            domain.map(|domain| hostname(&grant.token, Some(&domain)))
        }
        _ => None,
    };

    Ok(Json(PrivateDnsMeResponse {
        enabled,
        granted: grant.is_some(),
        hostname,
    }))
}

#[utoipa::path(
    get,
    path = "/api/private-dns/me/profile",
    tag = TAG,
    description = "The signed iOS configuration profile (`.mobileconfig`) for the caller \
                   device, identified by source IP. Served as \
                   `application/x-apple-aspen-config` at a plain, navigable URL so Safari's \
                   install flow triggers on a real navigation. Returns 404 when this device \
                   isn't granted or the feature is disabled. No authentication required.",
    responses(
        (status = 200, description = "Signed configuration profile", content_type = "application/x-apple-aspen-config", body = Vec<u8>),
        (status = 404, description = "No profile for this device", body = ApiError),
        (status = 500, description = "Internal server error", body = ApiError),
    ),
    security(()),
)]
pub async fn get_me_profile(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
) -> Result<impl IntoResponse, AppError> {
    let Some(device) = state
        .device_service()
        .get_device_for_ip(&ip.to_string())
        .await?
        .device
    else {
        return Err(AppError::NotFound("no profile for this device".to_owned()));
    };

    let profile = auth_context::with_context(
        system_admin(),
        state.private_dns_service().device_profile(device.id),
    )
    .await?;

    let Some(profile) = profile else {
        return Err(AppError::NotFound("no profile for this device".to_owned()));
    };

    Ok((
        [(header::CONTENT_TYPE, MOBILECONFIG_CONTENT_TYPE)],
        Body::from(profile),
    ))
}
