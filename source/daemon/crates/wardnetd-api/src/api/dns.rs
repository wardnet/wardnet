use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;
use wardnet_common::api::{
    CreateAllowlistRequest, CreateAllowlistResponse, CreateBlocklistRequest,
    CreateBlocklistResponse, CreateFilterRuleRequest, CreateFilterRuleResponse,
    DeleteAllowlistResponse, DeleteBlocklistResponse, DeleteFilterRuleResponse,
    DnsCacheFlushResponse, DnsConfigResponse, DnsStatusResponse, ListAllowlistResponse,
    ListBlocklistsResponse, ListFilterRulesResponse, ToggleDnsRequest, UpdateBlocklistRequest,
    UpdateBlocklistResponse, UpdateDnsConfigRequest, UpdateFilterRuleRequest,
    UpdateFilterRuleResponse,
};
use wardnet_common::jobs::JobDispatchedResponse;

use crate::api::middleware::AdminAuth;
use crate::api::responses::{AuthErrors, BadRequest, NotFound};
use crate::state::AppState;
use wardnetd_services::error::AppError;

/// Register DNS routes onto the given [`OpenApiRouter`].
pub fn register(router: OpenApiRouter<AppState>) -> OpenApiRouter<AppState> {
    router
        .routes(routes!(get_config, update_config))
        .routes(routes!(toggle))
        .routes(routes!(status))
        .routes(routes!(flush_cache))
        .routes(routes!(list_blocklists, create_blocklist))
        .routes(routes!(update_blocklist, delete_blocklist))
        .routes(routes!(update_blocklist_now))
        .routes(routes!(list_allowlist, create_allowlist_entry))
        .routes(routes!(delete_allowlist_entry))
        .routes(routes!(list_filter_rules, create_filter_rule))
        .routes(routes!(update_filter_rule, delete_filter_rule))
}

#[utoipa::path(
    get,
    path = "/api/dns/config",
    tag = "dns",
    description = "Return the current DNS filter configuration (enabled flag, cache size, \
                   upstream resolvers). Admin only.",
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
    description = "Update the DNS filter configuration (cache size, upstream resolvers, \
                   and related options). Does not toggle the enabled flag — use the \
                   /api/dns/config/toggle endpoint for that. Admin only.",
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
    description = "Enable or disable the DNS filter server. In addition to persisting \
                   the enabled flag, this starts or stops the live DNS server process \
                   so the change takes effect immediately. Admin only.",
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

    if enabled {
        if let Err(e) = state.dns_server().start().await {
            tracing::error!(error = %e, "failed to start DNS server");
        }
    } else if let Err(e) = state.dns_server().stop().await {
        tracing::error!(error = %e, "failed to stop DNS server");
    }

    Ok(Json(response))
}

#[utoipa::path(
    get,
    path = "/api/dns/status",
    tag = "dns",
    description = "Return live DNS server status and cache statistics (running flag, \
                   cache size/capacity, cache hit rate). Admin only.",
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
    description = "Flush the DNS resolver cache, clearing every cached record so \
                   subsequent queries resolve against upstream. Returns the count of \
                   entries that were cleared. Admin only.",
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

// ---------------------------------------------------------------------------
// Blocklists
// ---------------------------------------------------------------------------

#[utoipa::path(
    get,
    path = "/api/dns/blocklists",
    tag = "dns",
    description = "List every configured DNS blocklist source, along with its enabled \
                   flag, last-updated timestamp, and entry count. Admin only.",
    responses(
        (status = 200, description = "List of blocklists", body = ListBlocklistsResponse),
        AuthErrors,
    ),
    security(
        ("session_cookie" = []),
        ("bearer_auth" = []),
    ),
)]
pub async fn list_blocklists(
    State(state): State<AppState>,
    _auth: AdminAuth,
) -> Result<Json<ListBlocklistsResponse>, AppError> {
    let response = state.dns_service().list_blocklists().await?;
    Ok(Json(response))
}

#[utoipa::path(
    post,
    path = "/api/dns/blocklists",
    tag = "dns",
    description = "Register a new DNS blocklist source (URL + format) that the daemon \
                   will periodically fetch and compile into the filter. Admin only.",
    request_body = CreateBlocklistRequest,
    responses(
        (status = 201, description = "Blocklist created", body = CreateBlocklistResponse),
        AuthErrors,
        BadRequest,
    ),
    security(
        ("session_cookie" = []),
        ("bearer_auth" = []),
    ),
)]
pub async fn create_blocklist(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Json(body): Json<CreateBlocklistRequest>,
) -> Result<(StatusCode, Json<CreateBlocklistResponse>), AppError> {
    let response = state.dns_service().create_blocklist(body).await?;
    Ok((StatusCode::CREATED, Json(response)))
}

#[utoipa::path(
    put,
    path = "/api/dns/blocklists/{id}",
    tag = "dns",
    description = "Update an existing DNS blocklist (URL, label, enabled flag, or \
                   refresh interval). Admin only.",
    params(("id" = Uuid, Path, description = "Blocklist ID")),
    request_body = UpdateBlocklistRequest,
    responses(
        (status = 200, description = "Updated blocklist", body = UpdateBlocklistResponse),
        AuthErrors,
        NotFound,
        BadRequest,
    ),
    security(
        ("session_cookie" = []),
        ("bearer_auth" = []),
    ),
)]
pub async fn update_blocklist(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateBlocklistRequest>,
) -> Result<Json<UpdateBlocklistResponse>, AppError> {
    let response = state.dns_service().update_blocklist(id, body).await?;
    Ok(Json(response))
}

#[utoipa::path(
    delete,
    path = "/api/dns/blocklists/{id}",
    tag = "dns",
    description = "Delete a DNS blocklist by ID. Any entries contributed by this \
                   blocklist are removed from the active filter. Admin only.",
    params(("id" = Uuid, Path, description = "Blocklist ID")),
    responses(
        (status = 200, description = "Blocklist deleted", body = DeleteBlocklistResponse),
        AuthErrors,
        NotFound,
    ),
    security(
        ("session_cookie" = []),
        ("bearer_auth" = []),
    ),
)]
pub async fn delete_blocklist(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Path(id): Path<Uuid>,
) -> Result<Json<DeleteBlocklistResponse>, AppError> {
    let response = state.dns_service().delete_blocklist(id).await?;
    Ok(Json(response))
}

#[utoipa::path(
    post,
    path = "/api/dns/blocklists/{id}/update",
    tag = "dns",
    description = "Trigger an immediate refresh of a blocklist. Dispatches a background \
                   job that fetches, parses, and stores the blocklist, then returns \
                   202 Accepted with the job id so the client can poll \
                   /api/jobs/{id} for progress. Admin only.",
    params(("id" = Uuid, Path, description = "Blocklist ID")),
    responses(
        (status = 202, description = "Background update job dispatched", body = JobDispatchedResponse),
        AuthErrors,
        NotFound,
    ),
    security(
        ("session_cookie" = []),
        ("bearer_auth" = []),
    ),
)]
pub async fn update_blocklist_now(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Path(id): Path<Uuid>,
) -> Result<(StatusCode, Json<JobDispatchedResponse>), AppError> {
    let response = state.dns_service().update_blocklist_now(id).await?;
    Ok((StatusCode::ACCEPTED, Json(response)))
}

// ---------------------------------------------------------------------------
// Allowlist
// ---------------------------------------------------------------------------

#[utoipa::path(
    get,
    path = "/api/dns/allowlist",
    tag = "dns",
    description = "List every entry in the DNS allowlist. Allowlisted domains bypass \
                   blocklist entries and always resolve. Admin only.",
    responses(
        (status = 200, description = "List of allowlist entries", body = ListAllowlistResponse),
        AuthErrors,
    ),
    security(
        ("session_cookie" = []),
        ("bearer_auth" = []),
    ),
)]
pub async fn list_allowlist(
    State(state): State<AppState>,
    _auth: AdminAuth,
) -> Result<Json<ListAllowlistResponse>, AppError> {
    let response = state.dns_service().list_allowlist().await?;
    Ok(Json(response))
}

#[utoipa::path(
    post,
    path = "/api/dns/allowlist",
    tag = "dns",
    description = "Add a domain to the DNS allowlist so it is never blocked, regardless \
                   of blocklist contents. Admin only.",
    request_body = CreateAllowlistRequest,
    responses(
        (status = 201, description = "Allowlist entry created", body = CreateAllowlistResponse),
        AuthErrors,
        BadRequest,
    ),
    security(
        ("session_cookie" = []),
        ("bearer_auth" = []),
    ),
)]
pub async fn create_allowlist_entry(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Json(body): Json<CreateAllowlistRequest>,
) -> Result<(StatusCode, Json<CreateAllowlistResponse>), AppError> {
    let response = state.dns_service().create_allowlist_entry(body).await?;
    Ok((StatusCode::CREATED, Json(response)))
}

#[utoipa::path(
    delete,
    path = "/api/dns/allowlist/{id}",
    tag = "dns",
    description = "Remove a domain from the DNS allowlist. If the domain is also \
                   listed by an active blocklist, it will start being blocked again. \
                   Admin only.",
    params(("id" = Uuid, Path, description = "Allowlist entry ID")),
    responses(
        (status = 200, description = "Allowlist entry deleted", body = DeleteAllowlistResponse),
        AuthErrors,
        NotFound,
    ),
    security(
        ("session_cookie" = []),
        ("bearer_auth" = []),
    ),
)]
pub async fn delete_allowlist_entry(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Path(id): Path<Uuid>,
) -> Result<Json<DeleteAllowlistResponse>, AppError> {
    let response = state.dns_service().delete_allowlist_entry(id).await?;
    Ok(Json(response))
}

// ---------------------------------------------------------------------------
// Custom filter rules
// ---------------------------------------------------------------------------

#[utoipa::path(
    get,
    path = "/api/dns/rules",
    tag = "dns",
    description = "List every custom DNS filter rule (allow, block, or rewrite) \
                   configured by the admin. Custom rules are evaluated alongside \
                   blocklists and allowlists. Admin only.",
    responses(
        (status = 200, description = "List of custom filter rules", body = ListFilterRulesResponse),
        AuthErrors,
    ),
    security(
        ("session_cookie" = []),
        ("bearer_auth" = []),
    ),
)]
pub async fn list_filter_rules(
    State(state): State<AppState>,
    _auth: AdminAuth,
) -> Result<Json<ListFilterRulesResponse>, AppError> {
    let response = state.dns_service().list_filter_rules().await?;
    Ok(Json(response))
}

#[utoipa::path(
    post,
    path = "/api/dns/rules",
    tag = "dns",
    description = "Create a custom DNS filter rule — a named allow/block/rewrite \
                   entry the admin maintains alongside the imported blocklists. \
                   Admin only.",
    request_body = CreateFilterRuleRequest,
    responses(
        (status = 201, description = "Filter rule created", body = CreateFilterRuleResponse),
        AuthErrors,
        BadRequest,
    ),
    security(
        ("session_cookie" = []),
        ("bearer_auth" = []),
    ),
)]
pub async fn create_filter_rule(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Json(body): Json<CreateFilterRuleRequest>,
) -> Result<(StatusCode, Json<CreateFilterRuleResponse>), AppError> {
    let response = state.dns_service().create_filter_rule(body).await?;
    Ok((StatusCode::CREATED, Json(response)))
}

#[utoipa::path(
    put,
    path = "/api/dns/rules/{id}",
    tag = "dns",
    description = "Update a custom DNS filter rule in place (pattern, action, enabled \
                   flag). Admin only.",
    params(("id" = Uuid, Path, description = "Filter rule ID")),
    request_body = UpdateFilterRuleRequest,
    responses(
        (status = 200, description = "Updated filter rule", body = UpdateFilterRuleResponse),
        AuthErrors,
        NotFound,
        BadRequest,
    ),
    security(
        ("session_cookie" = []),
        ("bearer_auth" = []),
    ),
)]
pub async fn update_filter_rule(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateFilterRuleRequest>,
) -> Result<Json<UpdateFilterRuleResponse>, AppError> {
    let response = state.dns_service().update_filter_rule(id, body).await?;
    Ok(Json(response))
}

#[utoipa::path(
    delete,
    path = "/api/dns/rules/{id}",
    tag = "dns",
    description = "Delete a custom DNS filter rule by ID. Admin only.",
    params(("id" = Uuid, Path, description = "Filter rule ID")),
    responses(
        (status = 200, description = "Filter rule deleted", body = DeleteFilterRuleResponse),
        AuthErrors,
        NotFound,
    ),
    security(
        ("session_cookie" = []),
        ("bearer_auth" = []),
    ),
)]
pub async fn delete_filter_rule(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Path(id): Path<Uuid>,
) -> Result<Json<DeleteFilterRuleResponse>, AppError> {
    let response = state.dns_service().delete_filter_rule(id).await?;
    Ok(Json(response))
}
