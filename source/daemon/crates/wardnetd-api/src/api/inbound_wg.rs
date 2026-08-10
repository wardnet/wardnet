use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;
use wardnet_common::api::{
    AddInboundWgPeerRequest, AddInboundWgPeerResponse, InboundWgConfigRequest,
    InboundWgConfigResponse, InboundWgPeerSummary, ListInboundWgPeersResponse,
    SetInboundWgPeerEnabledRequest,
};

use crate::api::middleware::SessionAuth;
use crate::api::responses::{AuthErrors, Conflict, NotFound};
use crate::state::AppState;
use wardnetd_services::error::AppError;

/// Register inbound-`WireGuard` routes onto the given [`OpenApiRouter`].
pub fn register(router: OpenApiRouter<AppState>) -> OpenApiRouter<AppState> {
    router
        .routes(routes!(get_config, set_config))
        .routes(routes!(list_peers, add_peer))
        .routes(routes!(remove_peer, set_peer_enabled))
}

#[utoipa::path(
    operation_id = "inbound_wg_get_config",
    get,
    path = "/api/inbound-wg/config",
    tag = "inbound-wg",
    description = "Read the current inbound WireGuard server config (enabled, listen port, \
                   public key) without mutating anything. Admin only.",
    responses(
        (status = 200, description = "Current inbound WireGuard config", body = InboundWgConfigResponse),
        AuthErrors,
    ),
)]
pub async fn get_config(
    State(state): State<AppState>,
    _auth: SessionAuth,
) -> Result<Json<InboundWgConfigResponse>, AppError> {
    let response = state.inbound_wg_service().get_config().await?;
    Ok(Json(response))
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
)]
pub async fn set_config(
    State(state): State<AppState>,
    _auth: SessionAuth,
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
)]
pub async fn list_peers(
    State(state): State<AppState>,
    _auth: SessionAuth,
) -> Result<Json<ListInboundWgPeersResponse>, AppError> {
    let peers = state.inbound_wg_service().list_peers().await?;
    Ok(Json(ListInboundWgPeersResponse { peers }))
}

#[utoipa::path(
    post,
    path = "/api/inbound-wg/peers",
    tag = "inbound-wg",
    description = "Grant remote access to an already-managed device via inbound WireGuard. \
                   The daemon generates a fresh keypair, allocates the next free address on \
                   the inbound subnet, stores the peer (public key + device link), and admits \
                   it onto the server interface. The peer's name is taken from the device. \
                   The response carries the peer's **private key exactly once** — it is never \
                   persisted, so it must be copied now. Returns 409 if the server is disabled \
                   or the device already has a credential, 404 if the device does not exist. \
                   Admin only.",
    request_body = AddInboundWgPeerRequest,
    responses(
        (status = 201, description = "Peer admitted", body = AddInboundWgPeerResponse),
        AuthErrors,
        Conflict,
        NotFound,
    ),
)]
pub async fn add_peer(
    State(state): State<AppState>,
    _auth: SessionAuth,
    Json(body): Json<AddInboundWgPeerRequest>,
) -> Result<(StatusCode, Json<AddInboundWgPeerResponse>), AppError> {
    // Compose the reachable endpoint here (the API layer has both services):
    // the client dials `<host>:<listen_port>`. `host` is the public hostname
    // (or last-known public IP) from DDNS — a placeholder for the real cloud
    // relay until #824 lands. When neither is known, the peer is still granted
    // but the response carries no client config.
    let endpoint = build_endpoint(&state).await?;
    let response = state
        .inbound_wg_service()
        .add_peer(body.device_id, endpoint)
        .await?;
    Ok((StatusCode::CREATED, Json(response)))
}

/// `<host>:<listen_port>` for a peer's `Endpoint`, or `None` when no public
/// hostname / IP is configured yet. Prefers the DDNS FQDN, falling back to the
/// last-known public IP.
async fn build_endpoint(state: &AppState) -> Result<Option<String>, AppError> {
    let ddns = state.ddns_service().status().await?;
    let host = ddns.fqdn.or(ddns.last_public_ip);
    let Some(host) = host else {
        return Ok(None);
    };
    let listen_port = state.inbound_wg_service().get_config().await?.listen_port;
    Ok(Some(format!("{host}:{listen_port}")))
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
)]
pub async fn remove_peer(
    State(state): State<AppState>,
    _auth: SessionAuth,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    state.inbound_wg_service().remove_peer(id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    patch,
    path = "/api/inbound-wg/peers/{id}",
    tag = "inbound-wg",
    description = "Pause or resume an inbound WireGuard peer without deleting its \
                   credential: re-admits it onto the live interface (enable) or \
                   best-effort removes it (disable), then persists the flag. Distinct \
                   from DELETE, which revokes the credential permanently — a paused \
                   peer can be resumed without a fresh keypair or QR scan. Admin only.",
    params(("id" = Uuid, Path, description = "Peer ID")),
    request_body = SetInboundWgPeerEnabledRequest,
    responses(
        (status = 200, description = "Peer state applied", body = InboundWgPeerSummary),
        AuthErrors,
        NotFound,
    ),
)]
pub async fn set_peer_enabled(
    State(state): State<AppState>,
    _auth: SessionAuth,
    Path(id): Path<Uuid>,
    Json(body): Json<SetInboundWgPeerEnabledRequest>,
) -> Result<Json<InboundWgPeerSummary>, AppError> {
    let response = state
        .inbound_wg_service()
        .set_peer_enabled(id, body.enabled)
        .await?;
    Ok(Json(response))
}
