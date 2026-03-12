use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use uuid::Uuid;
use wardnet_types::api::{
    CreateDhcpReservationRequest, CreateDhcpReservationResponse, DeleteDhcpReservationResponse,
    DhcpConfigResponse, DhcpStatusResponse, ListDhcpLeasesResponse, ListDhcpReservationsResponse,
    RevokeDhcpLeaseResponse, ToggleDhcpRequest, UpdateDhcpConfigRequest,
};

use crate::api::middleware::AdminAuth;
use crate::error::AppError;
use crate::state::AppState;

/// GET /api/dhcp/config
///
/// Thin handler — returns the current DHCP pool configuration.
/// Requires admin authentication.
pub async fn get_config(
    State(state): State<AppState>,
    _auth: AdminAuth,
) -> Result<Json<DhcpConfigResponse>, AppError> {
    let response = state.dhcp_service().get_config().await?;
    Ok(Json(response))
}

/// PUT /api/dhcp/config
///
/// Thin handler — updates the DHCP pool configuration.
/// Requires admin authentication.
pub async fn update_config(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Json(body): Json<UpdateDhcpConfigRequest>,
) -> Result<Json<DhcpConfigResponse>, AppError> {
    let response = state.dhcp_service().update_config(body).await?;
    Ok(Json(response))
}

/// POST /api/dhcp/config/toggle
///
/// Thin handler — enables or disables the DHCP server.
/// Requires admin authentication.
pub async fn toggle(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Json(body): Json<ToggleDhcpRequest>,
) -> Result<Json<DhcpConfigResponse>, AppError> {
    let response = state.dhcp_service().toggle(body).await?;
    Ok(Json(response))
}

/// GET /api/dhcp/leases
///
/// Thin handler — lists all active DHCP leases.
/// Requires admin authentication.
pub async fn list_leases(
    State(state): State<AppState>,
    _auth: AdminAuth,
) -> Result<Json<ListDhcpLeasesResponse>, AppError> {
    let response = state.dhcp_service().list_leases().await?;
    Ok(Json(response))
}

/// DELETE /api/dhcp/leases/:id
///
/// Thin handler — revokes an active DHCP lease.
/// Requires admin authentication.
pub async fn revoke_lease(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Path(id): Path<Uuid>,
) -> Result<Json<RevokeDhcpLeaseResponse>, AppError> {
    let response = state.dhcp_service().revoke_lease(id).await?;
    Ok(Json(response))
}

/// GET /api/dhcp/reservations
///
/// Thin handler — lists all static DHCP reservations.
/// Requires admin authentication.
pub async fn list_reservations(
    State(state): State<AppState>,
    _auth: AdminAuth,
) -> Result<Json<ListDhcpReservationsResponse>, AppError> {
    let response = state.dhcp_service().list_reservations().await?;
    Ok(Json(response))
}

/// POST /api/dhcp/reservations
///
/// Thin handler — creates a new static MAC-to-IP reservation.
/// Requires admin authentication.
pub async fn create_reservation(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Json(body): Json<CreateDhcpReservationRequest>,
) -> Result<(StatusCode, Json<CreateDhcpReservationResponse>), AppError> {
    let response = state.dhcp_service().create_reservation(body).await?;
    Ok((StatusCode::CREATED, Json(response)))
}

/// DELETE /api/dhcp/reservations/:id
///
/// Thin handler — deletes a static DHCP reservation.
/// Requires admin authentication.
pub async fn delete_reservation(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Path(id): Path<Uuid>,
) -> Result<Json<DeleteDhcpReservationResponse>, AppError> {
    let response = state.dhcp_service().delete_reservation(id).await?;
    Ok(Json(response))
}

/// GET /api/dhcp/status
///
/// Thin handler — returns DHCP server status and pool usage.
/// Requires admin authentication.
pub async fn status(
    State(state): State<AppState>,
    _auth: AdminAuth,
) -> Result<Json<DhcpStatusResponse>, AppError> {
    let response = state.dhcp_service().status().await?;
    Ok(Json(response))
}
