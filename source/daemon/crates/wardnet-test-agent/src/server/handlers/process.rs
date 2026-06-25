//! Process-control handler for the watchdog end-to-end test (issue #214).
//!
//! `POST /process/signal` reads the daemon pidfile and delivers a
//! **whitelisted** signal (`STOP` / `CONT`) to the `wardnetd` process. The
//! agent runs in the same container — hence the same PID namespace — as the
//! daemon, so a direct `kill(2)` syscall reaches it; no container runtime or
//! socket is involved. We signal in-process rather than shelling out to a
//! `kill` binary, which the minimal daemon container does not ship.
//!
//! This exists purely so the e2e suite can *freeze* the daemon (SIGSTOP) and
//! verify that systemd's `Type=notify` + `WatchdogSec=15` supervision restarts
//! it — i.e. the soft watchdog's transport. Production daemons never ship the
//! test agent. The signal whitelist keeps this from being an arbitrary-signal
//! injection primitive.

use std::path::Path;
use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use tracing::{info, warn};

use crate::server::AppState;
use crate::server::models::{ErrorResponse, ProcessSignalRequest, ProcessSignalResponse};

/// Signals the endpoint is allowed to deliver, paired with their `libc`
/// numbers. `STOP` freezes the daemon (the watchdog test); `CONT` resumes it
/// (cleanup / symmetry).
const ALLOWED_SIGNALS: &[(&str, libc::c_int)] = &[("STOP", libc::SIGSTOP), ("CONT", libc::SIGCONT)];

/// `POST /process/signal` — deliver `{ signal }` to the daemon PID.
pub async fn post_process_signal(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ProcessSignalRequest>,
) -> impl IntoResponse {
    let signal = req.signal.trim().to_uppercase();
    let Some(&(_, signum)) = ALLOWED_SIGNALS.iter().find(|(name, _)| *name == signal) else {
        let allowed: Vec<&str> = ALLOWED_SIGNALS.iter().map(|(name, _)| *name).collect();
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: format!("signal must be one of {allowed:?}, got {:?}", req.signal),
            }),
        )
            .into_response();
    };

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

    if !Path::new(&format!("/proc/{pid}")).exists() {
        return (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: format!("daemon process {pid} is not running"),
            }),
        )
            .into_response();
    }

    info!(pid, signal = %signal, "delivering signal to daemon");
    // SAFETY: `kill(2)` is a plain syscall with no memory-safety
    // preconditions; `pid` is validated above and `signum` is a whitelisted
    // constant. A non-zero return means the kernel rejected the call.
    let rc = unsafe { libc::kill(pid as libc::pid_t, signum) };
    if rc == 0 {
        Json(ProcessSignalResponse {
            pid,
            signal,
            delivered: true,
        })
        .into_response()
    } else {
        let err = std::io::Error::last_os_error();
        warn!(pid, signal = %signal, error = %err, "kill syscall failed");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("kill({pid}, SIG{signal}) failed: {err}"),
            }),
        )
            .into_response()
    }
}
