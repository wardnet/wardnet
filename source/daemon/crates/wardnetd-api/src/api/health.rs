//! Unauthenticated liveness/readiness endpoint `GET /health` (issue #214).
//!
//! Actuator/k8s convention: **200** when overall health is UP, **503** when
//! DOWN, with a per-component breakdown in the body. This is a deliberate,
//! documented exception to the require-auth rule (same shape as
//! `GET /api/setup/status`): the probe carries no sensitive data and must be
//! reachable by load balancers / uptime checks without a session. It does
//! **not** call `require_*` and is annotated `security(())`.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use wardnet_common::api::{HealthComponentDto, HealthResponse, HealthStatusDto};
use wardnetd_services::HealthStatus;

use crate::state::AppState;

/// Register the health route onto the given [`OpenApiRouter`]. The path is
/// top-level `/health`, not under `/api`, to match the ubiquitous probe
/// convention.
pub fn register(router: OpenApiRouter<AppState>) -> OpenApiRouter<AppState> {
    router.routes(routes!(health))
}

#[utoipa::path(
    get,
    path = "/health",
    tag = "health",
    description = "Liveness/readiness probe. Returns 200 with status `UP` when \
                   every registered health check passes (after debounce), or \
                   503 with status `DOWN` when any component is down. \
                   Unauthenticated by design — reachable by load balancers and \
                   uptime monitors without a session. See issue #214.",
    responses(
        (status = 200, description = "Daemon healthy", body = HealthResponse),
        (status = 503, description = "Daemon unhealthy", body = HealthResponse),
    ),
    security(()),
)]
pub async fn health(State(state): State<AppState>) -> (StatusCode, Json<HealthResponse>) {
    let snapshot = state.health_monitor().snapshot();

    let components = snapshot
        .components
        .iter()
        .map(|c| HealthComponentDto {
            name: c.name.clone(),
            status: to_dto(c.status),
            detail: c.detail.clone(),
        })
        .collect();

    let status = to_dto(snapshot.overall);
    let code = match status {
        HealthStatusDto::Up => StatusCode::OK,
        HealthStatusDto::Down => StatusCode::SERVICE_UNAVAILABLE,
    };

    (code, Json(HealthResponse { status, components }))
}

/// Map the service-layer [`HealthStatus`] onto the wire DTO.
fn to_dto(status: HealthStatus) -> HealthStatusDto {
    match status {
        HealthStatus::Up => HealthStatusDto::Up,
        HealthStatus::Down => HealthStatusDto::Down,
    }
}
