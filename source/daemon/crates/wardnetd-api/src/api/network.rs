use axum::Json;
use axum::extract::State;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use wardnet_common::api::{
    DhcpSelfProbeResponse, DiscoverGatewayMacRequest, DiscoverGatewayMacResponse,
    NetworkStatusResponse,
};

use crate::api::middleware::AdminAuth;
use crate::api::responses::AuthErrors;
use crate::state::AppState;
use wardnetd_services::error::AppError;

/// Register network routes onto the given [`OpenApiRouter`].
pub fn register(router: OpenApiRouter<AppState>) -> OpenApiRouter<AppState> {
    router
        .routes(routes!(status))
        .routes(routes!(discover_gateway_mac))
        .routes(routes!(dhcp_self_probe))
}

#[utoipa::path(
    get,
    path = "/api/network/status",
    tag = "network",
    description = "Read the LAN interface's current address + default \
                   gateway and classify whether the IP came from DHCP \
                   or a Wardnet-managed static config (install.sh \
                   --static-ip writes /etc/dhcpcd.conf.d/wardnet.conf, \
                   which flips dhcp_source to \"static\"). Powers the \
                   wizard's network step and the Settings page. Admin only.",
    responses(
        (status = 200, description = "Current LAN interface state", body = NetworkStatusResponse),
        AuthErrors,
    ),
    security(
        ("session_cookie" = []),
        ("bearer_auth" = []),
    ),
)]
pub async fn status(
    State(state): State<AppState>,
    _auth: AdminAuth,
) -> Result<Json<NetworkStatusResponse>, AppError> {
    let response = state.system_service().network_status().await?;
    Ok(Json(response))
}

#[utoipa::path(
    post,
    path = "/api/network/discover-gateway-mac",
    tag = "network",
    description = "Discover (or accept) the upstream router MAC address. \
                   With an empty body, the daemon ARP-probes the LAN \
                   gateway from `GET /api/network/status` and returns the \
                   responder's MAC. Pass `mac` to skip the probe and \
                   record an operator-supplied value (validated). Pass \
                   `target_ip` to override the probe target. The result \
                   is persisted in `system_config` so subsequent calls \
                   are idempotent. Admin only.",
    request_body = DiscoverGatewayMacRequest,
    responses(
        (status = 200, description = "MAC discovered or accepted", body = DiscoverGatewayMacResponse),
        (status = 400, description = "Invalid MAC or no gateway to probe", body = wardnet_common::api::ApiError),
        (status = 404, description = "ARP probe got no reply", body = wardnet_common::api::ApiError),
        AuthErrors,
    ),
    security(
        ("session_cookie" = []),
        ("bearer_auth" = []),
    ),
)]
pub async fn discover_gateway_mac(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Json(body): Json<DiscoverGatewayMacRequest>,
) -> Result<Json<DiscoverGatewayMacResponse>, AppError> {
    let response = state.system_service().discover_gateway_mac(body).await?;
    Ok(Json(response))
}

#[utoipa::path(
    post,
    path = "/api/network/dhcp-self-probe",
    tag = "network",
    description = "Broadcast a DHCPDISCOVER from a synthetic MAC and report \
                   whether Wardnet (and any foreign DHCP server) responded \
                   with an OFFER. The wizard's primary-mode step 3 calls \
                   this after the operator says they've disabled DHCP on \
                   their router; a foreign-only response tells the UI to \
                   re-show the disable-DHCP guide. Admin only.",
    responses(
        (status = 200, description = "Probe complete", body = DhcpSelfProbeResponse),
        AuthErrors,
    ),
    security(
        ("session_cookie" = []),
        ("bearer_auth" = []),
    ),
)]
pub async fn dhcp_self_probe(
    State(state): State<AppState>,
    _auth: AdminAuth,
) -> Result<Json<DhcpSelfProbeResponse>, AppError> {
    let response = state.system_service().dhcp_self_probe().await?;
    Ok(Json(response))
}
