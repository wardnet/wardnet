use axum::Json;
use axum::body::Body;
use axum::extract::State;
use axum::http::header;
use axum::response::IntoResponse;
use serde::Serialize;
use wardnet_types::api::SystemStatusResponse;

use crate::api::middleware::AdminAuth;
use crate::error::AppError;
use crate::recent_errors::RecentError;
use crate::state::AppState;

/// GET /api/system/status
///
/// Thin handler — returns system status (version, uptime, counts).
/// Requires admin authentication via session cookie or API key.
pub async fn status(
    State(state): State<AppState>,
    _auth: AdminAuth,
) -> Result<Json<SystemStatusResponse>, AppError> {
    let response = state.system_service().status().await?;
    Ok(Json(response))
}

/// Format a JSON log line into human-readable text.
fn format_log_line(line: &str) -> String {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
        return line.to_string();
    };
    let obj = v.as_object();
    let timestamp = obj
        .and_then(|o| o.get("timestamp"))
        .and_then(|t| t.as_str())
        .unwrap_or("");
    let level = obj
        .and_then(|o| o.get("level"))
        .and_then(|l| l.as_str())
        .unwrap_or("INFO");
    let target = obj
        .and_then(|o| o.get("target"))
        .and_then(|t| t.as_str())
        .unwrap_or("");
    let message = obj
        .and_then(|o| o.get("fields"))
        .and_then(|f| f.get("message"))
        .and_then(|m| m.as_str())
        .unwrap_or("");

    format!("{timestamp} {level:>5} {target} {message}")
}

/// Find the current log file. The rolling appender creates dated files like
/// `wardnetd.log.2026-04-12` in the same directory as the configured path.
/// Try the exact path first, then look for the most recent dated file.
async fn find_current_log(configured_path: &std::path::Path) -> Result<String, AppError> {
    // Try the exact configured path first.
    if let Ok(content) = tokio::fs::read_to_string(configured_path).await {
        return Ok(content);
    }

    // Look for dated files in the same directory with the same prefix.
    let dir = configured_path
        .parent()
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("invalid log path")))?;
    let prefix = configured_path
        .file_name()
        .and_then(|f| f.to_str())
        .unwrap_or("wardnetd.log");

    let mut entries = tokio::fs::read_dir(dir)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("failed to read log directory: {e}")))?;

    let mut candidates = Vec::new();
    while let Ok(Some(entry)) = entries.next_entry().await {
        if let Some(name) = entry.file_name().to_str()
            && name.starts_with(prefix)
        {
            candidates.push(entry.path());
        }
    }

    // Sort by name descending — most recent date suffix comes last alphabetically.
    candidates.sort();
    if let Some(latest) = candidates.last() {
        return tokio::fs::read_to_string(latest)
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("failed to read log file: {e}")));
    }

    Err(AppError::Internal(anyhow::anyhow!(
        "no log files found at {}",
        configured_path.display()
    )))
}

/// GET /api/system/logs/download
///
/// Downloads the full log file as human-readable text.
/// Requires admin authentication.
pub async fn download_logs(
    State(state): State<AppState>,
    _auth: AdminAuth,
) -> Result<impl IntoResponse, AppError> {
    let log_path = &state.config().logging.path;
    let content = find_current_log(log_path).await?;

    let formatted: String = content
        .lines()
        .map(format_log_line)
        .collect::<Vec<_>>()
        .join("\n");

    Ok((
        [
            (header::CONTENT_TYPE, "text/plain; charset=utf-8"),
            (
                header::CONTENT_DISPOSITION,
                "attachment; filename=\"wardnetd.log\"",
            ),
        ],
        Body::from(formatted),
    ))
}

/// Response for GET /api/system/errors.
#[derive(Debug, Serialize)]
pub struct RecentErrorsResponse {
    pub errors: Vec<RecentError>,
}

/// GET /api/system/errors
///
/// Returns the last 15 warnings and errors from the in-memory ring buffer.
/// Requires admin authentication.
pub async fn recent_errors(
    State(state): State<AppState>,
    _auth: AdminAuth,
) -> Json<RecentErrorsResponse> {
    let errors = state.recent_errors().get_recent();
    Json(RecentErrorsResponse { errors })
}
