use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use uuid::Uuid;
use wardnet_types::api::{
    CreateTunnelRequest, CreateTunnelResponse, DeleteTunnelResponse, ListTunnelsResponse,
};

use crate::api::middleware::AdminAuth;
use crate::error::AppError;
use crate::state::AppState;

/// GET /api/tunnels
///
/// Thin handler — lists all configured tunnels. Requires admin authentication.
pub async fn list_tunnels(
    State(state): State<AppState>,
    _auth: AdminAuth,
) -> Result<Json<ListTunnelsResponse>, AppError> {
    let response = state.tunnel_service().list_tunnels().await?;
    Ok(Json(response))
}

/// POST /api/tunnels
///
/// Thin handler — imports a tunnel from a `WireGuard` `.conf` file.
/// Requires admin authentication.
pub async fn create_tunnel(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Json(body): Json<CreateTunnelRequest>,
) -> Result<(StatusCode, Json<CreateTunnelResponse>), AppError> {
    let response = state.tunnel_service().import_tunnel(body).await?;
    Ok((StatusCode::CREATED, Json(response)))
}

/// DELETE /api/tunnels/:id
///
/// Thin handler — deletes a tunnel and its configuration.
/// Requires admin authentication.
pub async fn delete_tunnel(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Path(id): Path<Uuid>,
) -> Result<Json<DeleteTunnelResponse>, AppError> {
    let response = state.tunnel_service().delete_tunnel(id).await?;
    Ok(Json(response))
}
