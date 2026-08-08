//! Handlers for the auto-update binary-swap e2e (issue #319).
//!
//! Two jobs the daemon's own API cannot do:
//!
//! 1. **Observe the swap.** `/usr/local/bin/wardnetd`, its `.old`
//!    sibling, and the staged pending tarball are all outside anything
//!    `/api/*` exposes. Digesting them here is what turns "the daemon
//!    reports a new version" into "the binary on disk actually
//!    changed".
//! 2. **Drive the negative case, and recover from it.** The
//!    trust-anchor spec deliberately wedges `wardnetd.service`: the
//!    post-upgrade runner exits nonzero, so systemd refuses to start
//!    the daemon and the daemon's API is gone. Staging the corrupt
//!    pair, restarting the unit, and clearing up afterwards therefore
//!    have to come from a process that is *not* wardnetd. The test
//!    agent is that process — a separate unit, running as root, in the
//!    same container.
//!
//! Why the corrupt pair is staged directly rather than served: the
//! daemon and the runner are built against the *same* ephemeral key in
//! this image, so a wrongly-signed tarball fetched over HTTP would be
//! rejected by the daemon's own pre-stage `verify_signature` and never
//! reach the runner. Writing it straight to the staging directory is
//! the honest expression of the threat model `swap.rs` documents — the
//! wardnet user can stage any bytes it likes, and only the runner's
//! embedded key decides what actually runs.
//!
//! All of this only exists in the test image; production daemons never
//! ship the test agent.

use std::path::Path;

use axum::Json;
use axum::extract::Query;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::process::Command;
use tracing::{info, warn};

/// The live daemon binary. Mirrors `UpdateConfig::live_binary_path`'s
/// default and `wardnet_postupgrade_runner::DEFAULT_LIVE_BINARY_PATH`.
const LIVE_BINARY: &str = "/usr/local/bin/wardnetd";
/// Previous binary preserved by the runner's swap, and the source the
/// rollback path renames back over [`LIVE_BINARY`].
const OLD_BINARY: &str = "/usr/local/bin/wardnetd.old";

/// Staging directory the daemon writes pending artifacts into.
/// Mirrors `UpdateConfig::staging_dir`'s default.
const PENDING_TARBALL: &str = "/var/lib/wardnet/updates/wardnetd-pending.tar.gz";
const PENDING_SIGNATURE: &str = "/var/lib/wardnet/updates/wardnetd-pending.tar.gz.minisig";
const ROLLBACK_REQUEST: &str = "/var/lib/wardnet/updates/wardnetd-rollback.request";

/// Corrupt-pair fixtures baked in by `Dockerfile.test`: a known-good
/// release tarball plus a signature over it made with a key nothing in
/// this image embeds.
const BAD_FIXTURE_TARBALL: &str = "/usr/local/share/wardnet-e2e/wardnetd-pending.tar.gz";
const BAD_FIXTURE_SIGNATURE: &str = "/usr/local/share/wardnet-e2e/wardnetd-pending.tar.gz.badsig";

/// `systemctl` by absolute path, deliberately.
///
/// This image parks a shim at `/usr/sbin/systemctl` that swallows
/// `reboot` / `poweroff` for the Safe Power spec (see
/// `Dockerfile.test`), and `/usr/sbin` precedes `/usr/bin` in systemd's
/// default `PATH`. A bare `systemctl` here would hit the shim, append a
/// line to its log, exit 0, and restart nothing — a silent no-op that
/// would make the trust-anchor spec pass for entirely the wrong reason.
/// `wardnetd-rollback.service` sidesteps the same trap with
/// `/bin/systemctl`.
const SYSTEMCTL: &str = "/usr/bin/systemctl";

/// Units the agent will act on. An allowlist rather than a free-form
/// unit name: the endpoint runs as root, and nothing in the suite needs
/// to touch anything else.
const ALLOWED_UNITS: &[&str] = &["wardnetd.service", "wardnet-postupgrade.service"];

// ---------------------------------------------------------------------------
// GET /update/binaries
// ---------------------------------------------------------------------------

/// One file's observable identity: presence, size, and content digest.
#[derive(Debug, Serialize)]
pub struct FileDigest {
    pub path: String,
    pub exists: bool,
    pub size_bytes: Option<u64>,
    /// Lowercase hex SHA-256 of the contents, `None` when absent.
    pub sha256: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct UpdateBinariesResponse {
    pub live: FileDigest,
    pub old: FileDigest,
    pub pending_tarball: FileDigest,
    pub pending_signature: FileDigest,
}

/// `GET /update/binaries` — digest the live binary, its `.old`
/// sibling, and the staged pending pair.
///
/// The digests are the load-bearing part. A swap and a no-op both leave
/// a file at `/usr/local/bin/wardnetd` of plausible size; only the
/// content distinguishes them.
pub async fn get_binaries() -> impl IntoResponse {
    Json(UpdateBinariesResponse {
        live: digest(LIVE_BINARY).await,
        old: digest(OLD_BINARY).await,
        pending_tarball: digest(PENDING_TARBALL).await,
        pending_signature: digest(PENDING_SIGNATURE).await,
    })
}

async fn digest(path: &str) -> FileDigest {
    match tokio::fs::read(path).await {
        Ok(bytes) => {
            let mut hasher = Sha256::new();
            hasher.update(&bytes);
            FileDigest {
                path: path.to_owned(),
                exists: true,
                size_bytes: Some(bytes.len() as u64),
                sha256: Some(hex::encode(hasher.finalize())),
            }
        }
        Err(e) => {
            if e.kind() != std::io::ErrorKind::NotFound {
                warn!(error = %e, path, "could not read {path} for digest");
            }
            FileDigest {
                path: path.to_owned(),
                exists: false,
                size_bytes: None,
                sha256: None,
            }
        }
    }
}

// ---------------------------------------------------------------------------
// POST /update/stage-bad-signature
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct StagedResponse {
    pub staged: bool,
    pub tarball: String,
    pub signature: String,
}

/// `POST /update/stage-bad-signature` — plant a validly-formatted but
/// wrong-key signature next to a good tarball in the staging directory.
///
/// Deliberately takes no parameters: the bytes come from fixtures baked
/// into the image, so the endpoint cannot be turned into an
/// arbitrary-write primitive even though it runs as root.
///
/// Ownership is set to `wardnet` to match what the daemon would have
/// written. The runner reads these as root either way, but leaving them
/// root-owned would misrepresent the scenario — the whole point is that
/// the *unprivileged* user staged them.
pub async fn post_stage_bad_signature() -> impl IntoResponse {
    if let Err(e) = stage_bad_signature().await {
        warn!(error = %e, "failed to stage the corrupt update pair");
        return (StatusCode::INTERNAL_SERVER_ERROR, format!("{e:#}")).into_response();
    }
    info!(
        tarball = PENDING_TARBALL,
        "staged a wrong-key signature for the trust-anchor spec"
    );
    Json(StagedResponse {
        staged: true,
        tarball: PENDING_TARBALL.to_owned(),
        signature: PENDING_SIGNATURE.to_owned(),
    })
    .into_response()
}

async fn stage_bad_signature() -> anyhow::Result<()> {
    let dir = Path::new(PENDING_TARBALL)
        .parent()
        .ok_or_else(|| anyhow::anyhow!("{PENDING_TARBALL} has no parent directory"))?;
    tokio::fs::create_dir_all(dir).await?;

    tokio::fs::copy(BAD_FIXTURE_TARBALL, PENDING_TARBALL).await?;
    tokio::fs::copy(BAD_FIXTURE_SIGNATURE, PENDING_SIGNATURE).await?;

    // A stale rollback request would take precedence over the pending
    // tarball in `swap::run` and mask the verification failure we are
    // trying to provoke.
    let _ = tokio::fs::remove_file(ROLLBACK_REQUEST).await;

    chown_wardnet(&[PENDING_TARBALL, PENDING_SIGNATURE]).await
}

async fn chown_wardnet(paths: &[&str]) -> anyhow::Result<()> {
    let output = Command::new("chown")
        .arg("wardnet:wardnet")
        .args(paths)
        .output()
        .await?;
    if !output.status.success() {
        anyhow::bail!(
            "chown wardnet:wardnet failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// POST /update/clear-staged  +  POST /update/restore-original
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct ClearStagedResponse {
    /// Paths that existed and were removed.
    pub removed: Vec<String>,
}

/// `POST /update/clear-staged` — drop the pending tarball, its
/// signature, and any rollback-request marker.
///
/// Recovery for the trust-anchor spec: while a pending tarball the
/// runner cannot verify is on disk, every `wardnetd.service` start
/// fails, so this is what unwedges the container.
pub async fn post_clear_staged() -> impl IntoResponse {
    let mut removed = Vec::new();
    for path in [PENDING_TARBALL, PENDING_SIGNATURE, ROLLBACK_REQUEST] {
        if tokio::fs::remove_file(path).await.is_ok() {
            removed.push(path.to_owned());
        }
    }
    info!(?removed, "cleared staged update artifacts");
    Json(ClearStagedResponse { removed })
}

#[derive(Debug, Serialize)]
pub struct RestoreResponse {
    /// `true` when an `.old` binary was found and renamed back.
    pub restored: bool,
}

/// `POST /update/restore-original` — rename `<live>.old` back over
/// `<live>` if it is present.
///
/// The unconditional half of the swap spec's `afterAll`. On the happy
/// path the spec's own rollback has already consumed the `.old`, so
/// this reports `restored: false` and does nothing. It matters when a
/// mid-file failure leaves the swapped-in `9999.12.31` binary live:
/// that would flip `update_available` to `false` and break
/// `update-status.spec.ts` in an unrelated slot.
pub async fn post_restore_original() -> impl IntoResponse {
    if !Path::new(OLD_BINARY).exists() {
        return Json(RestoreResponse { restored: false }).into_response();
    }
    match tokio::fs::rename(OLD_BINARY, LIVE_BINARY).await {
        Ok(()) => {
            info!("restored {OLD_BINARY} over {LIVE_BINARY}");
            Json(RestoreResponse { restored: true }).into_response()
        }
        Err(e) => {
            warn!(error = %e, "failed to restore {OLD_BINARY}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("rename {OLD_BINARY} -> {LIVE_BINARY}: {e}"),
            )
                .into_response()
        }
    }
}

// ---------------------------------------------------------------------------
// systemd
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct UnitQuery {
    pub unit: String,
}

#[derive(Debug, Serialize)]
pub struct UnitRestartResponse {
    pub unit: String,
    /// Exit code of `systemctl restart`. Nonzero when the unit refused
    /// to come up — which is the expected outcome in the trust-anchor
    /// spec, so this is reported rather than raised as a 5xx.
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

/// `POST /systemd/restart?unit=...` — `reset-failed` then `restart`.
///
/// The `reset-failed` is not cosmetic: `wardnetd.service` carries
/// `StartLimitBurst=5` over a 30s window, and a wedged start counts
/// against it. Without clearing the counter first, the recovery restart
/// at the end of the trust-anchor spec can be refused outright with
/// "start request repeated too quickly".
///
/// A nonzero `systemctl restart` is a legitimate result here, so it is
/// returned in the body rather than mapped to an error status. Callers
/// assert on `exit_code`.
pub async fn post_systemd_restart(Query(q): Query<UnitQuery>) -> impl IntoResponse {
    if !ALLOWED_UNITS.contains(&q.unit.as_str()) {
        return (
            StatusCode::BAD_REQUEST,
            format!("unit not allowed: {} (allowed: {ALLOWED_UNITS:?})", q.unit),
        )
            .into_response();
    }

    let _ = Command::new(SYSTEMCTL)
        .args(["reset-failed", &q.unit])
        .output()
        .await;

    let output = match Command::new(SYSTEMCTL)
        .args(["restart", &q.unit])
        .output()
        .await
    {
        Ok(o) => o,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed to spawn {SYSTEMCTL}: {e}"),
            )
                .into_response();
        }
    };

    let exit_code = output.status.code().unwrap_or(-1);
    info!(unit = %q.unit, exit_code, "restarted unit");
    Json(UnitRestartResponse {
        unit: q.unit,
        exit_code,
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
    .into_response()
}

#[derive(Debug, Serialize)]
pub struct UnitStatusResponse {
    pub unit: String,
    /// `active`, `failed`, `inactive`, `activating`, …
    pub active_state: String,
    /// `success`, `exit-code`, `dependency`, …
    pub result: String,
    /// Exit status of the unit's main process, as a string. `"0"` for a
    /// clean oneshot; the runner's nonzero exit shows up here.
    pub exec_main_status: String,
}

/// `GET /systemd/status?unit=...` — the three `systemctl show`
/// properties the specs assert on.
///
/// `systemctl show` is used rather than `is-active` because a wedged
/// `wardnetd.service` needs distinguishing from a merely-stopped one:
/// `Result=dependency` says the post-upgrade runner blocked it, which
/// is the assertion the trust-anchor spec actually wants to make.
pub async fn get_systemd_status(Query(q): Query<UnitQuery>) -> impl IntoResponse {
    if !ALLOWED_UNITS.contains(&q.unit.as_str()) {
        return (
            StatusCode::BAD_REQUEST,
            format!("unit not allowed: {} (allowed: {ALLOWED_UNITS:?})", q.unit),
        )
            .into_response();
    }

    let output = match Command::new(SYSTEMCTL)
        .args([
            "show",
            &q.unit,
            "--property=ActiveState",
            "--property=Result",
            "--property=ExecMainStatus",
        ])
        .output()
        .await
    {
        Ok(o) => o,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed to spawn {SYSTEMCTL}: {e}"),
            )
                .into_response();
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    Json(UnitStatusResponse {
        unit: q.unit.clone(),
        active_state: show_property(&stdout, "ActiveState"),
        result: show_property(&stdout, "Result"),
        exec_main_status: show_property(&stdout, "ExecMainStatus"),
    })
    .into_response()
}

/// Pull one `Key=value` line out of `systemctl show` output. Returns an
/// empty string when the property is absent, which is what systemd
/// itself does for a unit it has never loaded.
fn show_property(stdout: &str, key: &str) -> String {
    stdout
        .lines()
        .find_map(|line| line.strip_prefix(&format!("{key}=")))
        .unwrap_or_default()
        .trim()
        .to_owned()
}

#[cfg(test)]
mod tests;
