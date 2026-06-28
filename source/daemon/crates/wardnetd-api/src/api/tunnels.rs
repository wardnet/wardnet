use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;
use wardnet_common::api::{
    CreateTunnelRequest, CreateTunnelResponse, DeleteTunnelResponse, ListTunnelsResponse,
    RebuildTunnelResponse, TunnelDetailResponse, TunnelDevicesResponse, TunnelTestResponse,
    UpdateTunnelDnsOverrideRequest, UpdateTunnelDnsOverrideResponse,
};
use wardnet_common::jobs::JobDispatchedResponse;
use wardnet_common::speed_test::TunnelSpeedTestHistoryResponse;

use crate::api::middleware::AdminAuth;
use crate::api::responses::{AuthErrors, BadRequest, Conflict, NotFound, UpstreamUnavailable};
use crate::state::AppState;
use wardnetd_services::error::AppError;

/// Register tunnel routes onto the given [`OpenApiRouter`].
pub fn register(router: OpenApiRouter<AppState>) -> OpenApiRouter<AppState> {
    router
        .routes(routes!(list_tunnels, create_tunnel))
        .routes(routes!(get_tunnel, delete_tunnel))
        .routes(routes!(list_tunnel_devices))
        .routes(routes!(test_tunnel))
        .routes(routes!(start_speed_test))
        .routes(routes!(list_speed_tests))
        .routes(routes!(update_tunnel_dns_override))
        .routes(routes!(rebuild_tunnel))
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
    post,
    path = "/api/tunnels/{id}/speed-test",
    tag = "tunnels",
    description = "Start a speed test for this tunnel. Returns immediately with a job \
                   id (202); the background job measures throughput, latency and jitter \
                   twice — once over the direct (WAN) path and once through the tunnel — \
                   so the result shows how much of the line the VPN preserves. If the \
                   tunnel is down it is brought up for the run and torn back down after. \
                   Poll `GET /api/jobs/{job_id}` for progress. A concurrent run (or an \
                   overlapping test) on the same tunnel returns 409. Admin only.",
    params(("id" = Uuid, Path, description = "Tunnel ID")),
    responses(
        (status = 202, description = "Speed test job dispatched", body = JobDispatchedResponse),
        AuthErrors,
        NotFound,
        Conflict,
    ),
    security(
        ("session_cookie" = []),
        ("bearer_auth" = []),
    ),
)]
pub async fn start_speed_test(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Path(id): Path<Uuid>,
) -> Result<(StatusCode, Json<JobDispatchedResponse>), AppError> {
    let response = state.tunnel_service_arc().start_speed_test(id).await?;
    Ok((StatusCode::ACCEPTED, Json(response)))
}

#[utoipa::path(
    get,
    path = "/api/tunnels/{id}/speed-test/results",
    tag = "tunnels",
    description = "List recent speed test results for this tunnel, newest first \
                   (last few runs). Each result carries both the direct (WAN) and \
                   tunnel measurements for throughput, latency and jitter. Admin only.",
    params(("id" = Uuid, Path, description = "Tunnel ID")),
    responses(
        (status = 200, description = "Speed test history", body = TunnelSpeedTestHistoryResponse),
        AuthErrors,
        NotFound,
    ),
    security(
        ("session_cookie" = []),
        ("bearer_auth" = []),
    ),
)]
pub async fn list_speed_tests(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Path(id): Path<Uuid>,
) -> Result<Json<TunnelSpeedTestHistoryResponse>, AppError> {
    let response = state.tunnel_service().list_speed_tests(id).await?;
    Ok(Json(response))
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

#[utoipa::path(
    post,
    path = "/api/tunnels/{id}/rebuild",
    tag = "tunnels",
    description = "Tear down the WireGuard interface and re-apply it from the persisted \
                   config. Use when a tunnel is stale or devices cannot route through it. \
                   Equivalent to the internal watchdog recovery path. Admin only.",
    params(("id" = Uuid, Path, description = "Tunnel ID")),
    responses(
        (status = 200, description = "Tunnel rebuild initiated", body = RebuildTunnelResponse),
        AuthErrors,
        NotFound,
    ),
    security(
        ("session_cookie" = []),
        ("bearer_auth" = []),
    ),
)]
pub async fn rebuild_tunnel(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Path(id): Path<Uuid>,
) -> Result<Json<RebuildTunnelResponse>, AppError> {
    state.tunnel_service().rebuild(id).await?;
    Ok(Json(RebuildTunnelResponse { ok: true }))
}
