//! Handler for serving generated fixture files (`WireGuard` configs, keys, etc.)
//! from a local directory.

use std::path::PathBuf;
use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;

use crate::server::AppState;

/// Maximum allowed length for a fixture filename.
const MAX_FILENAME_LEN: usize = 64;

/// Returns `true` when the filename contains only safe characters:
/// ASCII alphanumeric, hyphen, underscore, and dot.
fn is_valid_filename(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MAX_FILENAME_LEN
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
}

/// `GET /fixtures/:name` -- reads a fixture file and returns it as plain text.
///
/// Returns 400 if the filename is invalid, 404 if the file does not exist.
pub async fn get_fixture(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    if !is_valid_filename(&name) {
        return (
            StatusCode::BAD_REQUEST,
            "invalid fixture name: only alphanumeric, hyphen, underscore, and dot are allowed \
             (max 64 chars)",
        )
            .into_response();
    }

    let path: PathBuf = state.fixtures_dir.join(&name);

    match tokio::fs::read_to_string(&path).await {
        Ok(content) => (StatusCode::OK, [("content-type", "text/plain")], content).into_response(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            (StatusCode::NOT_FOUND, "fixture not found").into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to read fixture: {e}"),
        )
            .into_response(),
    }
}
