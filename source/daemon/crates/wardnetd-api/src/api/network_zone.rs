//! Network Zones HTTP handlers — `/api/network/zones/...` (epic #244, issue #735).
//!
//! CRUD over the device policy-bucket catalog. Device→zone assignment lives as
//! a sub-resource of the device (`PUT /api/devices/{id}/zone`) in
//! [`super::devices`]. All handlers are admin-only.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;
use wardnet_common::api::{
    CreateNetworkZoneRequest, CreateNetworkZoneResponse, DeleteNetworkZoneResponse,
    GetNetworkZoneResponse, ListNetworkZonesResponse, QuarantineNewDevicesResponse,
    SetQuarantineNewDevicesRequest, UpdateNetworkZoneRequest, UpdateNetworkZoneResponse,
};

use crate::api::middleware::AdminAuth;
use crate::api::responses::{AuthErrors, BadRequest, NotFound};
use crate::state::AppState;
use wardnetd_services::error::AppError;

pub fn register(router: OpenApiRouter<AppState>) -> OpenApiRouter<AppState> {
    router
        .routes(routes!(list_zones, create_zone))
        .routes(routes!(
            get_quarantine_new_devices,
            set_quarantine_new_devices
        ))
        .routes(routes!(get_zone, update_zone, delete_zone))
}

#[utoipa::path(
    get,
    path = "/api/network/zones",
    tag = "network-zones",
    description = "List every Network Zone with its current member count. \
                   Admin only.",
    responses(
        (status = 200, description = "Zones list", body = ListNetworkZonesResponse),
        AuthErrors,
    ),
)]
pub async fn list_zones(
    State(state): State<AppState>,
    _auth: AdminAuth,
) -> Result<Json<ListNetworkZonesResponse>, AppError> {
    let zones = state.network_zone_service().list_zones().await?;
    Ok(Json(ListNetworkZonesResponse { zones }))
}

#[utoipa::path(
    post,
    path = "/api/network/zones",
    tag = "network-zones",
    description = "Create a Network Zone. `provenance` is forced to manual and \
                   default flags are never set on create (use PUT to promote). \
                   Admin only.",
    request_body = CreateNetworkZoneRequest,
    responses(
        (status = 201, description = "Zone created", body = CreateNetworkZoneResponse),
        AuthErrors,
        BadRequest,
        (status = 409, description = "Name already in use", body = wardnet_common::api::ApiError),
    ),
)]
pub async fn create_zone(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Json(body): Json<CreateNetworkZoneRequest>,
) -> Result<(StatusCode, Json<CreateNetworkZoneResponse>), AppError> {
    let zone = state.network_zone_service().create_zone(body).await?;
    Ok((
        StatusCode::CREATED,
        Json(CreateNetworkZoneResponse { zone }),
    ))
}

#[utoipa::path(
    get,
    path = "/api/network/zones/{id}",
    tag = "network-zones",
    description = "Fetch a single Network Zone with its member count. Admin only.",
    params(("id" = Uuid, Path, description = "Zone id")),
    responses(
        (status = 200, description = "Zone", body = GetNetworkZoneResponse),
        AuthErrors,
        NotFound,
    ),
)]
pub async fn get_zone(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Path(id): Path<Uuid>,
) -> Result<Json<GetNetworkZoneResponse>, AppError> {
    let zone = state.network_zone_service().get_zone(id).await?;
    Ok(Json(GetNetworkZoneResponse { zone }))
}

#[utoipa::path(
    put,
    path = "/api/network/zones/{id}",
    tag = "network-zones",
    description = "Partially update a Network Zone. Setting `is_default` or \
                   `is_default_for_new` to true promotes this zone; false is \
                   rejected (flags only move by promoting another zone). System \
                   zones are editable but not deletable. Admin only.",
    params(("id" = Uuid, Path, description = "Zone id")),
    request_body = UpdateNetworkZoneRequest,
    responses(
        (status = 200, description = "Updated zone", body = UpdateNetworkZoneResponse),
        AuthErrors,
        NotFound,
        BadRequest,
        (status = 409, description = "Name already in use", body = wardnet_common::api::ApiError),
    ),
)]
pub async fn update_zone(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateNetworkZoneRequest>,
) -> Result<Json<UpdateNetworkZoneResponse>, AppError> {
    let zone = state.network_zone_service().update_zone(id, body).await?;
    Ok(Json(UpdateNetworkZoneResponse { zone }))
}

#[utoipa::path(
    delete,
    path = "/api/network/zones/{id}",
    tag = "network-zones",
    description = "Delete a Network Zone. Rejected for system zones, the anchor \
                   (default) zone, or zones that still have member devices. \
                   Admin only.",
    params(("id" = Uuid, Path, description = "Zone id")),
    responses(
        (status = 200, description = "Zone deleted", body = DeleteNetworkZoneResponse),
        AuthErrors,
        NotFound,
        (status = 409, description = "System / default / non-empty zone", body = wardnet_common::api::ApiError),
    ),
)]
pub async fn delete_zone(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Path(id): Path<Uuid>,
) -> Result<Json<DeleteNetworkZoneResponse>, AppError> {
    state.network_zone_service().delete_zone(id).await?;
    Ok(Json(DeleteNetworkZoneResponse { deleted: true }))
}

#[utoipa::path(
    get,
    path = "/api/network/quarantine-new-devices",
    tag = "network-zones",
    description = "Read the new-device quarantine toggle (issue #738). When on, a \
                   freshly-discovered device lands in the default-for-new zone (as \
                   always) and admins get a push to approve it. Off by default. \
                   Admin only.",
    responses(
        (status = 200, description = "Current toggle state", body = QuarantineNewDevicesResponse),
        AuthErrors,
    ),
)]
pub async fn get_quarantine_new_devices(
    State(state): State<AppState>,
    _auth: AdminAuth,
) -> Result<Json<QuarantineNewDevicesResponse>, AppError> {
    let enabled = state
        .network_zone_service()
        .get_quarantine_new_devices()
        .await?;
    Ok(Json(QuarantineNewDevicesResponse { enabled }))
}

#[utoipa::path(
    put,
    path = "/api/network/quarantine-new-devices",
    tag = "network-zones",
    description = "Enable or disable new-device quarantine (issue #738). Placement \
                   is unchanged either way (new devices always land in the \
                   default-for-new zone); the toggle only controls whether admins \
                   are notified to approve. Admin only.",
    request_body = SetQuarantineNewDevicesRequest,
    responses(
        (status = 200, description = "Updated toggle state", body = QuarantineNewDevicesResponse),
        AuthErrors,
        BadRequest,
    ),
)]
pub async fn set_quarantine_new_devices(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Json(body): Json<SetQuarantineNewDevicesRequest>,
) -> Result<Json<QuarantineNewDevicesResponse>, AppError> {
    state
        .network_zone_service()
        .set_quarantine_new_devices(body.enabled)
        .await?;
    Ok(Json(QuarantineNewDevicesResponse {
        enabled: body.enabled,
    }))
}
