use axum::Json;
use axum::extract::State;
use wardnet_types::api::{
    DnsCacheFlushResponse, DnsConfigResponse, DnsStatusResponse, ToggleDnsRequest,
    UpdateDnsConfigRequest,
};

use crate::api::middleware::AdminAuth;
use crate::error::AppError;
use crate::state::AppState;

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
