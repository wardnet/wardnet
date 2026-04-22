//! `BackupService` — composes the archiver, dumper, and secret store
//! into admin-facing operations.
//!
//! ### Semantics
//!
//! * **Export** is a read-only capture: dump the DB via `VACUUM INTO`,
//!   read the config file, snapshot the secret store, pack everything
//!   into an age-encrypted `.wardnet.age` stream. Streams the encrypted
//!   bytes back to the caller — the plaintext never touches disk.
//! * **Preview-then-apply import** is two-phase. `preview_import`
//!   decrypts, validates format/schema compat, and caches the unpacked
//!   contents under a short-lived `preview_token`. `apply_import`
//!   consumes that token, renames live files to `.bak-<ts>` siblings,
//!   writes the new contents, and sets a `backup_restart_pending`
//!   marker. Operators (or the systemd auto-restart unit shipped with
//!   the installer) must restart the daemon for the running pool to
//!   pick up the new database file.
//! * Every method begins with `auth_context::require_admin()?` — no
//!   anonymous access to backup operations, same defense-in-depth rule
//!   as the rest of the service layer.
//!
//! ### What happens to the secret store on restore
//!
//! The pre-restore state of the secret store is captured via the
//! trait's [`backup_contents`](wardnetd_data::secret_store::SecretStore::backup_contents)
//! method and serialised to a sibling `secrets.bak-<ts>.json`. The
//! secret store is then overwritten with the bundle's contents via
//! [`restore_from_backup`](wardnetd_data::secret_store::SecretStore::restore_from_backup).
//! Providers whose secrets live externally (e.g. `HashiCorp` Vault) return
//! empty lists from both methods, so the JSON sibling is omitted and
//! the overwrite is a no-op.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use base64::Engine;
use chrono::{DateTime, Utc};
use tokio::sync::Mutex;
use tokio::time::Instant;
use uuid::Uuid;
use wardnet_common::api::{
    ApplyImportRequest, ApplyImportResponse, BackupStatusResponse, ExportBackupRequest,
    ListSnapshotsResponse, RestorePreviewResponse,
};
use wardnet_common::backup::{
    BackupStatus, BundleManifest, CURRENT_BUNDLE_FORMAT_VERSION, LocalSnapshot, MIN_PASSPHRASE_LEN,
    RestorePhase, SnapshotKind,
};
use wardnetd_data::database_dumper::DatabaseDumper;
use wardnetd_data::repository::SystemConfigRepository;
use wardnetd_data::secret_store::{SecretEntry, SecretStore};

use crate::auth_context;
use crate::backup::archiver::{BackupArchiver, BundleContents};
use crate::error::AppError;

/// How long a preview token remains valid before `apply_import`
/// refuses it. Short window on purpose — the operator is expected to
/// confirm the restore within a few minutes of previewing it.
const PREVIEW_TOKEN_TTL: Duration = Duration::from_secs(5 * 60);

/// `system_config` key flipped to `"true"` after a successful
/// `apply_import`. The auto-update subsystem's rollback unit reads this
/// alongside its own pending marker; the web UI surfaces the banner.
pub const BACKUP_RESTART_PENDING_KEY: &str = "backup_restart_pending";

/// Backup/restore operations. All methods are admin-guarded.
#[async_trait]
pub trait BackupService: Send + Sync {
    /// Current subsystem snapshot.
    async fn status(&self) -> Result<BackupStatusResponse, AppError>;

    /// Produce a fully-encrypted `.wardnet.age` bundle. Returns the
    /// raw bytes — the HTTP layer streams them to the client with an
    /// appropriate `Content-Disposition`.
    async fn export(&self, req: ExportBackupRequest) -> Result<Vec<u8>, AppError>;

    /// Decrypt a bundle, validate compatibility, and cache the
    /// unpacked contents under a `preview_token`. Nothing on disk
    /// changes.
    async fn preview_import(
        &self,
        bundle: Vec<u8>,
        passphrase: String,
    ) -> Result<RestorePreviewResponse, AppError>;

    /// Consume a `preview_token` and commit the restore. Renames live
    /// files to `.bak-<ts>` siblings, writes new contents, sets
    /// `backup_restart_pending=true` in `system_config`.
    async fn apply_import(&self, req: ApplyImportRequest) -> Result<ApplyImportResponse, AppError>;

    /// Enumerate `.bak-*` siblings retained by previous restores.
    async fn list_snapshots(&self) -> Result<ListSnapshotsResponse, AppError>;

    /// Delete `.bak-*` siblings older than `retain`. Called on a timer
    /// by [`BackupCleanupRunner`](crate::backup::runner::BackupCleanupRunner)
    /// and exercisable directly by tests.
    async fn cleanup_old_snapshots(&self, retain: Duration) -> Result<u32, AppError>;
}

/// Default [`BackupService`] implementation.
pub struct BackupServiceImpl {
    archiver: Arc<dyn BackupArchiver>,
    dumper: Arc<dyn DatabaseDumper>,
    secret_store: Arc<dyn SecretStore>,
    system_config: Arc<dyn SystemConfigRepository>,
    database_path: PathBuf,
    config_path: PathBuf,
    current_version: String,
    host_id: String,
    pending: Arc<Mutex<HashMap<String, PendingImport>>>,
    status: Arc<Mutex<BackupStatus>>,
    /// Serialises the mutating operations (`export`, `preview_import`,
    /// `apply_import`) so two concurrent admin calls can't interleave
    /// file renames or preview state. Read-only methods (`status`,
    /// `list_snapshots`) do not take this lock.
    op_lock: Arc<Mutex<()>>,
}

/// An import waiting for the operator to confirm via `apply_import`.
#[derive(Clone)]
struct PendingImport {
    contents: BundleContents,
    created_at: Instant,
}

impl BackupServiceImpl {
    /// Construct a new service. `current_version` is typically
    /// `env!("WARDNET_VERSION")`; `host_id` is an operator-visible
    /// identifier (hostname is a reasonable choice) surfaced in the
    /// manifest so operators can double-check which machine a bundle
    /// came from during restore.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn new(
        archiver: Arc<dyn BackupArchiver>,
        dumper: Arc<dyn DatabaseDumper>,
        secret_store: Arc<dyn SecretStore>,
        system_config: Arc<dyn SystemConfigRepository>,
        database_path: PathBuf,
        config_path: PathBuf,
        current_version: impl Into<String>,
        host_id: impl Into<String>,
    ) -> Self {
        Self {
            archiver,
            dumper,
            secret_store,
            system_config,
            database_path,
            config_path,
            current_version: current_version.into(),
            host_id: host_id.into(),
            pending: Arc::new(Mutex::new(HashMap::new())),
            status: Arc::new(Mutex::new(BackupStatus::default())),
            op_lock: Arc::new(Mutex::new(())),
        }
    }

    async fn set_status(&self, status: BackupStatus) {
        *self.status.lock().await = status;
    }

    fn require_passphrase(passphrase: &str) -> Result<(), AppError> {
        if passphrase.chars().count() < MIN_PASSPHRASE_LEN {
            return Err(AppError::BadRequest(format!(
                "passphrase must be at least {MIN_PASSPHRASE_LEN} characters"
            )));
        }
        Ok(())
    }

    /// Directory where `.bak-*` snapshots live — always a sibling of
    /// the live database file.
    fn snapshot_dir(&self) -> &Path {
        self.database_path.parent().unwrap_or(Path::new("."))
    }

    /// Decide whether a freshly-unpacked manifest can be restored
    /// against the running daemon.
    async fn check_compat(
        &self,
        manifest: &BundleManifest,
    ) -> Result<(bool, Option<String>), AppError> {
        if !manifest.is_format_supported() {
            return Ok((
                false,
                Some(format!(
                    "bundle format version {} is newer than supported ({}) — upgrade the daemon first",
                    manifest.bundle_format_version, CURRENT_BUNDLE_FORMAT_VERSION
                )),
            ));
        }
        let running_schema = self
            .dumper
            .current_schema_version()
            .await
            .map_err(AppError::Internal)?;
        if manifest.schema_version > running_schema {
            return Ok((
                false,
                Some(format!(
                    "bundle schema version {} is newer than the running daemon's ({}) — upgrade the daemon first, then retry",
                    manifest.schema_version, running_schema
                )),
            ));
        }
        Ok((true, None))
    }

    /// Serialise a `Vec<SecretEntry>` as a base64-encoded JSON blob
    /// suitable for the `secrets.bak-<timestamp>.json` sibling.
    fn serialise_secrets(entries: &[SecretEntry]) -> Result<Vec<u8>, AppError> {
        let serialisable: Vec<_> = entries
            .iter()
            .map(|entry| {
                serde_json::json!({
                    "path": &entry.path,
                    "value_b64": base64::engine::general_purpose::STANDARD.encode(&entry.value),
                })
            })
            .collect();
        serde_json::to_vec_pretty(&serialisable).map_err(|e| AppError::Internal(anyhow::anyhow!(e)))
    }

    /// Write a captured pre-restore secret set to
    /// `secrets.bak-<timestamp>.json` next to the live database. The
    /// file is created with mode `0600` — it holds base64'd private
    /// keys and must not be world-readable. Returns `None` when the
    /// store had nothing to capture (external-provider case).
    async fn write_secrets_snapshot(
        &self,
        entries: &[SecretEntry],
        timestamp: &str,
    ) -> Result<Option<LocalSnapshot>, AppError> {
        if entries.is_empty() {
            return Ok(None);
        }
        let path = self
            .snapshot_dir()
            .join(format!("secrets.bak-{timestamp}.json"));
        let json = Self::serialise_secrets(entries)?;
        tokio::fs::write(&path, &json).await.map_err(|e| {
            AppError::Internal(anyhow::anyhow!(
                "failed to save pre-restore secrets snapshot at {}: {e}",
                path.display()
            ))
        })?;

        // Tighten perms — the JSON holds base64-encoded WireGuard
        // private keys. Default umask leaves it world-readable.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            tokio::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
                .await
                .map_err(|e| {
                    AppError::Internal(anyhow::anyhow!(
                        "failed to tighten permissions on secrets snapshot {}: {e}",
                        path.display()
                    ))
                })?;
        }

        Ok(Some(LocalSnapshot {
            path: path.display().to_string(),
            kind: SnapshotKind::Keys,
            created_at: Utc::now(),
            size_bytes: json.len() as u64,
        }))
    }

    /// Temporary sibling path used to hold the pre-restore live file
    /// while the restore writes new content into place. UUID-suffixed
    /// so two concurrent imports (impossible today thanks to
    /// `op_lock`, but belt-and-braces) can't collide.
    fn hold_path(base: &Path) -> PathBuf {
        let name = base
            .file_name()
            .map_or_else(|| "file".to_owned(), |n| n.to_string_lossy().into_owned());
        base.with_file_name(format!(".{name}.pre-restore-{}", Uuid::new_v4()))
    }

    /// Stage the bundle's config bytes as a sibling file so the
    /// eventual `rename` into place is atomic. The staging file is
    /// created with `0600` — the config may carry provider
    /// credentials or secrets referenced by other subsystems.
    async fn stage_config(&self, bytes: &[u8]) -> Result<PathBuf, AppError> {
        let path = self
            .config_path
            .with_file_name(format!(".wardnet.toml.restore-{}.new", Uuid::new_v4()));
        tokio::fs::write(&path, bytes).await.map_err(|e| {
            AppError::Internal(anyhow::anyhow!(
                "failed to stage restored config at {}: {e}",
                path.display()
            ))
        })?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            tokio::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
                .await
                .map_err(|e| {
                    AppError::Internal(anyhow::anyhow!(
                        "failed to tighten permissions on staged config {}: {e}",
                        path.display()
                    ))
                })?;
        }
        Ok(path)
    }

    /// Phased apply: stage new content → swap live files aside to
    /// UUID-suffixed holds → install from staging → promote holds to
    /// `.bak-<timestamp>`. On any failure after the swap begins,
    /// best-effort rollback renames the holds back into place and
    /// replays the pre-restore secret-store state.
    async fn run_apply_import(
        &self,
        pending: &PendingImport,
        timestamp: &str,
    ) -> Result<Vec<LocalSnapshot>, AppError> {
        // ---- Phase 0: capture pre-restore secret state (for rollback) ----
        //
        // Read only — nothing on disk is touched yet. If this errors,
        // we abort before any destructive action.
        let pre_restore_secrets = self
            .secret_store
            .backup_contents()
            .await
            .map_err(AppError::Internal)?;

        // ---- Phase 1: stage new config beside the live file ----
        //
        // Any error here leaves all live files untouched. We don't
        // stage the database the same way because `DatabaseDumper::restore`
        // already implements its own write+rename internally.
        self.set_status(BackupStatus::Importing {
            phase: RestorePhase::Validating,
        })
        .await;
        let staged_config = self.stage_config(&pending.contents.config_bytes).await?;

        // ---- Phase 2: move live files aside ----
        //
        // Swap into temp `.pre-restore-<uuid>` holds. If phase 3 later
        // fails, we rename them back. Each rename is atomic on Linux
        // for same-filesystem operations.
        self.set_status(BackupStatus::Importing {
            phase: RestorePhase::BackingUp,
        })
        .await;
        let db_hold = Self::hold_path(&self.database_path);
        let config_hold = Self::hold_path(&self.config_path);

        if let Err(e) = tokio::fs::rename(&self.database_path, &db_hold).await {
            // Nothing was moved yet — just clean up staging and fail.
            let _ = tokio::fs::remove_file(&staged_config).await;
            if e.kind() != std::io::ErrorKind::NotFound {
                return Err(AppError::Internal(anyhow::anyhow!(
                    "failed to move live database aside: {e}"
                )));
            }
            // NotFound means there was no live DB to displace — OK to
            // proceed as if the hold doesn't exist.
        }

        // ---- Phase 3: install new content ----
        //
        // The three destructive writes. Wrap in an async block so
        // on failure we can attempt a coordinated rollback below.
        self.set_status(BackupStatus::Importing {
            phase: RestorePhase::Extracting,
        })
        .await;

        let install_result: Result<(), anyhow::Error> = async {
            self.dumper
                .restore(&pending.contents.database_bytes)
                .await?;

            if let Err(e) = tokio::fs::rename(&self.config_path, &config_hold).await
                && e.kind() != std::io::ErrorKind::NotFound
            {
                return Err(anyhow::anyhow!("failed to move live config aside: {e}"));
            }
            tokio::fs::rename(&staged_config, &self.config_path).await?;

            self.secret_store
                .restore_from_backup(&pending.contents.secrets)
                .await?;
            Ok(())
        }
        .await;

        if let Err(e) = install_result {
            // ---- Rollback ----
            //
            // Best-effort: rename holds back over whatever is at the
            // live paths. `rename(2)` on Linux atomically replaces the
            // destination, so the window of inconsistency closes with
            // each step that succeeds.
            if db_hold.exists() {
                let _ = tokio::fs::rename(&db_hold, &self.database_path).await;
            }
            if config_hold.exists() {
                let _ = tokio::fs::rename(&config_hold, &self.config_path).await;
            }
            let _ = tokio::fs::remove_file(&staged_config).await;
            // Replay pre-restore secrets — the `restore_from_backup`
            // contract is "replace store contents with these entries",
            // so re-applying the captured list reverts the store.
            let _ = self
                .secret_store
                .restore_from_backup(&pre_restore_secrets)
                .await;
            return Err(AppError::Internal(anyhow::anyhow!(
                "restore failed, rolled back: {e}"
            )));
        }

        // ---- Phase 4: mark restart pending + promote holds ----
        //
        // The install succeeded. Set the restart flag first (it
        // writes to the restored DB) so the restart-pending banner
        // shows up even if the promotion step below trips on a
        // permission issue.
        self.system_config
            .set(BACKUP_RESTART_PENDING_KEY, "true")
            .await
            .map_err(AppError::Internal)?;

        let mut snapshots: Vec<LocalSnapshot> = Vec::new();
        if let Some(s) = Self::promote_hold_to_snapshot(
            &db_hold,
            &self.database_path,
            timestamp,
            SnapshotKind::Database,
        )
        .await?
        {
            snapshots.push(s);
        }
        if let Some(s) = Self::promote_hold_to_snapshot(
            &config_hold,
            &self.config_path,
            timestamp,
            SnapshotKind::Config,
        )
        .await?
        {
            snapshots.push(s);
        }
        if let Some(s) = self
            .write_secrets_snapshot(&pre_restore_secrets, timestamp)
            .await?
        {
            snapshots.push(s);
        }

        Ok(snapshots)
    }

    /// Promote a temporary `.pre-restore-<uuid>` hold into the
    /// user-visible `<name>.bak-<timestamp>` snapshot a restore
    /// advertises back to the operator.
    async fn promote_hold_to_snapshot(
        hold: &Path,
        live: &Path,
        timestamp: &str,
        kind: SnapshotKind,
    ) -> Result<Option<LocalSnapshot>, AppError> {
        if !hold.exists() {
            return Ok(None);
        }
        let live_name = live
            .file_name()
            .map_or_else(|| "file".to_owned(), |n| n.to_string_lossy().into_owned());
        let dst = live.with_file_name(format!("{live_name}.bak-{timestamp}"));
        tokio::fs::rename(hold, &dst).await.map_err(|e| {
            AppError::Internal(anyhow::anyhow!(
                "failed to promote hold {} to snapshot {}: {e}",
                hold.display(),
                dst.display()
            ))
        })?;
        let size = tokio::fs::metadata(&dst)
            .await
            .map(|m| m.len())
            .unwrap_or(0);
        Ok(Some(LocalSnapshot {
            path: dst.display().to_string(),
            kind,
            created_at: Utc::now(),
            size_bytes: size,
        }))
    }
}

#[async_trait]
impl BackupService for BackupServiceImpl {
    async fn status(&self) -> Result<BackupStatusResponse, AppError> {
        auth_context::require_admin()?;
        let status = self.status.lock().await.clone();
        Ok(BackupStatusResponse { status })
    }

    async fn export(&self, req: ExportBackupRequest) -> Result<Vec<u8>, AppError> {
        auth_context::require_admin()?;
        Self::require_passphrase(&req.passphrase)?;

        // Serialise with any other mutating backup op so a concurrent
        // apply can't interleave with this export's snapshot.
        let _op_guard = self.op_lock.lock().await;

        self.set_status(BackupStatus::Exporting).await;

        let outcome = async {
            let database_bytes = self.dumper.dump().await.map_err(AppError::Internal)?;
            let config_bytes = tokio::fs::read(&self.config_path).await.map_err(|e| {
                AppError::Internal(anyhow::anyhow!(
                    "failed to read config at {}: {e}",
                    self.config_path.display()
                ))
            })?;
            let secrets = self
                .secret_store
                .backup_contents()
                .await
                .map_err(AppError::Internal)?;
            let schema_version = self
                .dumper
                .current_schema_version()
                .await
                .map_err(AppError::Internal)?;

            let key_count = u32::try_from(secrets.len()).unwrap_or(u32::MAX);
            let manifest = BundleManifest::new(
                self.current_version.clone(),
                schema_version,
                self.host_id.clone(),
                key_count,
            );

            let contents = BundleContents {
                manifest,
                database_bytes,
                config_bytes,
                secrets,
            };

            self.archiver
                .pack(&req.passphrase, contents)
                .await
                .map_err(AppError::Internal)
        }
        .await;

        match &outcome {
            Ok(bytes) => {
                tracing::info!(
                    bytes = bytes.len(),
                    "backup exported: bytes={bytes}",
                    bytes = bytes.len(),
                );
                self.set_status(BackupStatus::Idle).await;
            }
            Err(e) => {
                let reason = e.to_string();
                tracing::warn!(
                    error = %reason,
                    "backup export failed: {reason}",
                );
                self.set_status(BackupStatus::Failed { reason }).await;
            }
        }
        outcome
    }

    async fn preview_import(
        &self,
        bundle: Vec<u8>,
        passphrase: String,
    ) -> Result<RestorePreviewResponse, AppError> {
        auth_context::require_admin()?;
        Self::require_passphrase(&passphrase)?;

        let _op_guard = self.op_lock.lock().await;

        self.set_status(BackupStatus::Importing {
            phase: RestorePhase::Validating,
        })
        .await;

        // Wrap in an `outcome` block so any failure path flips status
        // to `Failed { reason }` and the next `GET /api/backup/status`
        // surfaces the error instead of lying about an in-progress
        // import forever.
        let outcome = async {
            let contents = self
                .archiver
                .unpack(&passphrase, &bundle)
                .await
                .map_err(|e| AppError::BadRequest(format!("bundle could not be decrypted: {e}")))?;

            let (compatible, incompatibility_reason) =
                self.check_compat(&contents.manifest).await?;

            let files_to_replace = vec![
                self.database_path.display().to_string(),
                self.config_path.display().to_string(),
                format!(
                    "{} secret(s) in the configured store",
                    contents.secrets.len()
                ),
            ];

            let preview_token = Uuid::new_v4().to_string();
            {
                let mut pending = self.pending.lock().await;
                let now = Instant::now();
                pending.retain(|_, entry| now.duration_since(entry.created_at) < PREVIEW_TOKEN_TTL);
                pending.insert(
                    preview_token.clone(),
                    PendingImport {
                        contents: contents.clone(),
                        created_at: Instant::now(),
                    },
                );
            }

            Ok::<_, AppError>(RestorePreviewResponse {
                manifest: contents.manifest,
                compatible,
                incompatibility_reason,
                files_to_replace,
                preview_token,
            })
        }
        .await;

        match &outcome {
            Ok(_) => self.set_status(BackupStatus::Idle).await,
            Err(e) => {
                self.set_status(BackupStatus::Failed {
                    reason: e.to_string(),
                })
                .await;
            }
        }
        outcome
    }

    async fn apply_import(&self, req: ApplyImportRequest) -> Result<ApplyImportResponse, AppError> {
        auth_context::require_admin()?;

        let _op_guard = self.op_lock.lock().await;

        let pending_entry = {
            let mut pending = self.pending.lock().await;
            pending.remove(&req.preview_token).ok_or_else(|| {
                AppError::BadRequest(
                    "preview token is unknown or expired — call preview_import again".into(),
                )
            })?
        };
        if Instant::now().duration_since(pending_entry.created_at) >= PREVIEW_TOKEN_TTL {
            return Err(AppError::BadRequest("preview token has expired".into()));
        }

        let (compatible, reason) = self.check_compat(&pending_entry.contents.manifest).await?;
        if !compatible {
            let reason = reason.unwrap_or_else(|| "bundle is not compatible".into());
            self.set_status(BackupStatus::Failed {
                reason: reason.clone(),
            })
            .await;
            return Err(AppError::BadRequest(reason));
        }

        let timestamp = Utc::now().format("%Y%m%dT%H%M%SZ").to_string();

        // Wrap the full flow in an outcome block: on any failure, the
        // inner function attempts rollback before returning `Err`, and
        // out here we flip status to `Failed { reason }` so the API
        // surfaces the failure instead of hanging on `Importing`.
        let outcome = self.run_apply_import(&pending_entry, &timestamp).await;

        match &outcome {
            Ok(_) => {
                let manifest = &pending_entry.contents.manifest;
                tracing::info!(
                    schema_version = manifest.schema_version,
                    host_id = %manifest.host_id,
                    "backup applied — daemon restart required: schema_version={sv}, host_id={host}",
                    sv = manifest.schema_version,
                    host = manifest.host_id,
                );
                self.set_status(BackupStatus::Importing {
                    phase: RestorePhase::Applied,
                })
                .await;
            }
            Err(e) => {
                let reason = e.to_string();
                tracing::warn!(
                    error = %reason,
                    "backup apply_import failed, rollback attempted: {reason}",
                );
                self.set_status(BackupStatus::Failed { reason }).await;
            }
        }

        outcome.map(|snapshots| ApplyImportResponse {
            manifest: pending_entry.contents.manifest,
            snapshots,
        })
    }

    async fn list_snapshots(&self) -> Result<ListSnapshotsResponse, AppError> {
        auth_context::require_admin()?;
        let snapshots = enumerate_snapshots(self.snapshot_dir()).await?;
        Ok(ListSnapshotsResponse { snapshots })
    }

    async fn cleanup_old_snapshots(&self, retain: Duration) -> Result<u32, AppError> {
        auth_context::require_admin()?;
        let snapshots = enumerate_snapshots(self.snapshot_dir()).await?;
        let now = Utc::now();
        let mut deleted: u32 = 0;
        for snap in snapshots {
            let age = now.signed_duration_since(snap.created_at);
            let age = age.to_std().unwrap_or(Duration::ZERO);
            if age >= retain {
                match tokio::fs::remove_file(&snap.path).await {
                    Ok(()) => deleted += 1,
                    Err(e) => {
                        tracing::warn!(
                            path = %snap.path,
                            error = %e,
                            "failed to delete old snapshot: path={path}, error={e}",
                            path = snap.path,
                        );
                    }
                }
            }
        }
        if deleted > 0 {
            tracing::info!(
                deleted,
                "backup snapshot cleanup: deleted={deleted}",
                deleted = deleted,
            );
        }
        Ok(deleted)
    }
}

/// Walk `dir` and collect every `*.bak-*` sibling. Free function so
/// the cleanup runner can exercise the same path-matching rule the
/// service uses for listing.
async fn enumerate_snapshots(dir: &Path) -> Result<Vec<LocalSnapshot>, AppError> {
    let mut out = Vec::new();
    let mut entries = match tokio::fs::read_dir(dir).await {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(out),
        Err(e) => {
            return Err(AppError::Internal(anyhow::anyhow!(
                "failed to read snapshot dir {}: {e}",
                dir.display()
            )));
        }
    };
    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?
    {
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(kind) = classify_snapshot(&name) else {
            continue;
        };
        let metadata = entry
            .metadata()
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;
        let created: DateTime<Utc> = metadata
            .modified()
            .ok()
            .map_or_else(Utc::now, DateTime::<Utc>::from);
        out.push(LocalSnapshot {
            path: entry.path().display().to_string(),
            kind,
            created_at: created,
            size_bytes: metadata.len(),
        });
    }
    Ok(out)
}

/// Match one of our three expected `.bak-*` filename patterns. Returns
/// `None` for any file that doesn't look like something a restore
/// produced, so operators can drop unrelated files into the same
/// directory without confusing the listing.
fn classify_snapshot(name: &str) -> Option<SnapshotKind> {
    if name.starts_with("wardnet.db.bak-") {
        Some(SnapshotKind::Database)
    } else if name.starts_with("wardnet.toml.bak-") {
        Some(SnapshotKind::Config)
    } else if name.starts_with("secrets.bak-")
        && Path::new(name)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("json"))
    {
        Some(SnapshotKind::Keys)
    } else {
        None
    }
}
