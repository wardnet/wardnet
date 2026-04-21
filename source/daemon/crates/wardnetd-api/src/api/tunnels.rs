use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;
use wardnet_common::api::{
    CreateTunnelRequest, CreateTunnelResponse, DeleteTunnelResponse, ListTunnelsResponse,
};

use crate::api::middleware::AdminAuth;
use crate::api::responses::{AuthErrors, BadRequest, NotFound};
use crate::state::AppState;
use wardnetd_services::error::AppError;

/// Register tunnel routes onto the given [`OpenApiRouter`].
pub fn register(router: OpenApiRouter<AppState>) -> OpenApiRouter<AppState> {
    router
        .routes(routes!(list_tunnels, create_tunnel))
        .routes(routes!(delete_tunnel))
}

#[utoipa::path(
    get,
    path = "/api/tunnels",
    tag = "tunnels",
    description = "List every configured VPN tunnel with its label, country, peer \
                   endpoint summary, and current runtime status. Admin only.",
    responses(
        (status = 200, description = "Configured tunnels", body = ListTunnelsResponse),
        AuthErrors,
    ),
    security(
        ("session_cookie" = []),
        ("bearer_auth" = []),
    ),
)]
pub async fn list_tunnels(
    State(state): State<AppState>,
    _auth: AdminAuth,
) -> Result<Json<ListTunnelsResponse>, AppError> {
    let response = state.tunnel_service().list_tunnels().await?;
    Ok(Json(response))
}

#[utoipa::path(
    post,
    path = "/api/tunnels",
    tag = "tunnels",
    description = "Import a tunnel from a WireGuard `.conf` file. The daemon parses \
                   the config, persists the tunnel definition, and stores the private \
                   key in the key store. Admin only.",
    request_body = CreateTunnelRequest,
    responses(
        (status = 201, description = "Tunnel imported", body = CreateTunnelResponse),
        AuthErrors,
        BadRequest,
    ),
    security(
        ("session_cookie" = []),
        ("bearer_auth" = []),
    ),
)]
pub async fn create_tunnel(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Json(body): Json<CreateTunnelRequest>,
) -> Result<(StatusCode, Json<CreateTunnelResponse>), AppError> {
    let response = state.tunnel_service().import_tunnel(body).await?;
    Ok((StatusCode::CREATED, Json(response)))
}

#[utoipa::path(
    delete,
    path = "/api/tunnels/{id}",
    tag = "tunnels",
    description = "Delete a tunnel by ID: tear down the live WireGuard interface if \
                   it is up, remove the persisted config, and delete the associated \
                   private key from the key store. Admin only.",
    params(("id" = Uuid, Path, description = "Tunnel ID")),
    responses(
        (status = 200, description = "Tunnel deleted", body = DeleteTunnelResponse),
        AuthErrors,
        NotFound,
    ),
    security(
        ("session_cookie" = []),
        ("bearer_auth" = []),
    ),
)]
pub async fn delete_tunnel(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Path(id): Path<Uuid>,
) -> Result<Json<DeleteTunnelResponse>, AppError> {
    let response = state.tunnel_service().delete_tunnel(id).await?;
    Ok(Json(response))
}
