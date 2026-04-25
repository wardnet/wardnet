//! Handler for the `GET /pid` endpoint -- reads the daemon pidfile and reports
//! whether the process is alive.

use std::path::Path;
use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use tracing::warn;

use crate::server::AppState;
use crate::server::models::{ErrorResponse, PidResponse};

/// `GET /pid` -- returns `{ pid, running }` from the daemon pidfile.
///
/// Returns 404 if the pidfile is missing, 500 on read errors, 400 if the file
/// content is not a valid PID.
pub async fn get_pid(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let raw = match tokio::fs::read_to_string(&state.pidfile_path).await {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: format!("pidfile not found: {}", state.pidfile_path.display()),
                }),
            )
                .into_response();
        }
        Err(e) => {
            warn!(error = %e, path = %state.pidfile_path.display(), "failed to read pidfile");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("failed to read pidfile: {e}"),
                }),
            )
                .into_response();
        }
    };

    let pid: i32 = match raw.trim().parse() {
        Ok(p) => p,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: format!("pidfile content is not a valid pid: {:?}", raw.trim()),
                }),
            )
                .into_response();
        }
    };

    let running = Path::new(&format!("/proc/{pid}")).exists();

    Json(PidResponse { pid, running }).into_response()
}
