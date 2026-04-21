//! Backup and restore — encrypted bundle pack/unpack plus `SQLite`
//! dump helpers.
//!
//! The subsystem is composed of two narrow traits so each concern can
//! be unit-tested in isolation and mocked from the service layer:
//!
//! * [`BackupArchiver`] — turns bundle inputs into an encrypted
//!   `.wardnet.age` byte stream and back. Concrete impl: [`AgeArchiver`]
//!   (tar + gzip + age passphrase encryption).
//! * [`DatabaseDumper`] — captures a point-in-time `SQLite` snapshot
//!   via `VACUUM INTO` (consistent under concurrent writes) and
//!   restores one in place. Concrete impl: [`SqliteDumper`].
//!
//! Secret-store bundling lives on the
//! [`SecretStore`](wardnetd_data::secret_store::SecretStore) trait
//! itself via `backup_contents()` / `restore_from_backup()`, so each
//! provider (filesystem today, `HashiCorp` Vault / `OnePassword` /
//! AWS Secrets Manager later) controls what it contributes to a
//! bundle.
//!
//! Nothing in this module depends on the HTTP layer — the API lives in
//! `wardnetd-api` and calls into the `BackupService` (added in a later
//! commit) which composes these primitives.

pub mod archiver;
pub mod database_dumper;

pub use archiver::{AgeArchiver, BackupArchiver, BundleContents};
pub use database_dumper::{DatabaseDumper, SqliteDumper};

#[cfg(test)]
mod tests;
