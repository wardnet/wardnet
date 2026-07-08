use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;
use wardnet_common::api::{
    AddInboundWgPeerRequest, AddInboundWgPeerResponse, InboundWgConfigRequest,
    InboundWgConfigResponse, ListInboundWgPeersResponse,
};

use crate::api::middleware::AdminAuth;
use crate::api::responses::{AuthErrors, Conflict, NotFound};
use crate::state::AppState;
use wardnetd_services::error::AppError;

/// Register inbound-`WireGuard` routes onto the given [`OpenApiRouter`].
pub fn register(router: OpenApiRouter<AppState>) -> OpenApiRouter<AppState> {
    router
        .routes(routes!(set_config))
        .routes(routes!(list_peers, add_peer))
        .routes(routes!(remove_peer))
}

#[utoipa::path(
    put,
    path = "/api/inbound-wg/config",
    tag = "inbound-wg",
    description = "Enable or disable the inbound (multi-peer) WireGuard server and set \
                   its UDP listen port. On enable the daemon generates the server keypair \
                   (once), stands up the `wg_wardin0` interface, installs the NAT masquerade \
                   and listen-port accept firewall rules, and re-admits every enabled peer. \
                   On disable it removes those rules and tears the interface down; peer \
                   definitions are preserved. Admin only.",
    request_body = InboundWgConfigRequest,
    responses(
        (status = 200, description = "Inbound WireGuard config applied", body = InboundWgConfigResponse),
        AuthErrors,
    ),
    security(
        ("session_cookie" = []),
        ("bearer_auth" = []),
    ),
)]
pub async fn set_config(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Json(body): Json<InboundWgConfigRequest>,
) -> Result<Json<InboundWgConfigResponse>, AppError> {
    let response = state
        .inbound_wg_service()
        .set_config(body.enabled, body.listen_port)
        .await?;
    Ok(Json(response))
}

#[utoipa::path(
    get,
    path = "/api/inbound-wg/peers",
    tag = "inbound-wg",
    description = "List every admitted inbound WireGuard peer with its name, public key, \
                   allocated address, and enabled state. Private keys are never returned. \
                   Admin only.",
    responses(
        (status = 200, description = "Configured inbound peers", body = ListInboundWgPeersResponse),
        AuthErrors,
    ),
    security(
        ("session_cookie" = []),
        ("bearer_auth" = []),
    ),
)]
pub async fn list_peers(
    State(state): State<AppState>,
    _auth: AdminAuth,
) -> Result<Json<ListInboundWgPeersResponse>, AppError> {
    let peers = state.inbound_wg_service().list_peers().await?;
    Ok(Json(ListInboundWgPeersResponse { peers }))
}

#[utoipa::path(
    post,
    path = "/api/inbound-wg/peers",
    tag = "inbound-wg",
    description = "Admit a new inbound WireGuard peer. The daemon generates a fresh keypair, \
                   allocates the next free address on the inbound subnet, stores the peer \
                   (public key only), and admits it onto the server interface. The response \
                   carries the peer's **private key exactly once** — it is never persisted, \
                   so it must be copied now. Returns 409 if the server is disabled. Admin only.",
    request_body = AddInboundWgPeerRequest,
    responses(
        (status = 201, description = "Peer admitted", body = AddInboundWgPeerResponse),
        AuthErrors,
        Conflict,
    ),
    security(
        ("session_cookie" = []),
        ("bearer_auth" = []),
    ),
)]
pub async fn add_peer(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Json(body): Json<AddInboundWgPeerRequest>,
) -> Result<(StatusCode, Json<AddInboundWgPeerResponse>), AppError> {
    let response = state.inbound_wg_service().add_peer(body.name).await?;
    Ok((StatusCode::CREATED, Json(response)))
}

#[utoipa::path(
    delete,
    path = "/api/inbound-wg/peers/{id}",
    tag = "inbound-wg",
    description = "Remove an inbound WireGuard peer by id: drop it from the live server \
                   interface and delete its stored definition. Admin only.",
    params(("id" = Uuid, Path, description = "Peer ID")),
    responses(
        (status = 204, description = "Peer removed"),
        AuthErrors,
        NotFound,
    ),
    security(
        ("session_cookie" = []),
        ("bearer_auth" = []),
    ),
)]
pub async fn remove_peer(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    state.inbound_wg_service().remove_peer(id).await?;
    Ok(StatusCode::NO_CONTENT)
}
