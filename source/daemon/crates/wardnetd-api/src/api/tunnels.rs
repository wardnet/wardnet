use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use serde::Deserialize;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;
use wardnet_common::api::{
    CreateTunnelRequest, CreateTunnelResponse, DeleteTunnelResponse, ListTunnelsResponse,
    TunnelDetailResponse, TunnelDevicesResponse, TunnelMetricsRange, TunnelMetricsResponse,
    TunnelTestResponse, UpdateTunnelDnsOverrideRequest, UpdateTunnelDnsOverrideResponse,
};

use crate::api::middleware::AdminAuth;
use crate::api::responses::{AuthErrors, BadRequest, Conflict, NotFound, UpstreamUnavailable};
use crate::state::AppState;
use wardnetd_services::error::AppError;

/// Register tunnel routes onto the given [`OpenApiRouter`].
pub fn register(router: OpenApiRouter<AppState>) -> OpenApiRouter<AppState> {
    router
        .routes(routes!(list_tunnels, create_tunnel))
        .routes(routes!(get_tunnel, delete_tunnel))
        .routes(routes!(get_tunnel_metrics))
        .routes(routes!(list_tunnel_devices))
        .routes(routes!(test_tunnel))
        .routes(routes!(update_tunnel_dns_override))
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
    get,
    path = "/api/tunnels/{id}",
    tag = "tunnels",
    description = "Fetch a single tunnel by ID with its label, country, peer endpoint \
                   summary, runtime status, and live byte counters. Admin only.",
    params(("id" = Uuid, Path, description = "Tunnel ID")),
    responses(
        (status = 200, description = "Tunnel detail", body = TunnelDetailResponse),
        AuthErrors,
        NotFound,
    ),
    security(
        ("session_cookie" = []),
        ("bearer_auth" = []),
    ),
)]
pub async fn get_tunnel(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Path(id): Path<Uuid>,
) -> Result<Json<TunnelDetailResponse>, AppError> {
    let tunnel = state.tunnel_service().get_tunnel(id).await?;
    Ok(Json(TunnelDetailResponse { tunnel }))
}

/// Query string for `GET /api/tunnels/{id}/metrics`.
#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct GetMetricsQuery {
    /// Range selector. One of `1h`, `6h`, `24h`, `48h`, `12mo`. Defaults
    /// to `24h` when omitted.
    #[serde(default)]
    pub range: Option<TunnelMetricsRange>,
}

#[utoipa::path(
    get,
    path = "/api/tunnels/{id}/metrics",
    tag = "tunnels",
    description = "Throughput history for a tunnel. The `1h..48h` ranges return \
                   one point per `interval_secs` (5 min by default) carrying the \
                   bytes-tx/bytes-rx delta over that interval. The `12mo` range \
                   returns one point per day with the daily totals. Admin only.",
    params(
        ("id" = Uuid, Path, description = "Tunnel ID"),
        GetMetricsQuery,
    ),
    responses(
        (status = 200, description = "Throughput history", body = TunnelMetricsResponse),
        AuthErrors,
        NotFound,
    ),
    security(
        ("session_cookie" = []),
        ("bearer_auth" = []),
    ),
)]
pub async fn get_tunnel_metrics(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Path(id): Path<Uuid>,
    Query(q): Query<GetMetricsQuery>,
) -> Result<Json<TunnelMetricsResponse>, AppError> {
    let range = q.range.unwrap_or(TunnelMetricsRange::TwentyFourHours);
    let response = state.tunnel_service().get_metrics(id, range).await?;
    Ok(Json(response))
}

#[utoipa::path(
    get,
    path = "/api/tunnels/{id}/devices",
    tag = "tunnels",
    description = "List the devices currently routed through this tunnel — i.e. those \
                   whose routing rule has `target = Tunnel { tunnel_id: <id> }`. Admin only.",
    params(("id" = Uuid, Path, description = "Tunnel ID")),
    responses(
        (status = 200, description = "Devices using the tunnel", body = TunnelDevicesResponse),
        AuthErrors,
        NotFound,
    ),
    security(
        ("session_cookie" = []),
        ("bearer_auth" = []),
    ),
)]
pub async fn list_tunnel_devices(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Path(id): Path<Uuid>,
) -> Result<Json<TunnelDevicesResponse>, AppError> {
    let response = state.tunnel_service().list_tunnel_devices(id).await?;
    Ok(Json(response))
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

#[utoipa::path(
    post,
    path = "/api/tunnels/{id}/test",
    tag = "tunnels",
    description = "Run an exit-IP / country probe through this tunnel. The daemon \
                   brings the tunnel up if needed, waits for a fresh handshake, sends \
                   one HTTP probe through the tunnel interface, and reports the exit \
                   IP, ISO-3166 alpha-2 country code, and probe latency. Restores \
                   the prior up/down state when finished. Admin only.",
    params(("id" = Uuid, Path, description = "Tunnel ID")),
    responses(
        (status = 200, description = "Tunnel test result", body = TunnelTestResponse),
        AuthErrors,
        NotFound,
        Conflict,
        UpstreamUnavailable,
    ),
    security(
        ("session_cookie" = []),
        ("bearer_auth" = []),
    ),
)]
pub async fn test_tunnel(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Path(id): Path<Uuid>,
) -> Result<Json<TunnelTestResponse>, AppError> {
    let result = state.tunnel_service().test_tunnel(id).await?;
    Ok(Json(TunnelTestResponse { result }))
}

#[utoipa::path(
    put,
    path = "/api/tunnels/{id}/dns-override",
    tag = "tunnels",
    description = "Toggle the per-tunnel `override_default_dns` flag. When true, \
                   tunneled-device DNS queries are filtered by wardnet's ad-blocking \
                   pipeline and forwarded via a SO_BINDTODEVICE-bound socket on the \
                   tunnel interface. When false, those queries fall back to the \
                   system-wide upstream pool. Replaces the legacy nftables prerouting \
                   DNAT bypass — see issue #342. Admin only.",
    params(("id" = Uuid, Path, description = "Tunnel ID")),
    request_body = UpdateTunnelDnsOverrideRequest,
    responses(
        (status = 200, description = "DNS override updated", body = UpdateTunnelDnsOverrideResponse),
        AuthErrors,
        NotFound,
    ),
    security(
        ("session_cookie" = []),
        ("bearer_auth" = []),
    ),
)]
pub async fn update_tunnel_dns_override(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateTunnelDnsOverrideRequest>,
) -> Result<Json<UpdateTunnelDnsOverrideResponse>, AppError> {
    let tunnel = state
        .tunnel_service()
        .set_dns_override(id, body.override_default_dns)
        .await?;
    Ok(Json(UpdateTunnelDnsOverrideResponse { tunnel }))
}
