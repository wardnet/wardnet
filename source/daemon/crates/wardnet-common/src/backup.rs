//! Shared DTOs for the backup and restore subsystem.
//!
//! A **bundle** is a single, self-contained recovery point: the `SQLite` database,
//! the operator `wardnet.toml` config, and every `WireGuard` private key from
//! the secret store — all packed into a gzipped tar and encrypted end-to-end
//! with [age](https://age-encryption.org) in symmetric passphrase mode.
//!
//! These types cross the wire (SDK and web UI consume them) so everything here
//! is `serde`-friendly. There is no unencrypted representation of a bundle in
//! memory or on disk — the archiver always reads/writes the `.wardnet.age`
//! stream directly.
//!
//! ### Version negotiation
//!
//! Bundles carry two version numbers:
//!
//! * `bundle_format_version` — layout of the tar itself (manifest fields, file
//!   names). Starts at `1`. Bumped on any incompatible change to bundle
//!   contents. Import refuses if the bundle's format version is higher than
//!   the running daemon's supported version.
//! * `schema_version` — the highest applied sqlx migration at export time.
//!   Import refuses if the bundle's schema is *higher* than the latest
//!   migration shipped by the running daemon ("upgrade the daemon, then
//!   restore"). A lower schema is fine — the normal `sqlx migrate` path runs
//!   after the swap, same as a fresh install upgrading a stale DB.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Current bundle format version produced by this daemon.
///
/// Bump this constant (and add compatibility handling in the importer) any
/// time the on-disk layout of a bundle changes in a way that older daemons
/// can't read.
pub const CURRENT_BUNDLE_FORMAT_VERSION: u32 = 1;

/// Minimum passphrase length (in characters) enforced server-side on export.
///
/// Short passphrases defeat age's scrypt KDF in practice — the cost factor
/// is configurable but the keyspace has to carry the security.
pub const MIN_PASSPHRASE_LEN: usize = 12;

/// Metadata describing a bundle. Serialised as `manifest.json` inside the tar.
///
/// The manifest is the first entry extracted during validation so we can
/// reject incompatible bundles before touching any daemon state.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
pub struct BundleManifest {
    /// Daemon version (from `WARDNET_VERSION`) that produced the bundle, e.g.
    /// `0.2.0` or `0.2.1-dev.7+gabc1234`. Informational — not used for compat.
    pub wardnet_version: String,
    /// Highest applied sqlx migration version at export time.
    pub schema_version: i64,
    /// UTC timestamp the bundle was created.
    pub created_at: DateTime<Utc>,
    /// Opaque identifier for the source host (hostname or a stable system id).
    /// Surfaced in the restore preview so operators can confirm they're not
    /// restoring a bundle from the wrong machine.
    pub host_id: String,
    /// Bundle layout version. See `CURRENT_BUNDLE_FORMAT_VERSION`.
    pub bundle_format_version: u32,
    /// Number of `WireGuard` key files included in the bundle. Surfaced in the
    /// preview UI so the operator knows exactly what will be overwritten.
    pub key_count: u32,
}

impl BundleManifest {
    /// Create a fresh manifest for export. Callers supply the schema version
    /// (from `sqlx::migrate::Migrator`) and key count (from the secret store).
    #[must_use]
    pub fn new(
        wardnet_version: impl Into<String>,
        schema_version: i64,
        host_id: impl Into<String>,
        key_count: u32,
    ) -> Self {
        Self {
            wardnet_version: wardnet_version.into(),
            schema_version,
            created_at: Utc::now(),
            host_id: host_id.into(),
            bundle_format_version: CURRENT_BUNDLE_FORMAT_VERSION,
            key_count,
        }
    }

    /// True when the running daemon can read this bundle.
    ///
    /// We accept any bundle with a format version *less than or equal to*
    /// `CURRENT_BUNDLE_FORMAT_VERSION` — newer daemons are expected to handle
    /// older layouts via compatibility shims.
    #[must_use]
    pub fn is_format_supported(&self) -> bool {
        self.bundle_format_version <= CURRENT_BUNDLE_FORMAT_VERSION
    }
}

/// Coarse subsystem status surfaced by `GET /api/backup/status` and the web UI.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum BackupStatus {
    /// Nothing happening. The subsystem is ready to export or import.
    #[default]
    Idle,
    /// Bundle is being packed and encrypted for download.
    Exporting,
    /// An import is in progress — `phase` describes where.
    Importing { phase: RestorePhase },
    /// The last import failed; the error is retained until the next operation.
    Failed { reason: String },
}

/// Phase of an in-flight restore. Emitted as progress events so the UI can
/// show a spinner with meaningful status text.
///
/// The restore pipeline is strictly sequential — no phase is ever skipped on
/// the happy path, and a failure preserves the phase it occurred in so the
/// operator can tell *where* the restore broke.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RestorePhase {
    #[default]
    /// No restore in progress.
    Idle,
    /// Decrypting the bundle and parsing the manifest.
    Validating,
    /// Stopping DHCP/DNS/update runners so the DB swap is safe.
    StoppingRunners,
    /// Renaming current `wardnet.db` / `wardnet.toml` / `keys/` to
    /// `.bak-<timestamp>` siblings.
    BackingUp,
    /// Extracting bundle contents into place.
    Extracting,
    /// Running `sqlx migrate` against the restored database so a
    /// lower-schema-version bundle lands cleanly on a newer daemon.
    Migrating,
    /// Restarting runners with the new state.
    RestartingRunners,
    /// Restore completed successfully.
    Applied,
    /// Something went wrong — the `.bak-*` siblings are still on disk for
    /// manual recovery. `reason` holds the operator-facing error message.
    Failed { reason: String },
}

/// Summary of a local `.bak-*` directory left behind by a previous import.
///
/// Retained for 24h by the background cleanup task so operators can manually
/// recover from a bad restore. Surfaced by `GET /api/backup/snapshots`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
pub struct LocalSnapshot {
    /// Fully-qualified path on disk, e.g.
    /// `/var/lib/wardnet/wardnet.db.bak-20260421T143022Z`.
    pub path: String,
    /// Which file the snapshot was taken of (`"database"`, `"config"`,
    /// `"keys"`). Lets the UI group snapshots produced by the same import.
    pub kind: SnapshotKind,
    /// When the snapshot was created.
    pub created_at: DateTime<Utc>,
    /// Total size on disk in bytes (directories are summed recursively).
    pub size_bytes: u64,
}

/// What a [`LocalSnapshot`] is a snapshot of.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SnapshotKind {
    /// `SQLite` database file.
    Database,
    /// `wardnet.toml` operator config.
    Config,
    /// Entire `keys/` directory.
    Keys,
}

impl SnapshotKind {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Database => "database",
            Self::Config => "config",
            Self::Keys => "keys",
        }
    }
}
