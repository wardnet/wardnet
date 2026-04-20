use axum::Json;
use axum::extract::{Path, State};
use uuid::Uuid;
use wardnet_common::jobs::Job;

use crate::api::middleware::AdminAuth;
use crate::state::AppState;
use wardnetd_services::error::AppError;

/// GET /api/jobs/{id} — poll the status of a background job. Returns 404 when
/// the job id is unknown (either never dispatched or GC'd after its TTL).
pub async fn get_job(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Path(id): Path<Uuid>,
) -> Result<Json<Job>, AppError> {
    state
        .job_service()
        .get(id)
        .await
        .map(Json)
        .ok_or_else(|| AppError::NotFound(format!("job {id} not found")))
}
