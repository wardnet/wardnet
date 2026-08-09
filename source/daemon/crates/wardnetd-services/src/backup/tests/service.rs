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
use wardnetd_data::db::restore_pending_marker_path;
use wardnetd_data::repository::SystemConfigRepository;
use wardnetd_data::secret_store::{FileSecretStore, SecretStore};

use crate::auth_context;
use crate::backup::archiver::{AgeArchiver, BackupArchiver, BundleContents};
use crate::backup::service::{BACKUP_RESTART_PENDING_KEY, BackupService, BackupServiceImpl};
use crate::error::AppError;

/// Live config the harness plants on disk.
///
/// Real TOML, because `apply_import` parses it to carry deploy-time-only
/// keys across a restore — and because a fixture that could never exist on
/// a real box would hide that step from every test here. `allow_edge_channel`
/// is set the way `install.sh` writes it for a box installed from the edge
/// channel: opted in, with root, on this machine.
const LIVE_CONFIG: &str = "\
[network]
lan_interface = \"eth0\"

[logging]
level = \"info\"

[update]
allow_edge_channel = true
manifest_base_url = \"https://releases.wardnet.network\"
";

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

    async fn delete(&self, key: &str) -> anyhow::Result<()> {
        self.values.lock().unwrap().remove(key);
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
    build_harness_at(schema_version, ConfigLocation::BesideDatabase)
}

/// Where the live config sits relative to the live database.
///
/// A real install splits them (`/var/lib/wardnet/wardnet.db` and
/// `/etc/wardnet/wardnet.toml`); every test here colocated them, which is
/// precisely why the snapshot walk could miss an entire directory without a
/// single test noticing. [`ConfigLocation::SeparateDirectory`] reproduces
/// production's layout.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ConfigLocation {
    BesideDatabase,
    SeparateDirectory,
}

fn build_harness_at(schema_version: i64, config_location: ConfigLocation) -> Harness {
    let tempdir = std::env::temp_dir().join(format!("wardnet-backup-test-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&tempdir).unwrap();

    let database_path = tempdir.join("wardnet.db");
    let config_path = match config_location {
        ConfigLocation::BesideDatabase => tempdir.join("wardnet.toml"),
        ConfigLocation::SeparateDirectory => {
            let etc = tempdir.join("etc");
            std::fs::create_dir_all(&etc).unwrap();
            etc.join("wardnet.toml")
        }
    };
    let secrets_root = tempdir.join("secrets");

    // Pre-populate the live files so apply_import has something to
    // rename — tests that don't touch apply_import don't care.
    std::fs::write(&database_path, b"live db").unwrap();
    std::fs::write(&config_path, LIVE_CONFIG).unwrap();

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
        tokio::fs::read_to_string(&h.config_path).await.unwrap(),
        LIVE_CONFIG,
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

// NB: `apply_import` writing /etc/wardnet also depends on the systemd unit
// granting `ReadWritePaths=/etc/wardnet` under `ProtectSystem=strict`. That
// invariant is asserted where the unit is embedded —
// `wardnet-postupgrade`'s `up::tests::systemd_unit::
// config_directory_is_writable_for_backup_restore`.

#[tokio::test]
async fn list_snapshots_finds_config_snapshots_in_their_own_directory() {
    // Production layout: the config lives in a different directory from the
    // database, so `apply_import`'s in-place rename leaves the config snapshot
    // somewhere the database's directory walk will never see.
    let h = build_harness_at(42, ConfigLocation::SeparateDirectory);
    assert_ne!(
        h.config_path.parent(),
        h.database_path.parent(),
        "this test is only meaningful when the two directories differ"
    );

    let db_snapshot = h
        .database_path
        .with_file_name("wardnet.db.bak-20990101T000000Z");
    let config_snapshot = h
        .config_path
        .with_file_name("wardnet.toml.bak-20990101T000000Z");
    tokio::fs::write(&db_snapshot, b"db").await.unwrap();
    tokio::fs::write(&config_snapshot, b"cfg").await.unwrap();

    let resp = auth_context::with_context(admin_ctx(), h.svc.list_snapshots())
        .await
        .unwrap();

    let paths: Vec<&str> = resp.snapshots.iter().map(|s| s.path.as_str()).collect();
    assert!(
        paths.contains(&db_snapshot.display().to_string().as_str()),
        "database snapshot missing from {paths:?}"
    );
    assert!(
        paths.contains(&config_snapshot.display().to_string().as_str()),
        "config snapshot missing from {paths:?} - the walk skipped its directory"
    );
}

#[tokio::test]
async fn list_snapshots_does_not_double_report_when_paths_share_a_directory() {
    // The colocated layout must not report each snapshot twice now that the
    // walk visits two (identical) directories.
    let h = build_harness(42);
    let db_snapshot = h
        .database_path
        .with_file_name("wardnet.db.bak-20990101T000000Z");
    tokio::fs::write(&db_snapshot, b"db").await.unwrap();

    let resp = auth_context::with_context(admin_ctx(), h.svc.list_snapshots())
        .await
        .unwrap();
    assert_eq!(resp.snapshots.len(), 1, "got {:?}", resp.snapshots);
}

#[tokio::test]
async fn cleanup_old_snapshots_prunes_config_snapshots_too() {
    // The cleanup runner shares the listing walk, so a directory the walk
    // skipped was never swept — those snapshots accumulated forever.
    let h = build_harness_at(42, ConfigLocation::SeparateDirectory);
    let config_snapshot = h
        .config_path
        .with_file_name("wardnet.toml.bak-20000101T000000Z");
    tokio::fs::write(&config_snapshot, b"old").await.unwrap();

    let ten_days_ago = std::time::SystemTime::now() - Duration::from_hours(24 * 10);
    let file = std::fs::File::options()
        .write(true)
        .open(&config_snapshot)
        .unwrap();
    file.set_modified(ten_days_ago).unwrap();
    drop(file);

    let deleted = auth_context::with_context(
        admin_ctx(),
        h.svc.cleanup_old_snapshots(Duration::from_hours(24)),
    )
    .await
    .unwrap();
    assert_eq!(deleted, 1);
    assert!(!config_snapshot.exists());
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
async fn apply_import_rolls_back_and_leaves_no_restore_marker_on_failure() {
    // When the install fails mid-apply, run_apply_import must roll the
    // live database back and not leave a restore-pending marker behind
    // (the marker would make the next boot discard a legitimate WAL).
    struct RestoreFailsDumper {
        dump_bytes: Vec<u8>,
        schema_version: i64,
    }
    #[async_trait]
    impl DatabaseDumper for RestoreFailsDumper {
        async fn dump(&self) -> anyhow::Result<Vec<u8>> {
            Ok(self.dump_bytes.clone())
        }
        async fn restore(&self, _bytes: &[u8]) -> anyhow::Result<i64> {
            anyhow::bail!("restore boom")
        }
        async fn current_schema_version(&self) -> anyhow::Result<i64> {
            Ok(self.schema_version)
        }
    }

    let tempdir = std::env::temp_dir().join(format!("wardnet-backup-test-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&tempdir).unwrap();
    let database_path = tempdir.join("wardnet.db");
    let config_path = tempdir.join("wardnet.toml");
    std::fs::write(&database_path, b"live db").unwrap();
    std::fs::write(&config_path, LIVE_CONFIG).unwrap();

    let secret_store: Arc<dyn SecretStore> = Arc::new(FileSecretStore::new(tempdir.join("sec")));
    let svc = BackupServiceImpl::new(
        Arc::new(AgeArchiver::new()),
        Arc::new(RestoreFailsDumper {
            dump_bytes: b"snapshot bytes".to_vec(),
            schema_version: 42,
        }),
        secret_store,
        Arc::new(MockSystemConfig::new()),
        database_path.clone(),
        config_path.clone(),
        "0.2.0-test",
        "test-host",
    );

    let passphrase = "correct-horse-battery-staple".to_owned();
    let bundle = auth_context::with_context(
        admin_ctx(),
        svc.export(ExportBackupRequest {
            passphrase: passphrase.clone(),
        }),
    )
    .await
    .unwrap();
    let preview = auth_context::with_context(admin_ctx(), svc.preview_import(bundle, passphrase))
        .await
        .unwrap();

    let err = auth_context::with_context(
        admin_ctx(),
        svc.apply_import(ApplyImportRequest {
            preview_token: preview.preview_token,
        }),
    )
    .await
    .unwrap_err();
    assert!(matches!(err, AppError::Internal(_)));

    // Rollback restored the original database, and no restore marker
    // survives to mislead the next startup.
    assert_eq!(
        tokio::fs::read(&database_path).await.unwrap(),
        b"live db",
        "rollback should restore the original database"
    );
    assert!(
        !restore_pending_marker_path(&database_path).exists(),
        "no restore-pending marker should remain after a rolled-back apply"
    );
}

// ---------------------------------------------------------------------------
// Deploy-time-only config keys (issue #1112)
//
// Restore writes /etc/wardnet/wardnet.toml, and the systemd unit grants
// ReadWritePaths=/etc/wardnet so the write lands. An admin session can
// therefore export a bundle, edit the TOML it contains (the passphrase is
// theirs), and restore it — which would otherwise be a way around the
// `[update] allow_edge_channel` gate that ADR-0023 says takes root *on the
// box*. The classification itself is tested in wardnet-common; these tests
// pin that apply_import actually applies it.
// ---------------------------------------------------------------------------

/// Pack a bundle whose config is `config` rather than the live machine's —
/// what an admin gets by unpacking their own export, editing the TOML, and
/// repacking it.
async fn bundle_with_config(config: &str, passphrase: &str, schema_version: i64) -> Vec<u8> {
    AgeArchiver::new()
        .pack(
            passphrase,
            BundleContents {
                manifest: BundleManifest::new("0.2.0-test", schema_version, "test-host", 0),
                database_bytes: b"snapshot bytes".to_vec(),
                config_bytes: config.as_bytes().to_vec(),
                secrets: Vec::new(),
            },
        )
        .await
        .unwrap()
}

/// Preview and apply `bundle`, returning the config left on disk.
async fn apply_and_read_config(h: &Harness, bundle: Vec<u8>, passphrase: &str) -> String {
    let preview = auth_context::with_context(
        admin_ctx(),
        h.svc.preview_import(bundle, passphrase.to_owned()),
    )
    .await
    .unwrap();
    auth_context::with_context(
        admin_ctx(),
        h.svc.apply_import(ApplyImportRequest {
            preview_token: preview.preview_token,
        }),
    )
    .await
    .unwrap();
    tokio::fs::read_to_string(&h.config_path).await.unwrap()
}

#[tokio::test]
async fn apply_import_keeps_the_live_value_of_deploy_time_keys() {
    let h = build_harness(42);
    let passphrase = "correct-horse-battery-staple";

    // The live box has the gate open and points at the real release server.
    // The bundle tries to flip both, plus repoint the secret store.
    let bundle = bundle_with_config(
        "[network]\nlan_interface = \"eth0\"\n\n\
         [update]\nallow_edge_channel = false\nmanifest_base_url = \"https://attacker.example\"\n\n\
         [secret_store]\nprovider = \"file_system\"\npath = \"/tmp/attacker\"\n",
        passphrase,
        42,
    )
    .await;

    let written: toml::Table = apply_and_read_config(&h, bundle, passphrase)
        .await
        .parse()
        .unwrap();

    assert_eq!(
        written["update"]["allow_edge_channel"].as_bool(),
        Some(true),
        "a bundle must not change the edge gate",
    );
    assert_eq!(
        written["update"]["manifest_base_url"].as_str(),
        Some("https://releases.wardnet.network"),
        "a bundle must not repoint the update source",
    );
    assert!(
        written.get("secret_store").is_none(),
        "the live machine has no [secret_store], so the bundle's must not survive: {written:?}",
    );
}

#[tokio::test]
async fn apply_import_cannot_open_the_edge_gate_on_a_box_that_never_had_it() {
    // The direction that matters: the live config leaves the gate at its
    // default (absent), and the bundle tries to open it.
    let h = build_harness(42);
    let passphrase = "correct-horse-battery-staple";
    tokio::fs::write(&h.config_path, "[logging]\nlevel = \"info\"\n")
        .await
        .unwrap();

    let bundle = bundle_with_config(
        "[logging]\nlevel = \"info\"\n\n[update]\nallow_edge_channel = true\n",
        passphrase,
        42,
    )
    .await;

    let written: toml::Table = apply_and_read_config(&h, bundle, passphrase)
        .await
        .parse()
        .unwrap();
    assert!(
        written.get("update").is_none(),
        "the bundle's [update] section must not survive the restore: {written:?}",
    );
}

#[tokio::test]
async fn apply_import_still_restores_ordinary_config_from_the_bundle() {
    let h = build_harness(42);
    let passphrase = "correct-horse-battery-staple";

    let bundle = bundle_with_config(
        "[logging]\nlevel = \"debug\"\n\n[detection]\nenabled = false\n",
        passphrase,
        42,
    )
    .await;

    let written: toml::Table = apply_and_read_config(&h, bundle, passphrase)
        .await
        .parse()
        .unwrap();
    assert_eq!(written["logging"]["level"].as_str(), Some("debug"));
    assert_eq!(written["detection"]["enabled"].as_bool(), Some(false));
}

#[tokio::test]
async fn preview_import_reports_a_config_this_daemon_cannot_load() {
    // A restore is followed by a mandatory restart, so a bundle carrying a
    // config the daemon cannot parse would take the box down with no UI left
    // to fix it from. The operator has to learn that at preview, while
    // nothing is at stake — not after committing to the apply.
    let h = build_harness(42);
    let passphrase = "correct-horse-battery-staple";

    for broken in [
        "this is not toml",
        "[from_the_future]\nknob = 1\n",
        "[server]\nport = \"not a number\"\n",
    ] {
        let bundle = bundle_with_config(broken, passphrase, 42).await;
        let preview = auth_context::with_context(
            admin_ctx(),
            h.svc.preview_import(bundle, passphrase.to_owned()),
        )
        .await
        .unwrap();

        assert!(!preview.compatible, "{broken:?} previewed as compatible");
        let reason = preview.incompatibility_reason.expect("reason populated");
        assert!(
            reason.contains("config"),
            "reason should name the config: {reason}",
        );
        // A newer release can add a config field without touching the bundle
        // format or the DB schema, so this branch also catches version gaps
        // the manifest checks above cannot see. Say what to do about it.
        assert!(
            reason.contains("upgrade the daemon"),
            "reason should name the remedy: {reason}",
        );
    }
}

#[tokio::test]
async fn apply_import_rejects_a_bundle_whose_config_is_not_toml() {
    // Belt and braces behind the preview check: apply re-runs compat, and
    // the merge runs before anything on disk is touched, so an unusable
    // config fails the restore outright rather than half-applying it.
    let h = build_harness(42);
    let passphrase = "correct-horse-battery-staple";
    let bundle = bundle_with_config("this is not toml", passphrase, 42).await;

    let preview = auth_context::with_context(
        admin_ctx(),
        h.svc.preview_import(bundle, passphrase.to_owned()),
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

    assert!(matches!(err, AppError::BadRequest(_)), "got {err:?}");
    assert_eq!(
        tokio::fs::read_to_string(&h.config_path).await.unwrap(),
        LIVE_CONFIG,
        "the live config should be untouched",
    );
    assert_eq!(
        tokio::fs::read(&h.database_path).await.unwrap(),
        b"live db",
        "the live database should be untouched",
    );
    assert!(
        h.dumper.restored.lock().unwrap().is_none(),
        "the restore should have failed before touching the database",
    );
}

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
