//! Handler for executing commands inside containers via `podman exec` (or an
//! alternative runtime set through the `CONTAINER_RUNTIME` environment variable).

use axum::Json;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use tokio::process::Command;
use tracing::{info, warn};

use crate::models::{ContainerExecRequest, ContainerExecResponse, ErrorResponse};

/// Required prefix for container names. Rejects names that don't start with
/// this value to prevent executing commands in arbitrary containers.
const CONTAINER_NAME_PREFIX: &str = "wardnet_";

/// Maximum combined length of the command arguments (basic abuse prevention).
const MAX_COMMAND_LEN: usize = 4096;

/// Returns the container runtime binary. Defaults to `podman` but respects the
/// `CONTAINER_RUNTIME` environment variable (e.g. `docker`).
fn container_runtime() -> String {
    std::env::var("CONTAINER_RUNTIME").unwrap_or_else(|_| "podman".to_owned())
}

/// Validates that a container name starts with the required prefix and contains
/// only safe characters (alphanumeric, underscore, hyphen).
fn is_valid_container_name(name: &str) -> bool {
    name.starts_with(CONTAINER_NAME_PREFIX)
        && !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// `POST /container/exec` -- executes a command inside a named container.
pub async fn post_container_exec(Json(req): Json<ContainerExecRequest>) -> impl IntoResponse {
    // Validate container name.
    if !is_valid_container_name(&req.container) {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: format!(
                    "container name must start with \"{CONTAINER_NAME_PREFIX}\" and contain only \
                     alphanumeric characters, underscores, or hyphens"
                ),
            }),
        )
            .into_response();
    }

    // Validate command is not empty.
    if req.command.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "command must not be empty".to_owned(),
            }),
        )
            .into_response();
    }

    // Basic length guard.
    let total_len: usize = req.command.iter().map(String::len).sum();
    if total_len > MAX_COMMAND_LEN {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: format!("command too long ({total_len} bytes, max {MAX_COMMAND_LEN})"),
            }),
        )
            .into_response();
    }

    let runtime = container_runtime();

    info!(
        runtime,
        container = %req.container,
        command = ?req.command,
        "executing container command"
    );

    let output = match Command::new(&runtime)
        .arg("exec")
        .arg(&req.container)
        .args(&req.command)
        .output()
        .await
    {
        Ok(o) => o,
        Err(e) => {
            warn!(error = %e, runtime, "failed to run container exec");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("failed to run {runtime} exec: {e}"),
                }),
            )
                .into_response();
        }
    };

    let exit_code = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();

    Json(ContainerExecResponse {
        exit_code,
        stdout,
        stderr,
    })
    .into_response()
}
