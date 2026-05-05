//! Auto-update service: status, check, install, rollback.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use tokio::sync::Mutex;
use uuid::Uuid;
use wardnet_common::api::{
    InstallUpdateRequest, InstallUpdateResponse, RollbackResponse, UpdateCheckResponse,
    UpdateConfigRequest, UpdateConfigResponse, UpdateHistoryResponse, UpdateStatusResponse,
};
use wardnet_common::event::WardnetEvent;
use wardnet_common::update::{
    InstallHandle, InstallPhase, Release, UpdateChannel, UpdateHistoryStatus, UpdateStatus,
};
use wardnetd_data::repository::{SystemConfigRepository, UpdateHistoryRow, UpdateRepository};

use crate::auth_context;
use crate::error::AppError;
use crate::event::EventPublisher;
use crate::update::applier::BinaryApplier;
use crate::update::release_source::ReleaseSource;
use crate::update::verifier::ReleaseVerifier;

/// Auto-update lifecycle: discover releases, install them, roll back on error.
///
/// All methods require admin authentication. Runtime state (last check,
/// pending version, channel, auto-update flag) lives in `system_config`;
/// history rows live in `update_history`.
#[async_trait]
pub trait UpdateService: Send + Sync {
    /// Current update subsystem snapshot.
    async fn status(&self) -> Result<UpdateStatusResponse, AppError>;

    /// Force a manifest refresh against the active channel and return the
    /// updated status. Called by the background runner and by manual
    /// `POST /api/update/check`.
    async fn check(&self) -> Result<UpdateCheckResponse, AppError>;

    /// Kick off an install. If the same install is already in flight,
    /// returns the existing handle (idempotent).
    async fn install(&self, req: InstallUpdateRequest) -> Result<InstallUpdateResponse, AppError>;

    /// Swap back to the `<live>.old` binary (if present) and clear pending
    /// version markers.
    async fn rollback(&self) -> Result<RollbackResponse, AppError>;

    /// Update the runtime auto-update config (channel, enabled).
    async fn update_config(
        &self,
        req: UpdateConfigRequest,
    ) -> Result<UpdateConfigResponse, AppError>;

    /// Recent history entries (newest first).
    async fn history(&self, limit: u32) -> Result<UpdateHistoryResponse, AppError>;

    /// Install the latest known release if auto-update is enabled and a
    /// newer version is available. Used by the background runner.
    ///
    /// Returns `Ok(None)` when no action was taken (disabled, already
    /// up-to-date, or an install is already in flight).
    async fn auto_install_if_due(&self) -> Result<Option<InstallHandle>, AppError>;
}

/// Track the in-flight install so concurrent callers see the same handle.
#[derive(Default)]
struct InflightState {
    handle: Option<InstallHandle>,
    phase: InstallPhase,
}

/// Default [`UpdateService`] implementation.
pub struct UpdateServiceImpl {
    system_config: Arc<dyn SystemConfigRepository>,
    history: Arc<dyn UpdateRepository>,
    release_source: Arc<dyn ReleaseSource>,
    verifier: Arc<dyn ReleaseVerifier>,
    applier: Arc<dyn BinaryApplier>,
    events: Arc<dyn EventPublisher>,
    require_signature: bool,
    current_version: String,
    inflight: Arc<Mutex<InflightState>>,
}

impl UpdateServiceImpl {
    /// Create a new service. `current_version` is the compile-time
    /// `WARDNET_VERSION`.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        system_config: Arc<dyn SystemConfigRepository>,
        history: Arc<dyn UpdateRepository>,
        release_source: Arc<dyn ReleaseSource>,
        verifier: Arc<dyn ReleaseVerifier>,
        applier: Arc<dyn BinaryApplier>,
        events: Arc<dyn EventPublisher>,
        require_signature: bool,
        current_version: impl Into<String>,
    ) -> Self {
        Self {
            system_config,
            history,
            release_source,
            verifier,
            applier,
            events,
            require_signature,
            current_version: current_version.into(),
            inflight: Arc::new(Mutex::new(InflightState::default())),
        }
    }

    async fn get_channel(&self) -> Result<UpdateChannel, AppError> {
        let value = self
            .system_config
            .get("update_channel")
            .await
            .map_err(AppError::Internal)?
            .unwrap_or_else(|| "stable".to_owned());
        Ok(UpdateChannel::parse_opt(&value).unwrap_or_default())
    }

    async fn get_auto_update_enabled(&self) -> Result<bool, AppError> {
        Ok(self
            .system_config
            .get("update_auto_update_enabled")
            .await
            .map_err(AppError::Internal)?
            .unwrap_or_else(|| "false".to_owned())
            == "true")
    }

    async fn get_last_check_at(&self) -> Result<Option<DateTime<Utc>>, AppError> {
        Ok(self
            .system_config
            .get("update_last_check_at")
            .await
            .map_err(AppError::Internal)?
            .filter(|s| !s.is_empty())
            .and_then(|s| s.parse().ok()))
    }

    async fn get_last_install_at(&self) -> Result<Option<DateTime<Utc>>, AppError> {
        Ok(self
            .system_config
            .get("update_last_install_at")
            .await
            .map_err(AppError::Internal)?
            .filter(|s| !s.is_empty())
            .and_then(|s| s.parse().ok()))
    }

    async fn get_last_known_version(&self) -> Result<Option<String>, AppError> {
        Ok(self
            .system_config
            .get("update_last_known_version")
            .await
            .map_err(AppError::Internal)?
            .filter(|s| !s.is_empty()))
    }

    async fn get_pending_version(&self) -> Result<Option<String>, AppError> {
        Ok(self
            .system_config
            .get("update_pending_version")
            .await
            .map_err(AppError::Internal)?
            .filter(|s| !s.is_empty()))
    }

    async fn set_cfg(&self, key: &str, value: &str) -> Result<(), AppError> {
        self.system_config
            .set(key, value)
            .await
            .map_err(AppError::Internal)
    }

    async fn build_status(&self) -> Result<UpdateStatus, AppError> {
        let channel = self.get_channel().await?;
        let auto = self.get_auto_update_enabled().await?;
        let last_check_at = self.get_last_check_at().await?;
        let last_install_at = self.get_last_install_at().await?;
        let latest = self.get_last_known_version().await?;
        let pending = self.get_pending_version().await?;
        let update_available = match latest.as_deref() {
            Some(v) if !v.is_empty() => is_newer(v, &self.current_version),
            _ => false,
        };
        let inflight = self.inflight.lock().await;
        let install_phase = inflight.phase.clone();
        drop(inflight);

        Ok(UpdateStatus {
            current_version: self.current_version.clone(),
            latest_version: latest,
            update_available,
            auto_update_enabled: auto,
            channel,
            last_check_at,
            last_install_at,
            install_phase,
            pending_version: pending,
            rollback_available: self.applier.rollback_available().await,
        })
    }

    async fn publish_progress(&self, target: &str, phase: InstallPhase) {
        let mut inflight = self.inflight.lock().await;
        inflight.phase = phase.clone();
        drop(inflight);
        self.events.publish(WardnetEvent::UpdateProgress {
            target_version: target.to_owned(),
            phase,
            timestamp: Utc::now(),
        });
    }

    pub(crate) async fn run_install(&self, release: Release) -> Result<(), AppError> {
        let target = release.version.clone();
        let history_id = self
            .history
            .insert(&UpdateHistoryRow {
                from_version: self.current_version.clone(),
                to_version: target.clone(),
                phase: "started".to_owned(),
                status: UpdateHistoryStatus::Started,
                error: None,
            })
            .await
            .map_err(AppError::Internal)?;

        let outcome = self.install_pipeline(&release).await;

        match outcome {
            Ok(()) => {
                self.set_cfg("update_pending_version", &target).await?;
                self.set_cfg("update_last_install_at", &Utc::now().to_rfc3339())
                    .await?;
                self.publish_progress(&target, InstallPhase::RestartPending)
                    .await;
                self.events.publish(WardnetEvent::UpdateCompleted {
                    from_version: self.current_version.clone(),
                    to_version: target.clone(),
                    timestamp: Utc::now(),
                });
                self.history
                    .finalize(
                        history_id,
                        UpdateHistoryStatus::Succeeded,
                        "restart_pending",
                        None,
                    )
                    .await
                    .map_err(AppError::Internal)?;
                tracing::info!(
                    target = %target,
                    "update installed — daemon will be restarted by systemd: target={target}",
                    target = target,
                );
                Ok(())
            }
            Err((phase, err)) => {
                // `{:#}` walks the anyhow chain so context wrapping (e.g.
                // the path the applier tried to swap) reaches the history
                // row and log line, not just the bare `io::Error`.
                let msg = format!("{err:#}");
                self.publish_progress(
                    &target,
                    InstallPhase::Failed {
                        reason: msg.clone(),
                    },
                )
                .await;
                self.events.publish(WardnetEvent::UpdateFailed {
                    target_version: target.clone(),
                    phase: phase.clone(),
                    error: msg.clone(),
                    timestamp: Utc::now(),
                });
                let phase_str = phase_name(&phase);
                self.history
                    .finalize(
                        history_id,
                        UpdateHistoryStatus::Failed,
                        phase_str,
                        Some(msg.as_str()),
                    )
                    .await
                    .map_err(AppError::Internal)?;
                tracing::error!(
                    target = %target,
                    error = %msg,
                    phase = phase_str,
                    "update install failed: target={target}, phase={phase_str}, error={msg}",
                );
                Err(err)
            }
        }
    }

    /// Run the download/verify/stage/swap pipeline. Emits progress events at
    /// each phase transition. On failure, returns the phase where the error
    /// occurred so history rows record a useful `phase` column.
    pub(crate) async fn install_pipeline(
        &self,
        release: &Release,
    ) -> Result<(), (InstallPhase, AppError)> {
        let target = release.version.clone();
        let downloading = || InstallPhase::Downloading {
            bytes: 0,
            total: None,
        };

        self.publish_progress(&target, downloading()).await;
        let tarball = self
            .release_source
            .fetch_asset(&release.tarball_url)
            .await
            .map_err(|e| (downloading(), AppError::Internal(e)))?;
        self.publish_progress(
            &target,
            InstallPhase::Downloading {
                bytes: tarball.len() as u64,
                total: Some(tarball.len() as u64),
            },
        )
        .await;

        let sha_bytes = self
            .release_source
            .fetch_asset(&release.sha256_url)
            .await
            .map_err(|e| (downloading(), AppError::Internal(e)))?;
        let sha256_hex = String::from_utf8(sha_bytes)
            .map_err(|e| {
                (
                    downloading(),
                    AppError::Internal(anyhow::anyhow!("sha256 sidecar not utf-8: {e}")),
                )
            })?
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_owned();

        self.publish_progress(&target, InstallPhase::Verifying)
            .await;

        self.verifier
            .verify_sha256(&tarball, &sha256_hex)
            .await
            .map_err(|e| (InstallPhase::Verifying, AppError::Internal(e)))?;

        // The detached signature is required for the runner-mediated
        // swap regardless of `require_signature` — `wardnet-postupgrade-
        // runner` re-verifies it under root before renaming the binary.
        // `require_signature` only controls whether the daemon also
        // verifies pre-stash; the trust anchor's check is unconditional.
        let sig_bytes = if let Some(minisig_url) = release.minisig_url.clone() {
            self.release_source
                .fetch_asset(&minisig_url)
                .await
                .map_err(|e| (InstallPhase::Verifying, AppError::Internal(e)))?
        } else if self.require_signature {
            return Err((
                InstallPhase::Verifying,
                AppError::Internal(anyhow::anyhow!(
                    "signature required but release has no minisig_url"
                )),
            ));
        } else {
            tracing::warn!(
                "signature verification disabled — staging without minisign \
                 (runner will refuse to swap without one)"
            );
            Vec::new()
        };

        if self.require_signature {
            self.verifier
                .verify_signature(&tarball, &sig_bytes)
                .await
                .map_err(|e| (InstallPhase::Verifying, AppError::Internal(e)))?;
        }

        self.publish_progress(&target, InstallPhase::Staging).await;
        self.publish_progress(&target, InstallPhase::Swapping).await;

        let outcome = self
            .applier
            .apply(&tarball, &sig_bytes)
            .await
            .map_err(|e| (InstallPhase::Swapping, AppError::Internal(e)))?;

        let previous = outcome.previous_binary.to_string_lossy().into_owned();
        self.set_cfg("update_previous_binary_path", &previous)
            .await
            .map_err(|e| (InstallPhase::Swapping, e))?;

        Ok(())
    }
}

/// Compare `CalVer` / dotted-numeric version strings. Returns `true`
/// if `candidate` is strictly greater than `current`.
///
/// Both versions use the `YYYY.MM.DD` shape published by the release
/// pipeline (see the workspace-root `CALVER` file). Padded month /
/// day components like `2026.05.03` are not valid `SemVer` (leading
/// zeros are rejected), so we can't reuse `semver::Version::parse`
/// here. Instead, split on `.`, parse each component as `u64`, and
/// compare the resulting tuples lexicographically — that gives the
/// right ordering for both padded (`05`) and unpadded (`5`)
/// components, and for any pre-release suffix we strip first.
///
/// Returns `false` on any parse failure rather than panicking, so a
/// malformed manifest entry can never trigger an install.
fn is_newer(candidate: &str, current: &str) -> bool {
    fn parts(v: &str) -> Option<Vec<u64>> {
        // Drop a SemVer-style pre-release / build suffix (`-beta.1`,
        // `+gabc1234`) before parsing; we don't currently use those for
        // CalVer releases, but tolerating them future-proofs the
        // comparator and avoids spurious "not newer" verdicts when
        // upstream injects a build tag.
        let head = v.split(['-', '+']).next().unwrap_or(v);
        let parts: Vec<u64> = head
            .split('.')
            .map(|p| p.parse::<u64>().ok())
            .collect::<Option<Vec<_>>>()?;
        if parts.is_empty() { None } else { Some(parts) }
    }
    match (parts(candidate), parts(current)) {
        (Some(c), Some(r)) => c > r,
        _ => false,
    }
}

fn phase_name(p: &InstallPhase) -> &'static str {
    match p {
        InstallPhase::Idle => "idle",
        InstallPhase::Checking => "checking",
        InstallPhase::Downloading { .. } => "downloading",
        InstallPhase::Verifying => "verifying",
        InstallPhase::Staging => "staging",
        InstallPhase::Swapping => "swapping",
        InstallPhase::RestartPending => "restart_pending",
        InstallPhase::Applied => "applied",
        InstallPhase::Failed { .. } => "failed",
    }
}

#[async_trait]
impl UpdateService for UpdateServiceImpl {
    async fn status(&self) -> Result<UpdateStatusResponse, AppError> {
        auth_context::require_admin()?;
        Ok(UpdateStatusResponse {
            status: self.build_status().await?,
        })
    }

    async fn check(&self) -> Result<UpdateCheckResponse, AppError> {
        auth_context::require_admin()?;
        let channel = self.get_channel().await?;

        let previous_phase = {
            let mut inflight = self.inflight.lock().await;
            let prev = inflight.phase.clone();
            if matches!(inflight.phase, InstallPhase::Idle) {
                inflight.phase = InstallPhase::Checking;
            }
            prev
        };

        let result = self.release_source.latest(channel).await;

        {
            let mut inflight = self.inflight.lock().await;
            if matches!(inflight.phase, InstallPhase::Checking) {
                inflight.phase = previous_phase;
            }
        }

        let latest = result.map_err(|e| {
            // Emit the full error text in the tracing *message* (not just a
            // structured field) so `ErrorNotifierService` captures it in the
            // `/api/system/errors` feed — otherwise the client only sees an
            // opaque "internal server error" in the toast and the error
            // notifier only stores "internal server error" as the message.
            tracing::warn!(
                channel = channel.as_str(),
                error = %e,
                "update check failed on channel {channel}: {e}",
                channel = channel.as_str(),
            );
            AppError::UpstreamUnavailable(format!(
                "release manifest fetch failed ({channel}): {e}",
                channel = channel.as_str(),
            ))
        })?;
        self.set_cfg("update_last_check_at", &Utc::now().to_rfc3339())
            .await?;

        if let Some(ref release) = latest {
            self.set_cfg("update_last_known_version", &release.version)
                .await?;
            if is_newer(&release.version, &self.current_version) {
                self.events.publish(WardnetEvent::UpdateAvailable {
                    current_version: self.current_version.clone(),
                    latest_version: release.version.clone(),
                    timestamp: Utc::now(),
                });
                tracing::info!(
                    current = %self.current_version,
                    latest = %release.version,
                    channel = channel.as_str(),
                    "update available: current={current}, latest={latest}, channel={channel}",
                    current = self.current_version,
                    latest = release.version,
                    channel = channel.as_str(),
                );
            }
        } else {
            self.set_cfg("update_last_known_version", "").await?;
        }

        Ok(UpdateCheckResponse {
            status: self.build_status().await?,
        })
    }

    async fn install(&self, req: InstallUpdateRequest) -> Result<InstallUpdateResponse, AppError> {
        auth_context::require_admin()?;

        // Idempotency: if a matching install is already running, return its handle.
        {
            let inflight = self.inflight.lock().await;
            if let Some(handle) = &inflight.handle {
                if req
                    .version
                    .as_deref()
                    .is_none_or(|v| v == handle.target_version)
                {
                    return Ok(InstallUpdateResponse {
                        handle: handle.clone(),
                        message: "install already in progress".to_owned(),
                    });
                }
                return Err(AppError::Conflict(format!(
                    "install already in progress for version {}",
                    handle.target_version
                )));
            }
        }

        let channel = self.get_channel().await?;
        let release = self
            .release_source
            .latest(channel)
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::NotFound("no release available".to_owned()))?;

        if let Some(requested) = req.version.as_deref()
            && requested != release.version
        {
            return Err(AppError::BadRequest(format!(
                "requested version {requested} not available on channel {} (latest: {})",
                channel.as_str(),
                release.version,
            )));
        }

        if !is_newer(&release.version, &self.current_version) {
            return Err(AppError::BadRequest(format!(
                "refusing to install {}: not newer than current {} (use rollback to downgrade)",
                release.version, self.current_version,
            )));
        }

        let handle = InstallHandle {
            install_id: Uuid::new_v4(),
            target_version: release.version.clone(),
        };

        {
            let mut inflight = self.inflight.lock().await;
            inflight.handle = Some(handle.clone());
            inflight.phase = InstallPhase::Checking;
        }

        let svc_clone = self.clone_for_task();
        let release_clone = release.clone();
        tokio::spawn(async move {
            let admin_ctx = wardnet_common::auth::AuthContext::Admin {
                admin_id: Uuid::nil(),
            };
            let result =
                auth_context::with_context(admin_ctx, svc_clone.run_install(release_clone)).await;
            let mut inflight = svc_clone.inflight.lock().await;
            inflight.handle = None;
            if matches!(result, Ok(())) {
                inflight.phase = InstallPhase::RestartPending;
            } else if !matches!(inflight.phase, InstallPhase::Failed { .. }) {
                inflight.phase = InstallPhase::Idle;
            }
        });

        Ok(InstallUpdateResponse {
            handle,
            message: "install started".to_owned(),
        })
    }

    async fn rollback(&self) -> Result<RollbackResponse, AppError> {
        auth_context::require_admin()?;
        if !self.applier.rollback_available().await {
            return Err(AppError::BadRequest(
                "no previous binary available for rollback".to_owned(),
            ));
        }
        self.applier.rollback().await.map_err(AppError::Internal)?;

        self.set_cfg("update_pending_version", "").await?;
        self.set_cfg("update_previous_binary_path", "").await?;
        self.history
            .insert(&UpdateHistoryRow {
                from_version: self.current_version.clone(),
                to_version: "previous".to_owned(),
                phase: "rolled_back".to_owned(),
                status: UpdateHistoryStatus::RolledBack,
                error: None,
            })
            .await
            .map_err(AppError::Internal)?;

        tracing::info!(
            "rollback request staged — runner will restore previous binary on next daemon restart",
        );
        Ok(RollbackResponse {
            message: "rollback staged — daemon will restart".to_owned(),
        })
    }

    async fn update_config(
        &self,
        req: UpdateConfigRequest,
    ) -> Result<UpdateConfigResponse, AppError> {
        auth_context::require_admin()?;
        if let Some(enabled) = req.auto_update_enabled {
            self.set_cfg(
                "update_auto_update_enabled",
                if enabled { "true" } else { "false" },
            )
            .await?;
        }
        if let Some(channel) = req.channel {
            self.set_cfg("update_channel", channel.as_str()).await?;
            // Invalidate the last-known version since it was for the old channel.
            self.set_cfg("update_last_known_version", "").await?;
        }
        Ok(UpdateConfigResponse {
            status: self.build_status().await?,
        })
    }

    async fn history(&self, limit: u32) -> Result<UpdateHistoryResponse, AppError> {
        auth_context::require_admin()?;
        let entries = self
            .history
            .list(limit.max(1))
            .await
            .map_err(AppError::Internal)?;
        Ok(UpdateHistoryResponse { entries })
    }

    async fn auto_install_if_due(&self) -> Result<Option<InstallHandle>, AppError> {
        auth_context::require_admin()?;
        if !self.get_auto_update_enabled().await? {
            return Ok(None);
        }
        {
            let inflight = self.inflight.lock().await;
            if inflight.handle.is_some() {
                return Ok(None);
            }
        }
        let channel = self.get_channel().await?;
        let latest = self
            .release_source
            .latest(channel)
            .await
            .map_err(AppError::Internal)?;
        let Some(release) = latest else {
            return Ok(None);
        };
        if !is_newer(&release.version, &self.current_version) {
            return Ok(None);
        }
        let handle = self
            .install(InstallUpdateRequest {
                version: Some(release.version.clone()),
            })
            .await?;
        Ok(Some(handle.handle))
    }
}

impl UpdateServiceImpl {
    /// Produce an `Arc<Self>`-backed clone for `tokio::spawn`. This avoids
    /// cloning the entire struct — we clone the interior `Arc`s only.
    fn clone_for_task(&self) -> Arc<Self> {
        Arc::new(Self {
            system_config: self.system_config.clone(),
            history: self.history.clone(),
            release_source: self.release_source.clone(),
            verifier: self.verifier.clone(),
            applier: self.applier.clone(),
            events: self.events.clone(),
            require_signature: self.require_signature,
            current_version: self.current_version.clone(),
            inflight: self.inflight.clone(),
        })
    }
}

#[cfg(test)]
mod is_newer_tests {
    use super::is_newer;

    #[test]
    fn calver_padded_components() {
        // The padded canonical CalVer form — leading zeros in month / day
        // were the reason we couldn't keep `semver::Version::parse`.
        assert!(is_newer("2026.05.10", "2026.05.03"));
        assert!(is_newer("2026.06.01", "2026.05.31"));
        assert!(!is_newer("2026.05.03", "2026.05.10"));
        assert!(!is_newer("2026.05.03", "2026.05.03"));
    }

    #[test]
    fn calver_unpadded_compares_numerically() {
        // `2026.5.3` < `2026.10.3` — the comparator parses each component
        // as `u64` so the lexically-smaller `10` correctly outranks `5`.
        assert!(is_newer("2026.10.3", "2026.5.3"));
        assert!(!is_newer("2026.5.3", "2026.10.3"));
    }

    #[test]
    fn semver_legacy_strings_still_compare() {
        // Pre-CalVer release tooling would have used these. Still works
        // because the comparator is tuple-of-ints, not CalVer-specific.
        assert!(is_newer("0.2.0", "0.1.0"));
        assert!(!is_newer("0.1.0", "0.2.0"));
    }

    #[test]
    fn suffixes_are_stripped_before_comparison() {
        // A `+gabc1234` build tag or `-beta.1` pre-release suffix on
        // either side must not mask the underlying ordering. We don't
        // currently publish pre-releases for CalVer, but tolerating them
        // keeps the gate from misfiring on a manifest that introduces
        // them later.
        assert!(is_newer("2026.05.10+gabc", "2026.05.03+gdef"));
        assert!(is_newer("2026.05.10-beta.1", "2026.05.03"));
    }

    #[test]
    fn malformed_returns_false() {
        // A malformed candidate must never report "newer" — that would
        // let a bad manifest entry trigger an install.
        assert!(!is_newer("not-a-version", "2026.05.03"));
        assert!(!is_newer("2026.05.03", "also-bad"));
        assert!(!is_newer("", "2026.05.03"));
    }
}
