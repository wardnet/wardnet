use axum::Json;
use axum::extract::State;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use wardnet_common::api::NetworkStatusResponse;

use crate::api::middleware::AdminAuth;
use crate::api::responses::AuthErrors;
use crate::state::AppState;
use wardnetd_services::error::AppError;

/// Register network routes onto the given [`OpenApiRouter`]. Currently
/// just `GET /api/network/status` — the DHCP self-probe and ARP
/// discovery endpoints land in follow-up commits.
pub fn register(router: OpenApiRouter<AppState>) -> OpenApiRouter<AppState> {
    router.routes(routes!(status))
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
