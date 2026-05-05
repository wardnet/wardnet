//! Read-only handlers for the Safe Reboot / Safe Shutdown e2e suite.
//!
//! Three concerns:
//!
//! 1. Surface the contents of the `systemctl` invocation log written
//!    by the test image's shim, so the spec can assert that the
//!    daemon's spawned task actually called `systemctl reboot
//!    --no-block` / `systemctl poweroff --no-block`.
//! 2. Surface the contents of the polkit rule file written by the
//!    `0001_polkit_power_rule` migration, so the spec can verify the
//!    install put it on disk with the expected payload.
//! 3. Run `pkcheck` as the `wardnet` user — the closest we can get
//!    to "polkit accepts the rule" without rebooting CI.
//!
//! All three only run inside the test image. Production daemons
//! never ship the test agent.

use axum::Json;
use axum::extract::Query;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use serde::{Deserialize, Serialize};
use tokio::process::Command;

/// Path the test image's `systemctl` shim writes to. Matches the
/// `Dockerfile.test` shim — keep in sync.
const SYSTEMCTL_INVOCATIONS_LOG: &str = "/var/lib/wardnet/systemctl-invocations.log";

/// Where the `0001_polkit_power_rule` migration drops the rule.
const POLKIT_RULE_PATH: &str = "/etc/polkit-1/rules.d/50-wardnet-power.rules";

#[derive(Debug, Serialize)]
pub struct FileContentsResponse {
    pub path: &'static str,
    pub exists: bool,
    pub contents: Option<String>,
}

/// `GET /power/systemctl-invocations` — returns each line written
/// by the shim. Returns `exists: false` if the daemon never invoked
/// the shim (i.e. before the first reboot/shutdown POST), so specs
/// can poll deterministically without 404 noise.
pub async fn get_systemctl_invocations() -> impl IntoResponse {
    read_optional_file(SYSTEMCTL_INVOCATIONS_LOG).await
}

/// `GET /power/polkit-rule` — returns the on-disk rule content if
/// the migration has run.
pub async fn get_polkit_rule() -> impl IntoResponse {
    read_optional_file(POLKIT_RULE_PATH).await
}

async fn read_optional_file(path: &'static str) -> impl IntoResponse {
    match tokio::fs::read_to_string(path).await {
        Ok(contents) => Json(FileContentsResponse {
            path,
            exists: true,
            contents: Some(contents),
        })
        .into_response(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Json(FileContentsResponse {
            path,
            exists: false,
            contents: None,
        })
        .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to read {path}: {e}"),
        )
            .into_response(),
    }
}

#[derive(Debug, Deserialize)]
pub struct PkcheckQuery {
    pub action_id: String,
}

#[derive(Debug, Serialize)]
pub struct PkcheckResponse {
    pub action_id: String,
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

/// `GET /power/pkcheck?action_id=...` — runs
/// `pkcheck --action-id <id> --process $$ --allow-user-interaction`
/// as the `wardnet` user. The spec asserts `exit_code == 0` for the
/// four login1 actions the polkit rule grants.
pub async fn get_pkcheck(Query(q): Query<PkcheckQuery>) -> impl IntoResponse {
    // Reject obviously bogus inputs before invoking pkcheck. Action
    // ids are dotted ASCII; anything else is almost certainly a
    // typo or injection attempt.
    if !is_valid_action_id(&q.action_id) {
        return (
            StatusCode::BAD_REQUEST,
            format!("invalid action_id: {}", q.action_id),
        )
            .into_response();
    }

    // `runuser` keeps us in the same cgroup as the agent so the
    // spawned pkcheck has a real UID context. `--allow-user-interaction`
    // matches what a regular GUI consumer would pass; without it
    // pkcheck refuses authorisation that requires admin auth, which
    // would mask the polkit-rule grant we want to verify.
    //
    // `sh -c 'pkcheck --process $$'` resolves $$ inside the runuser
    // shell so the subject is the just-spawned process — pkcheck
    // then reads /proc/$$/status and confirms its uid is `wardnet`.
    let output = match Command::new("runuser")
        .args([
            "-u",
            "wardnet",
            "--",
            "sh",
            "-c",
            &format!(
                "pkcheck --action-id {} --process $$ --allow-user-interaction",
                shell_escape(&q.action_id)
            ),
        ])
        .output()
        .await
    {
        Ok(o) => o,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed to spawn pkcheck: {e}"),
            )
                .into_response();
        }
    };

    Json(PkcheckResponse {
        action_id: q.action_id,
        exit_code: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
    .into_response()
}

/// Allow only `[A-Za-z0-9._-]` action ids — covers every
/// `org.freedesktop.login1.*` value polkit ships with.
fn is_valid_action_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-')
}

/// Wrap a string in single quotes for shell consumption. Inputs are
/// already validated by [`is_valid_action_id`] so this is belt-and-
/// suspenders, but cheap to apply.
fn shell_escape(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}
