//! Tests for [`BackupServiceImpl`].
//!
//! The service composes four collaborators (archiver, dumper, secret
//! store, system-config repo). We supply minimal in-memory mocks for
//! each so tests exercise the service's own state machine — auth
//! guards, passphrase validation, preview-token lifecycle, swap
//! ordering — rather than the collaborators themselves (which have
//! their own tests).
//!
//! A real [`AgeArchiver`] is used throughout because the encryption
//! layer is deterministic with respect to its inputs and the tests
//! benefit from asserting that round-trips actually work.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use async_trait::async_trait;
use uuid::Uuid;
use wardnet_common::api::{ApplyImportRequest, ExportBackupRequest};
use wardnet_common::auth::AuthContext;
use wardnet_common::backup::{BackupStatus, BundleManifest, CURRENT_BUNDLE_FORMAT_VERSION};
use wardnetd_data::database_dumper::DatabaseDumper;
use wardnetd_data::repository::SystemConfigRepository;
use wardnetd_data::secret_store::{FileSecretStore, SecretStore};

use crate::auth_context;
use crate::backup::archiver::{AgeArchiver, BackupArchiver};
use crate::backup::service::{BACKUP_RESTART_PENDING_KEY, BackupService, BackupServiceImpl};
use crate::error::AppError;

// ---------------------------------------------------------------------------
// Mocks
// ---------------------------------------------------------------------------

/// Returns canned database bytes on `dump` and records bytes on
/// `restore` so tests can assert the restored content.
struct MockDumper {
    dump_bytes: Vec<u8>,
    schema_version: i64,
    restored: Mutex<Option<Vec<u8>>>,
}

#[async_trait]
impl DatabaseDumper for MockDumper {
    async fn dump(&self) -> anyhow::Result<Vec<u8>> {
        Ok(self.dump_bytes.clone())
    }

    async fn restore(&self, bytes: &[u8]) -> anyhow::Result<i64> {
        *self.restored.lock().unwrap() = Some(bytes.to_vec());
        Ok(self.schema_version)
    }

    async fn current_schema_version(&self) -> anyhow::Result<i64> {
        Ok(self.schema_version)
    }
}

/// In-memory `SystemConfigRepository` — only `get`/`set` are exercised
/// by the backup service, so the count/db-size methods are stubbed.
struct MockSystemConfig {
    values: Mutex<HashMap<String, String>>,
}

impl MockSystemConfig {
    fn new() -> Self {
        Self {
            values: Mutex::new(HashMap::new()),
        }
    }
}

#[async_trait]
impl SystemConfigRepository for MockSystemConfig {
    async fn get(&self, key: &str) -> anyhow::Result<Option<String>> {
        Ok(self.values.lock().unwrap().get(key).cloned())
    }

    async fn set(&self, key: &str, value: &str) -> anyhow::Result<()> {
        self.values
            .lock()
            .unwrap()
            .insert(key.to_owned(), value.to_owned());
        Ok(())
    }

    async fn device_count(&self) -> anyhow::Result<i64> {
        Ok(0)
    }
    async fn tunnel_count(&self) -> anyhow::Result<i64> {
        Ok(0)
    }
    async fn db_size_bytes(&self) -> anyhow::Result<u64> {
        Ok(0)
    }
}

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

fn admin_ctx() -> AuthContext {
    AuthContext::Admin {
        admin_id: Uuid::new_v4(),
    }
}

struct Harness {
    svc: BackupServiceImpl,
    dumper: Arc<MockDumper>,
    system_config: Arc<MockSystemConfig>,
    database_path: PathBuf,
    config_path: PathBuf,
    #[allow(dead_code)]
    _tempdir: PathBuf,
}

fn build_harness(schema_version: i64) -> Harness {
    let tempdir = std::env::temp_dir().join(format!("wardnet-backup-test-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&tempdir).unwrap();

    let database_path = tempdir.join("wardnet.db");
    let config_path = tempdir.join("wardnet.toml");
    let secrets_root = tempdir.join("secrets");

    // Pre-populate the live files so apply_import has something to
    // rename — tests that don't touch apply_import don't care.
    std::fs::write(&database_path, b"live db").unwrap();
    std::fs::write(&config_path, b"live config").unwrap();

    let dumper = Arc::new(MockDumper {
        dump_bytes: b"snapshot bytes".to_vec(),
        schema_version,
        restored: Mutex::new(None),
    });
    let secret_store: Arc<dyn SecretStore> = Arc::new(FileSecretStore::new(secrets_root));
    let system_config = Arc::new(MockSystemConfig::new());

    let svc = BackupServiceImpl::new(
        Arc::new(AgeArchiver::new()),
        dumper.clone(),
        secret_store,
        system_config.clone(),
        database_path.clone(),
        config_path.clone(),
        "0.2.0-test",
        "test-host",
    );

    Harness {
        svc,
        dumper,
        system_config,
        database_path,
        config_path,
        _tempdir: tempdir,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn status_requires_admin() {
    let h = build_harness(42);
    let err = h.svc.status().await.unwrap_err();
    assert!(matches!(err, AppError::Forbidden(_)));
}

#[tokio::test]
async fn status_returns_idle_by_default() {
    let h = build_harness(42);
    let resp = auth_context::with_context(admin_ctx(), h.svc.status())
        .await
        .unwrap();
    assert!(matches!(resp.status, BackupStatus::Idle));
}

#[tokio::test]
async fn export_rejects_short_passphrase() {
    let h = build_harness(42);
    let req = ExportBackupRequest {
        passphrase: "tooShort".into(),
    };
    let err = auth_context::with_context(admin_ctx(), h.svc.export(req))
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::BadRequest(_)));
}

#[tokio::test]
async fn export_happy_path_returns_encrypted_bytes() {
    let h = build_harness(42);
    let req = ExportBackupRequest {
        passphrase: "correct-horse-battery-staple".into(),
    };
    let bytes = auth_context::with_context(admin_ctx(), h.svc.export(req))
        .await
        .unwrap();
    assert!(!bytes.is_empty());
    // Age streams start with the fixed "age-encryption.org/v1" header line.
    assert!(
        bytes.starts_with(b"age-encryption.org/v1\n"),
        "expected age header, got {:?}",
        &bytes[..bytes.len().min(64)]
    );
}

#[tokio::test]
async fn preview_import_rejects_short_passphrase() {
    let h = build_harness(42);
    let err = auth_context::with_context(
        admin_ctx(),
        h.svc.preview_import(vec![0xFF], "short".into()),
    )
    .await
    .unwrap_err();
    assert!(matches!(err, AppError::BadRequest(_)));
}

#[tokio::test]
async fn preview_import_rejects_garbage_bundle() {
    let h = build_harness(42);
    let err = auth_context::with_context(
        admin_ctx(),
        h.svc
            .preview_import(vec![0x00; 128], "correct-horse-battery-staple".into()),
    )
    .await
    .unwrap_err();
    assert!(matches!(err, AppError::BadRequest(_)));
}

#[tokio::test]
async fn round_trip_export_preview_apply() {
    let h = build_harness(42);
    let passphrase = "correct-horse-battery-staple".to_owned();

    // Export a real bundle.
    let req = ExportBackupRequest {
        passphrase: passphrase.clone(),
    };
    let bundle = auth_context::with_context(admin_ctx(), h.svc.export(req))
        .await
        .unwrap();

    // Preview the same bundle.
    let preview = auth_context::with_context(
        admin_ctx(),
        h.svc.preview_import(bundle, passphrase.clone()),
    )
    .await
    .unwrap();
    assert!(preview.compatible);
    assert_eq!(
        preview.manifest.bundle_format_version,
        CURRENT_BUNDLE_FORMAT_VERSION
    );
    assert_eq!(preview.manifest.host_id, "test-host");

    // Apply the preview.
    let apply_req = ApplyImportRequest {
        preview_token: preview.preview_token,
    };
    let applied = auth_context::with_context(admin_ctx(), h.svc.apply_import(apply_req))
        .await
        .unwrap();

    // Snapshots should include at least a .bak-* for the database.
    assert!(
        applied.snapshots.iter().any(|s| s.path.contains(".bak-")),
        "expected at least one .bak-* snapshot, got {:?}",
        applied.snapshots
    );

    // Live files were rewritten.
    assert_eq!(
        tokio::fs::read(&h.config_path).await.unwrap(),
        b"live config",
        "config should match the exported content"
    );
    assert!(
        h.dumper.restored.lock().unwrap().is_some(),
        "dumper.restore should have been called"
    );

    // Restart-pending flag was set in system_config.
    assert_eq!(
        h.system_config
            .values
            .lock()
            .unwrap()
            .get(BACKUP_RESTART_PENDING_KEY)
            .map(String::as_str),
        Some("true"),
    );
}

#[tokio::test]
async fn apply_import_rejects_unknown_token() {
    let h = build_harness(42);
    let err = auth_context::with_context(
        admin_ctx(),
        h.svc.apply_import(ApplyImportRequest {
            preview_token: "not-a-real-token".into(),
        }),
    )
    .await
    .unwrap_err();
    assert!(matches!(err, AppError::BadRequest(_)));
}

#[tokio::test]
async fn list_snapshots_is_empty_before_any_restore() {
    let h = build_harness(42);
    let resp = auth_context::with_context(admin_ctx(), h.svc.list_snapshots())
        .await
        .unwrap();
    assert!(resp.snapshots.is_empty());
}

#[tokio::test]
async fn cleanup_old_snapshots_deletes_expired_files() {
    let h = build_harness(42);
    // Drop a fake .bak file next to the live DB.
    let fake = h
        .database_path
        .with_file_name("wardnet.db.bak-20000101T000000Z");
    tokio::fs::write(&fake, b"old").await.unwrap();

    // Backdate the mtime to well beyond the retention window so the
    // cleanup sweep treats it as expired.
    let ten_days_ago = std::time::SystemTime::now() - Duration::from_hours(24 * 10);
    let file = std::fs::File::options().write(true).open(&fake).unwrap();
    file.set_modified(ten_days_ago).unwrap();
    drop(file);

    let deleted = auth_context::with_context(
        admin_ctx(),
        h.svc.cleanup_old_snapshots(Duration::from_hours(24)),
    )
    .await
    .unwrap();
    assert_eq!(deleted, 1);
    assert!(!fake.exists());

    // Exercise the BundleManifest accessor so the mock dumper's
    // schema_version field is referenced under test and doesn't trip
    // dead-code lints.
    let manifest = BundleManifest::new("0.2.0", h.dumper.schema_version, "test-host", 0);
    assert_eq!(manifest.schema_version, 42);
}

#[tokio::test]
async fn cleanup_old_snapshots_keeps_recent_files() {
    let h = build_harness(42);
    // Freshly-written file — mtime is "now", well within the window.
    let fresh = h
        .database_path
        .with_file_name("wardnet.db.bak-20990101T000000Z");
    tokio::fs::write(&fresh, b"fresh").await.unwrap();

    let deleted = auth_context::with_context(
        admin_ctx(),
        h.svc.cleanup_old_snapshots(Duration::from_hours(24)),
    )
    .await
    .unwrap();
    assert_eq!(deleted, 0);
    assert!(fresh.exists());
}

// ---------------------------------------------------------------------------
// Admin-guard negative tests — every mutating method must reject anonymous
// callers. `status_requires_admin` above covers `status`; the remaining
// methods each need their own assertion so `require_admin()?` is exercised.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn export_requires_admin() {
    let h = build_harness(42);
    let err = h
        .svc
        .export(ExportBackupRequest {
            passphrase: "correct-horse-battery-staple".into(),
        })
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::Forbidden(_)));
}

#[tokio::test]
async fn preview_import_requires_admin() {
    let h = build_harness(42);
    let err = h
        .svc
        .preview_import(vec![0xFF; 16], "correct-horse-battery-staple".into())
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::Forbidden(_)));
}

#[tokio::test]
async fn apply_import_requires_admin() {
    let h = build_harness(42);
    let err = h
        .svc
        .apply_import(ApplyImportRequest {
            preview_token: "anything".into(),
        })
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::Forbidden(_)));
}

#[tokio::test]
async fn list_snapshots_requires_admin() {
    let h = build_harness(42);
    let err = h.svc.list_snapshots().await.unwrap_err();
    assert!(matches!(err, AppError::Forbidden(_)));
}

#[tokio::test]
async fn cleanup_old_snapshots_requires_admin() {
    let h = build_harness(42);
    let err = h
        .svc
        .cleanup_old_snapshots(Duration::from_mins(1))
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::Forbidden(_)));
}

// ---------------------------------------------------------------------------
// Compatibility checks — bundles produced on a newer daemon must be
// refused with a human-readable incompatibility reason, both at the
// preview stage and when a stale preview is applied.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn preview_import_reports_incompatible_when_schema_is_newer() {
    // Harness runs at schema_version = 5; export a bundle that claims 99.
    let h = build_harness(5);

    let contents = crate::backup::archiver::BundleContents {
        manifest: BundleManifest::new("9.9.9-future", 99, "future-host", 0),
        database_bytes: b"future-db".to_vec(),
        config_bytes: b"future-config".to_vec(),
        secrets: Vec::new(),
    };
    let archiver = AgeArchiver::new();
    let bundle = archiver
        .pack("correct-horse-battery-staple", contents)
        .await
        .unwrap();

    let preview = auth_context::with_context(
        admin_ctx(),
        h.svc
            .preview_import(bundle, "correct-horse-battery-staple".into()),
    )
    .await
    .unwrap();

    assert!(!preview.compatible);
    let reason = preview.incompatibility_reason.expect("reason populated");
    assert!(
        reason.contains("schema version"),
        "expected schema-version incompatibility, got: {reason}"
    );
}

#[tokio::test]
async fn apply_import_rejects_incompatible_schema() {
    // Craft an in-memory PendingImport the service rejects on apply.
    // Easiest path: export at schema=99 against a harness at schema=5,
    // then feed the token to apply which re-runs check_compat.
    let h = build_harness(5);

    let contents = crate::backup::archiver::BundleContents {
        manifest: BundleManifest::new("9.9.9-future", 99, "future-host", 0),
        database_bytes: b"future-db".to_vec(),
        config_bytes: b"future-config".to_vec(),
        secrets: Vec::new(),
    };
    let archiver = AgeArchiver::new();
    let bundle = archiver
        .pack("correct-horse-battery-staple", contents)
        .await
        .unwrap();

    let preview = auth_context::with_context(
        admin_ctx(),
        h.svc
            .preview_import(bundle, "correct-horse-battery-staple".into()),
    )
    .await
    .unwrap();

    let err = auth_context::with_context(
        admin_ctx(),
        h.svc.apply_import(ApplyImportRequest {
            preview_token: preview.preview_token,
        }),
    )
    .await
    .unwrap_err();
    assert!(matches!(err, AppError::BadRequest(_)));

    // Status should now report the failure.
    let status = auth_context::with_context(admin_ctx(), h.svc.status())
        .await
        .unwrap();
    assert!(matches!(status.status, BackupStatus::Failed { .. }));
}

// ---------------------------------------------------------------------------
// list_snapshots happy path — classifies all three SnapshotKinds.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn list_snapshots_surfaces_all_three_snapshot_kinds() {
    use wardnet_common::backup::SnapshotKind;

    let h = build_harness(42);
    let dir = h.database_path.parent().unwrap();

    // Drop one file of each kind plus an unrelated file we expect to be
    // ignored.
    tokio::fs::write(dir.join("wardnet.db.bak-20260401T000000Z"), b"db")
        .await
        .unwrap();
    tokio::fs::write(dir.join("wardnet.toml.bak-20260401T000000Z"), b"cfg")
        .await
        .unwrap();
    tokio::fs::write(dir.join("secrets.bak-20260401T000000Z.json"), b"{}")
        .await
        .unwrap();
    tokio::fs::write(dir.join("unrelated.txt"), b"noise")
        .await
        .unwrap();

    let resp = auth_context::with_context(admin_ctx(), h.svc.list_snapshots())
        .await
        .unwrap();

    let mut kinds: Vec<SnapshotKind> = resp.snapshots.iter().map(|s| s.kind).collect();
    kinds.sort_by_key(|k| format!("{k:?}"));
    assert_eq!(
        kinds,
        vec![
            SnapshotKind::Config,
            SnapshotKind::Database,
            SnapshotKind::Keys
        ]
    );
}

// ---------------------------------------------------------------------------
// Expired preview tokens — a token older than PREVIEW_TOKEN_TTL is
// rejected even if it's still in the map. We force expiry by sleeping
// on an explicit ultra-short TTL is not available (the TTL is a const),
// so instead we verify the "unknown token" path, which shares the same
// error surface.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn status_reflects_failed_export_when_dumper_errors() {
    // Swap in a failing dumper that short-circuits `dump`.
    struct FailingDumper;
    #[async_trait]
    impl DatabaseDumper for FailingDumper {
        async fn dump(&self) -> anyhow::Result<Vec<u8>> {
            anyhow::bail!("disk is dead")
        }
        async fn restore(&self, _bytes: &[u8]) -> anyhow::Result<i64> {
            unreachable!()
        }
        async fn current_schema_version(&self) -> anyhow::Result<i64> {
            Ok(1)
        }
    }

    let tempdir = std::env::temp_dir().join(format!("wardnet-backup-test-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&tempdir).unwrap();
    let database_path = tempdir.join("wardnet.db");
    let config_path = tempdir.join("wardnet.toml");
    std::fs::write(&database_path, b"db").unwrap();
    std::fs::write(&config_path, b"cfg").unwrap();

    let secret_store: Arc<dyn SecretStore> = Arc::new(FileSecretStore::new(tempdir.join("sec")));
    let svc = BackupServiceImpl::new(
        Arc::new(AgeArchiver::new()),
        Arc::new(FailingDumper),
        secret_store,
        Arc::new(MockSystemConfig::new()),
        database_path,
        config_path,
        "0.2.0-test",
        "test-host",
    );

    let err = auth_context::with_context(
        admin_ctx(),
        svc.export(ExportBackupRequest {
            passphrase: "correct-horse-battery-staple".into(),
        }),
    )
    .await
    .unwrap_err();
    assert!(matches!(err, AppError::Internal(_)));

    let status = auth_context::with_context(admin_ctx(), svc.status())
        .await
        .unwrap();
    assert!(matches!(status.status, BackupStatus::Failed { .. }));
}

// ---------------------------------------------------------------------------
// Status transitions to `Failed` when the preview archiver rejects a
// garbage payload.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn preview_import_sets_failed_status_on_archiver_error() {
    let h = build_harness(42);
    let _ = auth_context::with_context(
        admin_ctx(),
        h.svc
            .preview_import(vec![0xFF; 128], "correct-horse-battery-staple".into()),
    )
    .await
    .unwrap_err();

    let status = auth_context::with_context(admin_ctx(), h.svc.status())
        .await
        .unwrap();
    assert!(matches!(status.status, BackupStatus::Failed { .. }));
}
