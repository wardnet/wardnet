use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;
use wardnet_common::api::{
    CreateDhcpReservationRequest, CreateDhcpReservationResponse, DeleteDhcpReservationResponse,
    DhcpConfigResponse, DhcpStatusResponse, ListDhcpLeasesResponse, ListDhcpReservationsResponse,
    RevokeDhcpLeaseResponse, ToggleDhcpRequest, UpdateDhcpConfigRequest,
};

use crate::api::middleware::AdminAuth;
use crate::api::responses::{AuthErrors, BadRequest, NotFound};
use crate::state::AppState;
use wardnetd_services::error::AppError;

/// Register DHCP routes onto the given [`OpenApiRouter`].
pub fn register(router: OpenApiRouter<AppState>) -> OpenApiRouter<AppState> {
    router
        .routes(routes!(get_config, update_config))
        .routes(routes!(toggle))
        .routes(routes!(list_leases))
        .routes(routes!(revoke_lease))
        .routes(routes!(list_reservations, create_reservation))
        .routes(routes!(delete_reservation))
        .routes(routes!(status))
}

const TAG: &str = "dhcp";
const PATH_CONFIG: &str = "/api/dhcp/config";
const PATH_TOGGLE: &str = "/api/dhcp/config/toggle";
const PATH_LEASES: &str = "/api/dhcp/leases";
const PATH_LEASE_ITEM: &str = "/api/dhcp/leases/{id}";
const PATH_RESERVATIONS: &str = "/api/dhcp/reservations";
const PATH_RESERVATION_ITEM: &str = "/api/dhcp/reservations/{id}";
const PATH_STATUS: &str = "/api/dhcp/status";

#[utoipa::path(
    get,
    path = PATH_CONFIG,
    tag = TAG,
    description = "Return the current DHCP pool configuration (enabled flag, range, \
                   lease time, default options). Admin only.",
    responses(
        (status = 200, description = "Current DHCP configuration", body = DhcpConfigResponse),
        AuthErrors,
    ),
    security(
        ("session_cookie" = []),
        ("bearer_auth" = []),
    ),
)]
pub async fn get_config(
    State(state): State<AppState>,
    _auth: AdminAuth,
) -> Result<Json<DhcpConfigResponse>, AppError> {
    let response = state.dhcp_service().get_config().await?;
    Ok(Json(response))
}

#[utoipa::path(
    put,
    path = PATH_CONFIG,
    tag = TAG,
    description = "Update the DHCP pool configuration (range, lease time, default \
                   options). Does not toggle the enabled flag — use the \
                   /api/dhcp/config/toggle endpoint for that. Admin only.",
    request_body = UpdateDhcpConfigRequest,
    responses(
        (status = 200, description = "Updated DHCP configuration", body = DhcpConfigResponse),
        AuthErrors,
        BadRequest,
    ),
    security(
        ("session_cookie" = []),
        ("bearer_auth" = []),
    ),
)]
pub async fn update_config(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Json(body): Json<UpdateDhcpConfigRequest>,
) -> Result<Json<DhcpConfigResponse>, AppError> {
    let response = state.dhcp_service().update_config(body).await?;
    Ok(Json(response))
}

#[utoipa::path(
    post,
    path = PATH_TOGGLE,
    tag = TAG,
    description = "Enable or disable the DHCP server. In addition to persisting the \
                   enabled flag, this starts or stops the live DHCP server process so \
                   the change takes effect immediately. Admin only.",
    request_body = ToggleDhcpRequest,
    responses(
        (status = 200, description = "Updated DHCP configuration", body = DhcpConfigResponse),
        AuthErrors,
        BadRequest,
    ),
    security(
        ("session_cookie" = []),
        ("bearer_auth" = []),
    ),
)]
pub async fn toggle(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Json(body): Json<ToggleDhcpRequest>,
) -> Result<Json<DhcpConfigResponse>, AppError> {
    let enabled = body.enabled;
    let response = state.dhcp_service().toggle(body).await?;

    // Start or stop the DHCP server based on the new config.
    if enabled {
        state.dhcp_server().start().await?;
    } else {
        state.dhcp_server().stop().await?;
    }

    Ok(Json(response))
}

#[utoipa::path(
    get,
    path = PATH_LEASES,
    tag = TAG,
    description = "List every active DHCP lease currently tracked by the daemon, \
                   including MAC, IP, hostname, and expiry. Admin only.",
    responses(
        (status = 200, description = "Active DHCP leases", body = ListDhcpLeasesResponse),
        AuthErrors,
    ),
    security(
        ("session_cookie" = []),
        ("bearer_auth" = []),
    ),
)]
pub async fn list_leases(
    State(state): State<AppState>,
    _auth: AdminAuth,
) -> Result<Json<ListDhcpLeasesResponse>, AppError> {
    let response = state.dhcp_service().list_leases().await?;
    Ok(Json(response))
}

#[utoipa::path(
    delete,
    path = PATH_LEASE_ITEM,
    tag = TAG,
    description = "Revoke an active DHCP lease by ID. The IP returns to the pool \
                   and the client will re-negotiate on its next renewal cycle. \
                   Admin only.",
    params(("id" = Uuid, Path, description = "Lease ID")),
    responses(
        (status = 200, description = "Lease revoked", body = RevokeDhcpLeaseResponse),
        AuthErrors,
        NotFound,
    ),
    security(
        ("session_cookie" = []),
        ("bearer_auth" = []),
    ),
)]
pub async fn revoke_lease(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Path(id): Path<Uuid>,
) -> Result<Json<RevokeDhcpLeaseResponse>, AppError> {
    let response = state.dhcp_service().revoke_lease(id).await?;
    Ok(Json(response))
}

#[utoipa::path(
    get,
    path = PATH_RESERVATIONS,
    tag = TAG,
    description = "List every configured static MAC-to-IP DHCP reservation. \
                   Reservations take precedence over dynamic leases. Admin only.",
    responses(
        (status = 200, description = "Static DHCP reservations", body = ListDhcpReservationsResponse),
        AuthErrors,
    ),
    security(
        ("session_cookie" = []),
        ("bearer_auth" = []),
    ),
)]
pub async fn list_reservations(
    State(state): State<AppState>,
    _auth: AdminAuth,
) -> Result<Json<ListDhcpReservationsResponse>, AppError> {
    let response = state.dhcp_service().list_reservations().await?;
    Ok(Json(response))
}

#[utoipa::path(
    post,
    path = PATH_RESERVATIONS,
    tag = TAG,
    description = "Create a new static MAC-to-IP DHCP reservation so a given device \
                   always receives the same IP. Admin only.",
    request_body = CreateDhcpReservationRequest,
    responses(
        (status = 201, description = "Reservation created", body = CreateDhcpReservationResponse),
        AuthErrors,
        BadRequest,
    ),
    security(
        ("session_cookie" = []),
        ("bearer_auth" = []),
    ),
)]
pub async fn create_reservation(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Json(body): Json<CreateDhcpReservationRequest>,
) -> Result<(StatusCode, Json<CreateDhcpReservationResponse>), AppError> {
    let response = state.dhcp_service().create_reservation(body).await?;
    Ok((StatusCode::CREATED, Json(response)))
}

#[utoipa::path(
    delete,
    path = PATH_RESERVATION_ITEM,
    tag = TAG,
    description = "Delete a static DHCP reservation by ID. Any existing lease for the \
                   MAC is preserved until its natural expiry. Admin only.",
    params(("id" = Uuid, Path, description = "Reservation ID")),
    responses(
        (status = 200, description = "Reservation deleted", body = DeleteDhcpReservationResponse),
        AuthErrors,
        NotFound,
    ),
    security(
        ("session_cookie" = []),
        ("bearer_auth" = []),
    ),
)]
pub async fn delete_reservation(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Path(id): Path<Uuid>,
) -> Result<Json<DeleteDhcpReservationResponse>, AppError> {
    let response = state.dhcp_service().delete_reservation(id).await?;
    Ok(Json(response))
}

#[utoipa::path(
    get,
    path = PATH_STATUS,
    tag = TAG,
    description = "Get live DHCP server status and pool usage (running flag, total/used \
                   addresses, lease counts). Admin only.",
    responses(
        (status = 200, description = "DHCP server status and pool usage", body = DhcpStatusResponse),
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
) -> Result<Json<DhcpStatusResponse>, AppError> {
    let response = state.dhcp_service().status().await?;
    Ok(Json(response))
}
