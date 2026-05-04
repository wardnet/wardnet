//! Read-only handler for the post-upgrade migration runner's state file.
//!
//! End-to-end specs use this endpoint to verify the runner ran cleanly
//! at container boot — the file at `/var/lib/wardnet-postupgrade/state.json`
//! is root-owned and not exposed via the daemon's API, so we surface
//! it through the test agent (which runs as root inside the same
//! container).

use axum::Json;
use axum::http::StatusCode;
use axum::response::IntoResponse;

const STATE_PATH: &str = "/var/lib/wardnet-postupgrade/state.json";

/// `GET /postupgrade/state` — returns the parsed `state.json` as
/// JSON, or 404 if the file is absent (which would indicate the
/// runner never ran or the install path placed the directory at the
/// wrong location).
pub async fn get_state() -> impl IntoResponse {
    match tokio::fs::read(STATE_PATH).await {
        Ok(bytes) => match serde_json::from_slice::<serde_json::Value>(&bytes) {
            Ok(v) => (StatusCode::OK, Json(v)).into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("state.json is not valid JSON: {e}"),
            )
                .into_response(),
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            (StatusCode::NOT_FOUND, "state.json not found").into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to read state.json: {e}"),
        )
            .into_response(),
    }
}
