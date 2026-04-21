//! Backup and restore — encrypted bundle pack/unpack + the service
//! that composes it with a provider-supplied database dumper and a
//! secret store.
//!
//! ### Traits
//!
//! * [`BackupService`] (in `service`) is the outward-facing admin
//!   surface: `export`, `preview_import`, `apply_import`,
//!   `list_snapshots`, `cleanup_old_snapshots`. Admin-guarded at the
//!   first line of every method.
//! * `BackupArchiver` (in `archiver`) codes the `.wardnet.age` wire
//!   format — `age(gzip(tar(files)))` with a scrypt-derived
//!   passphrase key. Internal to this module.
//!
//! Secret-store bundling lives on the
//! [`SecretStore`](wardnetd_data::secret_store::SecretStore) trait via
//! `backup_contents()` / `restore_from_backup()`, so each provider
//! (`file_system` today, `HashiCorp` Vault / `OnePassword` / AWS
//! Secrets Manager later) controls what it contributes.
//!
//! The database dumper lives in
//! [`wardnetd_data::database_dumper`] alongside the repositories: each
//! storage backend ships its own dumper. The backup service consumes
//! it via `factory.dumper()`.
//!
//! Nothing in this module depends on the HTTP layer — the API in
//! `wardnetd-api` calls into `BackupService`, which composes these
//! pieces.

pub mod archiver;
pub mod runner;
pub mod service;

// Only `BackupService`, `BackupServiceImpl` (used by
// `create_services` for wiring), and the cleanup runner leave this
// module. The archiver and the dumper are implementation details
// that stay internal — the binaries don't need to know about them.
pub use runner::{BackupCleanupRunner, DEFAULT_SNAPSHOT_RETENTION};
pub use service::{BACKUP_RESTART_PENDING_KEY, BackupService, BackupServiceImpl};

#[cfg(test)]
mod tests;
