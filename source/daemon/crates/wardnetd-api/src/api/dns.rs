use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
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
use crate::state::AppState;
use wardnetd_services::error::AppError;

/// GET /api/dns/config
pub async fn get_config(
    State(state): State<AppState>,
    _auth: AdminAuth,
) -> Result<Json<DnsConfigResponse>, AppError> {
    let response = state.dns_service().get_config().await?;
    Ok(Json(response))
}

/// PUT /api/dns/config
pub async fn update_config(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Json(body): Json<UpdateDnsConfigRequest>,
) -> Result<Json<DnsConfigResponse>, AppError> {
    let response = state.dns_service().update_config(body).await?;
    Ok(Json(response))
}

/// POST /api/dns/config/toggle
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

/// GET /api/dns/status
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

/// POST /api/dns/cache/flush
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

/// GET /api/dns/blocklists
pub async fn list_blocklists(
    State(state): State<AppState>,
    _auth: AdminAuth,
) -> Result<Json<ListBlocklistsResponse>, AppError> {
    let response = state.dns_service().list_blocklists().await?;
    Ok(Json(response))
}

/// POST /api/dns/blocklists
pub async fn create_blocklist(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Json(body): Json<CreateBlocklistRequest>,
) -> Result<(StatusCode, Json<CreateBlocklistResponse>), AppError> {
    let response = state.dns_service().create_blocklist(body).await?;
    Ok((StatusCode::CREATED, Json(response)))
}

/// PUT /api/dns/blocklists/{id}
pub async fn update_blocklist(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateBlocklistRequest>,
) -> Result<Json<UpdateBlocklistResponse>, AppError> {
    let response = state.dns_service().update_blocklist(id, body).await?;
    Ok(Json(response))
}

/// DELETE /api/dns/blocklists/{id}
pub async fn delete_blocklist(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Path(id): Path<Uuid>,
) -> Result<Json<DeleteBlocklistResponse>, AppError> {
    let response = state.dns_service().delete_blocklist(id).await?;
    Ok(Json(response))
}

/// POST /api/dns/blocklists/{id}/update — dispatches a background job that
/// fetches, parses, and stores the blocklist. Returns 202 Accepted with the
/// job id so the client can poll `GET /api/jobs/:id` for progress.
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

/// GET /api/dns/allowlist
pub async fn list_allowlist(
    State(state): State<AppState>,
    _auth: AdminAuth,
) -> Result<Json<ListAllowlistResponse>, AppError> {
    let response = state.dns_service().list_allowlist().await?;
    Ok(Json(response))
}

/// POST /api/dns/allowlist
pub async fn create_allowlist_entry(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Json(body): Json<CreateAllowlistRequest>,
) -> Result<(StatusCode, Json<CreateAllowlistResponse>), AppError> {
    let response = state.dns_service().create_allowlist_entry(body).await?;
    Ok((StatusCode::CREATED, Json(response)))
}

/// DELETE /api/dns/allowlist/{id}
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

/// GET /api/dns/rules
pub async fn list_filter_rules(
    State(state): State<AppState>,
    _auth: AdminAuth,
) -> Result<Json<ListFilterRulesResponse>, AppError> {
    let response = state.dns_service().list_filter_rules().await?;
    Ok(Json(response))
}

/// POST /api/dns/rules
pub async fn create_filter_rule(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Json(body): Json<CreateFilterRuleRequest>,
) -> Result<(StatusCode, Json<CreateFilterRuleResponse>), AppError> {
    let response = state.dns_service().create_filter_rule(body).await?;
    Ok((StatusCode::CREATED, Json(response)))
}

/// PUT /api/dns/rules/{id}
pub async fn update_filter_rule(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateFilterRuleRequest>,
) -> Result<Json<UpdateFilterRuleResponse>, AppError> {
    let response = state.dns_service().update_filter_rule(id, body).await?;
    Ok(Json(response))
}

/// DELETE /api/dns/rules/{id}
pub async fn delete_filter_rule(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Path(id): Path<Uuid>,
) -> Result<Json<DeleteFilterRuleResponse>, AppError> {
    let response = state.dns_service().delete_filter_rule(id).await?;
    Ok(Json(response))
}
