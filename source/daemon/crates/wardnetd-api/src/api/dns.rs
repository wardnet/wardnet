//! DNS server config + status + cache + query log handlers.
//!
//! Filter-source CRUD lives in [`super::dns_filter`] after issue #221.
//! DNS stats moved to the generic `/api/stats` endpoints (issue #409).

use axum::Json;
use axum::extract::{Query, State};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use wardnet_common::api::{
    DnsCacheFlushResponse, DnsConfigResponse, DnsStatusResponse, ListQueryLogParams,
    ListQueryLogResponse, ToggleDnsRequest, UpdateDnsConfigRequest,
};

use crate::api::middleware::AdminAuth;
use crate::api::responses::{AuthErrors, BadRequest};
use crate::state::AppState;
use wardnetd_services::error::AppError;

pub fn register(router: OpenApiRouter<AppState>) -> OpenApiRouter<AppState> {
    router
        .routes(routes!(get_config, update_config))
        .routes(routes!(toggle))
        .routes(routes!(status))
        .routes(routes!(flush_cache))
        .routes(routes!(list_query_log))
}

#[utoipa::path(
    get,
    path = "/api/dns/config",
    tag = "dns",
    description = "Return the current DNS server configuration. Admin only.",
    responses(
        (status = 200, description = "Current DNS configuration", body = DnsConfigResponse),
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
) -> Result<Json<DnsConfigResponse>, AppError> {
    let response = state.dns_service().get_config().await?;
    Ok(Json(response))
}

#[utoipa::path(
    put,
    path = "/api/dns/config",
    tag = "dns",
    description = "Partially update DNS server configuration. Admin only.",
    request_body = UpdateDnsConfigRequest,
    responses(
        (status = 200, description = "Updated DNS configuration", body = DnsConfigResponse),
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
    Json(body): Json<UpdateDnsConfigRequest>,
) -> Result<Json<DnsConfigResponse>, AppError> {
    let response = state.dns_service().update_config(body).await?;
    Ok(Json(response))
}

#[utoipa::path(
    post,
    path = "/api/dns/config/toggle",
    tag = "dns",
    description = "Enable or disable the DNS server. Admin only.",
    request_body = ToggleDnsRequest,
    responses(
        (status = 200, description = "Updated DNS configuration", body = DnsConfigResponse),
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
    Json(body): Json<ToggleDnsRequest>,
) -> Result<Json<DnsConfigResponse>, AppError> {
    let enabled = body.enabled;
    let response = state.dns_service().toggle(body).await?;

    let transition = if enabled {
        state.dns_server().start().await
    } else {
        state.dns_server().stop().await
    };
    if let Err(e) = transition {
        if let Err(revert_err) = state
            .dns_service()
            .toggle(ToggleDnsRequest { enabled: !enabled })
            .await
        {
            tracing::error!(error = %revert_err, "failed to revert DNS config after transition failure");
        }
        return Err(e.into());
    }

    Ok(Json(response))
}

#[utoipa::path(
    get,
    path = "/api/dns/status",
    tag = "dns",
    description = "Return live DNS server status and cache statistics. Admin only.",
    responses(
        (status = 200, description = "DNS server status and cache stats", body = DnsStatusResponse),
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
) -> Result<Json<DnsStatusResponse>, AppError> {
    let config = state.dns_service().get_dns_config().await?;
    let server = state.dns_server();
    Ok(Json(DnsStatusResponse {
        enabled: config.enabled,
        running: server.is_running(),
        cache_size: server.cache_size().await,
        cache_capacity: config.cache_size,
        cache_hit_rate: server.cache_hit_rate().await,
    }))
}

#[utoipa::path(
    post,
    path = "/api/dns/cache/flush",
    tag = "dns",
    description = "Flush the DNS resolver cache. Admin only.",
    responses(
        (status = 200, description = "Cache flushed", body = DnsCacheFlushResponse),
        AuthErrors,
    ),
    security(
        ("session_cookie" = []),
        ("bearer_auth" = []),
    ),
)]
pub async fn flush_cache(
    State(state): State<AppState>,
    _auth: AdminAuth,
) -> Result<Json<DnsCacheFlushResponse>, AppError> {
    let cleared = state.dns_server().flush_cache().await;
    Ok(Json(DnsCacheFlushResponse {
        message: "Cache flushed".to_owned(),
        entries_cleared: cleared,
    }))
}

#[utoipa::path(
    get,
    path = "/api/dns/log",
    tag = "dns",
    description = "Paginated DNS query log. Admin only.",
    params(ListQueryLogParams),
    responses(
        (status = 200, description = "Page of query log entries", body = ListQueryLogResponse),
        AuthErrors,
        BadRequest,
    ),
    security(
        ("session_cookie" = []),
        ("bearer_auth" = []),
    ),
)]
pub async fn list_query_log(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Query(params): Query<ListQueryLogParams>,
) -> Result<Json<ListQueryLogResponse>, AppError> {
    let response = state.dns_service().list_query_log(params).await?;
    Ok(Json(response))
}
