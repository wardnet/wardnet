use axum::Json;
use axum::extract::State;
use wardnet_common::api::InfoResponse;

use crate::state::AppState;

/// GET /api/info
///
/// Thin handler — returns the daemon version and uptime.
/// No authentication required. Used by the web UI connection status widget.
pub async fn info(State(state): State<AppState>) -> Json<InfoResponse> {
    let uptime = state.system_service().uptime();
    Json(InfoResponse {
        version: state.system_service().version().to_owned(),
        uptime_seconds: uptime.as_secs(),
    })
}
