//! Bundle archiver — pack/unpack the encrypted `.wardnet.age` stream.
//!
//! A bundle is `age(gzip(tar(files)))` with a scrypt-derived symmetric
//! key. The wire format is stable: operators need to be able to restore
//! a bundle produced by an older daemon on a newer one, and vice versa,
//! within the same `bundle_format_version`.
//!
//! ### Tar layout
//!
//! ```text
//! manifest.json                    # BundleManifest, parsed first
//! wardnet.db                       # SQLite snapshot
//! wardnet.toml                     # operator configuration
//! secrets/<secret-path>            # one file per entry from SecretStore
//! ```
//!
//! `manifest.json` is always the first entry in the tar so the importer
//! can read it and reject incompatible bundles before committing any
//! other bytes to disk.

use std::io::{Read, Write};

use age::secrecy::SecretString;
use async_trait::async_trait;
use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use wardnet_common::backup::BundleManifest;
use wardnetd_data::secret_store::SecretEntry;

/// Everything that goes into a bundle, in memory.
///
/// Kept as a plain struct (no lifetimes, no `impl Trait`) so the service
/// layer can build one and hand it off to a background task without
/// plumbing references through async boundaries.
#[derive(Debug, Clone)]
pub struct BundleContents {
    /// Manifest that will land at `manifest.json` inside the tar.
    pub manifest: BundleManifest,
    /// Raw `SQLite` snapshot bytes (produced by `DatabaseDumper::dump`).
    pub database_bytes: Vec<u8>,
    /// Raw `wardnet.toml` bytes.
    pub config_bytes: Vec<u8>,
    /// Whatever the active `SecretStore` decided to ship in its
    /// backup. For `FileSecretStore` this is every entry under the
    /// store root; for external providers it may be empty.
    pub secrets: Vec<SecretEntry>,
}

/// Encrypted-bundle codec.
///
/// Implementations must round-trip: `unpack(pack(contents)) == contents`
/// up to ordering of the secret list (tar's ordering is an
/// implementation detail; callers should sort on read if they care).
#[async_trait]
pub trait BackupArchiver: Send + Sync {
    /// Produce a fully-encrypted bundle from `contents`.
    ///
    /// Returns the entire `.wardnet.age` byte stream. The result is safe
    /// to stream to a client over HTTP — there's no mutable daemon state
    /// captured in it.
    async fn pack(&self, passphrase: &str, contents: BundleContents) -> anyhow::Result<Vec<u8>>;

    /// Decrypt a bundle and return its contents.
    ///
    /// Fails if the passphrase is wrong, if the bundle isn't valid age
    /// output, if the tar/gzip layers are corrupt, or if `manifest.json`
    /// is missing. Callers typically follow up with
    /// [`BundleManifest::is_format_supported`] before applying.
    async fn unpack(&self, passphrase: &str, bytes: &[u8]) -> anyhow::Result<BundleContents>;
}

/// Reference implementation: `age` passphrase mode over `gzip` over `tar`.
///
/// Stateless — safe to construct once and share via `Arc`.
#[derive(Debug, Default, Clone, Copy)]
pub struct AgeArchiver;

impl AgeArchiver {
    /// Construct a fresh archiver. Cheap; no I/O, no keys.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

/// Standard tar path for the manifest, used as the compat marker when
/// unpacking — if this entry is missing the bundle is rejected
/// regardless of the rest of the layout.
const MANIFEST_PATH: &str = "manifest.json";
/// Standard tar path for the `SQLite` snapshot.
const DATABASE_PATH: &str = "wardnet.db";
/// Standard tar path for the operator config.
const CONFIG_PATH: &str = "wardnet.toml";
/// Path prefix used for every secret-store entry in the tar.
const SECRETS_PREFIX: &str = "secrets/";
/// scrypt work-factor ceiling accepted on decrypt. `age` uses 15 by
/// default on encrypt, and the reference implementation caps reads at
/// 16 out of the box; we bump the cap to 20 so we can tolerate bundles
/// produced on faster hardware without tripping the "work factor too
/// high" guard.
const SCRYPT_MAX_WORK_FACTOR: u8 = 20;

#[async_trait]
impl BackupArchiver for AgeArchiver {
    async fn pack(&self, passphrase: &str, contents: BundleContents) -> anyhow::Result<Vec<u8>> {
        let passphrase = passphrase.to_owned();
        tokio::task::spawn_blocking(move || pack_sync(&passphrase, &contents))
            .await
            .map_err(|e| anyhow::anyhow!("archiver pack panicked: {e}"))?
    }

    async fn unpack(&self, passphrase: &str, bytes: &[u8]) -> anyhow::Result<BundleContents> {
        let passphrase = passphrase.to_owned();
        let bytes = bytes.to_vec();
        tokio::task::spawn_blocking(move || unpack_sync(&passphrase, &bytes))
            .await
            .map_err(|e| anyhow::anyhow!("archiver unpack panicked: {e}"))?
    }
}

/// Synchronous core of [`AgeArchiver::pack`].
fn pack_sync(passphrase: &str, contents: &BundleContents) -> anyhow::Result<Vec<u8>> {
    let manifest_bytes = serde_json::to_vec_pretty(&contents.manifest)
        .map_err(|e| anyhow::anyhow!("failed to serialise bundle manifest: {e}"))?;

    let mut compressed: Vec<u8> = Vec::new();
    {
        let gz = GzEncoder::new(&mut compressed, Compression::default());
        let mut tar = tar::Builder::new(gz);
        tar.mode(tar::HeaderMode::Deterministic);

        append_bytes(&mut tar, MANIFEST_PATH, &manifest_bytes)?;
        append_bytes(&mut tar, DATABASE_PATH, &contents.database_bytes)?;
        append_bytes(&mut tar, CONFIG_PATH, &contents.config_bytes)?;
        for entry in &contents.secrets {
            let path = format!("{SECRETS_PREFIX}{}", entry.path);
            append_bytes(&mut tar, &path, &entry.value)?;
        }

        tar.finish()
            .map_err(|e| anyhow::anyhow!("tar finalisation failed: {e}"))?;
        let gz = tar
            .into_inner()
            .map_err(|e| anyhow::anyhow!("tar into_inner failed: {e}"))?;
        gz.finish()
            .map_err(|e| anyhow::anyhow!("gzip finalisation failed: {e}"))?;
    }

    let passphrase = SecretString::from(passphrase.to_owned());
    let encryptor = age::Encryptor::with_user_passphrase(passphrase);

    let mut encrypted: Vec<u8> = Vec::new();
    let mut writer = encryptor
        .wrap_output(&mut encrypted)
        .map_err(|e| anyhow::anyhow!("age wrap_output failed: {e}"))?;
    writer
        .write_all(&compressed)
        .map_err(|e| anyhow::anyhow!("age write failed: {e}"))?;
    writer
        .finish()
        .map_err(|e| anyhow::anyhow!("age finalisation failed: {e}"))?;

    Ok(encrypted)
}

/// Synchronous core of [`AgeArchiver::unpack`].
fn unpack_sync(passphrase: &str, bytes: &[u8]) -> anyhow::Result<BundleContents> {
    let decryptor = age::Decryptor::new(bytes)
        .map_err(|e| anyhow::anyhow!("bundle is not a valid age stream: {e}"))?;

    let passphrase = SecretString::from(passphrase.to_owned());
    // Raise the scrypt work-factor ceiling so we can still read bundles
    // produced on fast hardware (the reference implementation's default
    // cap rejects them).
    let mut identity = age::scrypt::Identity::new(passphrase);
    identity.set_max_work_factor(SCRYPT_MAX_WORK_FACTOR);
    let identity_ref: &dyn age::Identity = &identity;

    let mut reader = decryptor
        .decrypt(std::iter::once(identity_ref))
        .map_err(|e| anyhow::anyhow!("bundle decryption failed (wrong passphrase?): {e}"))?;

    let mut compressed: Vec<u8> = Vec::new();
    reader
        .read_to_end(&mut compressed)
        .map_err(|e| anyhow::anyhow!("failed to read decrypted bundle: {e}"))?;

    let gz = GzDecoder::new(compressed.as_slice());
    let mut tar = tar::Archive::new(gz);

    let mut manifest: Option<BundleManifest> = None;
    let mut database_bytes: Option<Vec<u8>> = None;
    let mut config_bytes: Option<Vec<u8>> = None;
    let mut secrets: Vec<SecretEntry> = Vec::new();

    for entry in tar
        .entries()
        .map_err(|e| anyhow::anyhow!("tar entry iteration failed: {e}"))?
    {
        let mut entry = entry.map_err(|e| anyhow::anyhow!("tar entry read failed: {e}"))?;
        let path = entry
            .path()
            .map_err(|e| anyhow::anyhow!("tar entry has invalid path: {e}"))?
            .to_string_lossy()
            .into_owned();

        let mut buf = Vec::new();
        entry
            .read_to_end(&mut buf)
            .map_err(|e| anyhow::anyhow!("tar entry payload read failed for {path}: {e}"))?;

        match path.as_str() {
            MANIFEST_PATH => {
                manifest = Some(
                    serde_json::from_slice(&buf)
                        .map_err(|e| anyhow::anyhow!("manifest.json is not valid JSON: {e}"))?,
                );
            }
            DATABASE_PATH => database_bytes = Some(buf),
            CONFIG_PATH => config_bytes = Some(buf),
            p if p.starts_with(SECRETS_PREFIX) => {
                let secret_path = p[SECRETS_PREFIX.len()..].to_owned();
                if secret_path.is_empty() {
                    continue;
                }
                secrets.push(SecretEntry {
                    path: secret_path,
                    value: buf,
                });
            }
            other => {
                tracing::warn!(
                    entry = %other,
                    "backup bundle contained unexpected entry — ignored: entry={other}",
                );
            }
        }
    }

    let manifest = manifest
        .ok_or_else(|| anyhow::anyhow!("bundle is missing manifest.json — not a wardnet backup"))?;
    let database_bytes =
        database_bytes.ok_or_else(|| anyhow::anyhow!("bundle is missing wardnet.db"))?;
    let config_bytes =
        config_bytes.ok_or_else(|| anyhow::anyhow!("bundle is missing wardnet.toml"))?;

    Ok(BundleContents {
        manifest,
        database_bytes,
        config_bytes,
        secrets,
    })
}

/// Write a single in-memory blob into the tar at `path`.
///
/// We set mtime to zero so bundles are byte-reproducible for a given
/// set of inputs — useful when operators diff two exports to confirm
/// a restore round-trips.
fn append_bytes<W: Write>(
    tar: &mut tar::Builder<W>,
    path: &str,
    bytes: &[u8],
) -> anyhow::Result<()> {
    let mut header = tar::Header::new_gnu();
    header.set_size(bytes.len() as u64);
    header.set_mode(0o600);
    header.set_mtime(0);
    header.set_cksum();
    tar.append_data(&mut header, path, bytes)
        .map_err(|e| anyhow::anyhow!("tar append failed for {path}: {e}"))?;
    Ok(())
}
