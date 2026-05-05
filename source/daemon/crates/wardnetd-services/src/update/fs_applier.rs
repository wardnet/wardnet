//! Filesystem [`BinaryApplier`] — stage tarball + signature for the
//! root-mediated swap performed by `wardnet-postupgrade-runner`.
//!
//! Why staged-not-direct: the daemon runs as the unprivileged
//! `wardnet` user and cannot rename a binary into `/usr/local/bin/`
//! (the parent dir is `root:root 0755` and `rename(2)` needs write+x
//! on the destination directory). The previous version of this module
//! attempted the rename in-process and hit `EACCES (os error 13)` on
//! every Pi install — see issue #309 for the diagnosis.
//!
//! The current design keeps the daemon's responsibilities limited to
//! what the wardnet user *can* do:
//!
//!   1. Verify the tarball signature against the embedded public key.
//!   2. Pluck `wardnet-postupgrade.bin` / `.minisig` and swap them into
//!      `/var/lib/wardnet/postupgrade/` — wardnet-writable, no privilege
//!      step required.
//!   3. Write the original tarball + signature to
//!      `<staging>/wardnetd-pending.tar.gz{,.minisig}`. The runner picks
//!      these up at the next daemon restart, re-verifies the signature
//!      under root, and renames the extracted `wardnetd` into
//!      `/usr/local/bin/`.
//!
//! `rollback()` writes a request marker the runner consumes the same
//! way. The trust boundary survives because the runner re-verifies
//! every time — the wardnet user can stage anything but cannot get a
//! payload past the embedded public key.

use std::io::Read;
use std::path::PathBuf;

use anyhow::Context;
use async_trait::async_trait;
use flate2::read::GzDecoder;
use tokio::task;

use crate::update::applier::{BinaryApplier, SwapOutcome};

/// File names extracted from the release tarball.
const ARCHIVE_NAME_POSTUPGRADE_BIN: &str = "wardnet-postupgrade.bin";
const ARCHIVE_NAME_POSTUPGRADE_SIG: &str = "wardnet-postupgrade.minisig";

/// File names placed under [`PostupgradeConfig::dir`] on a successful
/// swap.
const POSTUPGRADE_BIN_FILENAME: &str = "wardnet-postupgrade.bin";
const POSTUPGRADE_SIG_FILENAME: &str = "wardnet-postupgrade.minisig";

/// File names the runner reads from the staging dir.
///
/// MUST match `wardnet_postupgrade_runner::swap::DEFAULT_PENDING_*` —
/// kept in sync manually because the daemon and the trust-anchor
/// runner are intentionally separate compilation units.
const PENDING_TARBALL_FILENAME: &str = "wardnetd-pending.tar.gz";
const PENDING_SIGNATURE_FILENAME: &str = "wardnetd-pending.tar.gz.minisig";
const ROLLBACK_REQUEST_FILENAME: &str = "wardnetd-rollback.request";

/// Optional post-upgrade pipeline: where to place the wardnet-writable
/// payload + signature, and which embedded public key to verify them
/// against. Constructed via [`FsBinaryApplier::with_postupgrade`].
struct PostupgradeConfig {
    dir: PathBuf,
    public_key: &'static str,
}

/// Stages a release tarball for the runner-mediated swap.
///
/// Holds the live binary path (used to compute the rollback target's
/// expected `<live>.old` location), the staging directory (where the
/// pending tarball + signature land), and an optional post-upgrade
/// configuration for the migration framework's signed payload.
pub struct FsBinaryApplier {
    live_path: PathBuf,
    staging_dir: PathBuf,
    postupgrade: Option<PostupgradeConfig>,
}

/// Bytes plucked from the tarball, ready to stage. Held in memory
/// before any disk write so verification can run on the exact bytes
/// that will hit disk and a verification failure costs nothing.
#[derive(Default)]
struct Plucked {
    postupgrade_bin: Option<Vec<u8>>,
    postupgrade_sig: Option<Vec<u8>>,
}

impl FsBinaryApplier {
    /// Construct a new applier without post-upgrade support. Tests use
    /// this; production wires `.with_postupgrade(...)` on top.
    #[must_use]
    pub fn new(live_path: PathBuf, staging_dir: PathBuf) -> Self {
        Self {
            live_path,
            staging_dir,
            postupgrade: None,
        }
    }

    /// Extend the applier with the post-upgrade pipeline. `dir` is the
    /// wardnet-writable directory the payload + sig land in (typically
    /// `/var/lib/wardnet/postupgrade/`). `public_key` is the embedded
    /// minisign public key (the same key wardnetd verifies its own
    /// release tarballs against).
    #[must_use]
    pub fn with_postupgrade(mut self, dir: PathBuf, public_key: &'static str) -> Self {
        self.postupgrade = Some(PostupgradeConfig { dir, public_key });
        self
    }

    fn old_path(&self) -> PathBuf {
        let mut p = self.live_path.clone();
        let name = p.file_name().map_or_else(
            || "wardnetd.old".to_owned(),
            |n| format!("{}.old", n.to_string_lossy()),
        );
        p.set_file_name(name);
        p
    }

    fn pending_tarball_path(&self) -> PathBuf {
        self.staging_dir.join(PENDING_TARBALL_FILENAME)
    }

    fn pending_signature_path(&self) -> PathBuf {
        self.staging_dir.join(PENDING_SIGNATURE_FILENAME)
    }

    fn rollback_request_path(&self) -> PathBuf {
        self.staging_dir.join(ROLLBACK_REQUEST_FILENAME)
    }

    fn stage_pending(&self, tarball: &[u8], signature: &[u8]) -> anyhow::Result<()> {
        std::fs::create_dir_all(&self.staging_dir)
            .with_context(|| format!("create_dir_all {}", self.staging_dir.display()))?;

        let plucked = Self::pluck(tarball)?;

        // Pre-flight verify postupgrade artifacts (when configured AND
        // both files are present). A failed verify aborts the apply
        // before any rename. A missing pair is "lenient" — we leave
        // the on-disk postupgrade alone and only stage the wardnetd
        // tarball, which tolerates downgrades to pre-framework releases
        // and CI regressions that briefly omit the artifacts.
        let postupgrade_pair = match (
            self.postupgrade.as_ref(),
            plucked.postupgrade_bin,
            plucked.postupgrade_sig,
        ) {
            (Some(cfg), Some(bin), Some(sig)) => {
                Self::verify_postupgrade(cfg.public_key, &bin, &sig)?;
                Some((cfg, bin, sig))
            }
            _ => None,
        };

        // Stage the runner-bound payload first: tarball + signature
        // adjacent to each other, written atomically (tmp + rename) so
        // a partial write can't be picked up by the runner. The runner
        // re-verifies before swapping, so a torn write would fail
        // verification rather than swap a bad binary.
        Self::write_atomic(&self.pending_tarball_path(), tarball)?;
        Self::write_atomic(&self.pending_signature_path(), signature)?;

        // Postupgrade artifacts are wardnet-writable today, so the
        // daemon swaps them itself — no runner round trip required.
        // A rename failure here is logged but does not abort the
        // pending stage above, since the stale on-disk postupgrade
        // pair stays consistent with either the previous or new
        // wardnetd.
        if let Some((cfg, bin, sig)) = postupgrade_pair
            && let Err(e) = self.swap_postupgrade(cfg, &bin, &sig)
        {
            let dir = cfg.dir.display().to_string();
            let error = format!("{e:#}");
            tracing::warn!(
                %error,
                %dir,
                "postupgrade artifacts not swapped: dir={dir}, error={error}",
            );
        }

        // A pending rollback request from a previous failed install
        // would race the new pending tarball if both stayed on disk.
        // Explicit precedence: a fresh install supersedes a stale
        // rollback request.
        let _ = std::fs::remove_file(self.rollback_request_path());

        Ok(())
    }

    /// Single pass over the tarball. Plucks the postupgrade pair into
    /// memory; everything else is skipped. The wardnetd binary itself
    /// is not extracted by the daemon — the runner does that under
    /// root after re-verifying the tarball signature.
    fn pluck(tarball: &[u8]) -> anyhow::Result<Plucked> {
        let decoder = GzDecoder::new(tarball);
        let mut archive = tar::Archive::new(decoder);
        let mut out = Plucked::default();

        for entry in archive.entries()? {
            let mut entry = entry?;
            let path = entry.path()?.into_owned();
            let name = path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or_default();
            let target: Option<&mut Option<Vec<u8>>> = match name {
                ARCHIVE_NAME_POSTUPGRADE_BIN => Some(&mut out.postupgrade_bin),
                ARCHIVE_NAME_POSTUPGRADE_SIG => Some(&mut out.postupgrade_sig),
                _ => None,
            };
            if let Some(slot) = target {
                let mut buf = Vec::new();
                entry.read_to_end(&mut buf)?;
                *slot = Some(buf);
            }
        }

        Ok(out)
    }

    fn verify_postupgrade(
        public_key_text: &str,
        payload: &[u8],
        signature: &[u8],
    ) -> anyhow::Result<()> {
        let pk = minisign_verify::PublicKey::decode(public_key_text.trim())
            .map_err(|e| anyhow::anyhow!("invalid embedded post-upgrade public key: {e}"))?;
        let sig_text = std::str::from_utf8(signature)
            .map_err(|e| anyhow::anyhow!("post-upgrade signature is not utf-8: {e}"))?;
        let sig = minisign_verify::Signature::decode(sig_text)
            .map_err(|e| anyhow::anyhow!("post-upgrade signature decode failed: {e}"))?;
        pk.verify(payload, &sig, /* allow_legacy */ false)
            .map_err(|e| anyhow::anyhow!("post-upgrade signature verification failed: {e}"))?;
        Ok(())
    }

    /// Atomic write: tmp + rename. The runner only reads files at the
    /// final names, so a crash mid-write leaves a `.tmp` alongside but
    /// no half-written pending tarball.
    fn write_atomic(path: &std::path::Path, bytes: &[u8]) -> anyhow::Result<()> {
        let tmp = path.with_extension(format!(
            "{}.tmp",
            path.extension().and_then(|s| s.to_str()).unwrap_or("tmp")
        ));
        std::fs::write(&tmp, bytes).with_context(|| format!("write {}", tmp.display()))?;
        std::fs::rename(&tmp, path)
            .with_context(|| format!("rename {} -> {}", tmp.display(), path.display()))?;
        Ok(())
    }

    fn swap_postupgrade(
        &self,
        cfg: &PostupgradeConfig,
        bin: &[u8],
        sig: &[u8],
    ) -> anyhow::Result<()> {
        std::fs::create_dir_all(&cfg.dir)
            .with_context(|| format!("create_dir_all {}", cfg.dir.display()))?;

        let staged_bin = self.staging_dir.join("wardnet-postupgrade.bin.staged");
        let staged_sig = self.staging_dir.join("wardnet-postupgrade.minisig.staged");
        std::fs::write(&staged_bin, bin)
            .with_context(|| format!("write {}", staged_bin.display()))?;
        std::fs::write(&staged_sig, sig)
            .with_context(|| format!("write {}", staged_sig.display()))?;

        // Rename the binary first, then the signature. If the second
        // rename fails after the first succeeds we are left with a
        // mismatched (.bin from this release, .minisig from the
        // previous) pair — the runner will reject that on the next
        // boot, blocking a malformed payload from running, which is
        // exactly the failure mode we want.
        let final_bin = cfg.dir.join(POSTUPGRADE_BIN_FILENAME);
        let final_sig = cfg.dir.join(POSTUPGRADE_SIG_FILENAME);
        std::fs::rename(&staged_bin, &final_bin).with_context(|| {
            format!("rename {} -> {}", staged_bin.display(), final_bin.display())
        })?;
        std::fs::rename(&staged_sig, &final_sig).with_context(|| {
            format!("rename {} -> {}", staged_sig.display(), final_sig.display())
        })?;
        Ok(())
    }
}

#[async_trait]
impl BinaryApplier for FsBinaryApplier {
    async fn apply(&self, tarball: &[u8], signature: &[u8]) -> anyhow::Result<SwapOutcome> {
        let tarball = tarball.to_vec();
        let signature = signature.to_vec();
        let live = self.live_path.clone();
        let staging = self.staging_dir.clone();
        let postupgrade = self.postupgrade.as_ref().map(|cfg| PostupgradeConfig {
            dir: cfg.dir.clone(),
            public_key: cfg.public_key,
        });
        let outcome = SwapOutcome {
            previous_binary: self.old_path(),
        };
        task::spawn_blocking(move || {
            let applier = FsBinaryApplier {
                live_path: live,
                staging_dir: staging,
                postupgrade,
            };
            applier.stage_pending(&tarball, &signature)
        })
        .await??;
        Ok(outcome)
    }

    async fn rollback(&self) -> anyhow::Result<()> {
        let old = self.old_path();
        if !old.exists() {
            return Err(anyhow::anyhow!(
                "no previous binary at {}",
                old.to_string_lossy()
            ));
        }
        let staging = self.staging_dir.clone();
        let request = self.rollback_request_path();
        task::spawn_blocking(move || -> anyhow::Result<()> {
            std::fs::create_dir_all(&staging)
                .with_context(|| format!("create_dir_all {}", staging.display()))?;
            // Empty marker — the runner's swap step honours its
            // presence; the body is unused. Atomic write so a
            // mid-write crash doesn't leave an empty file the runner
            // could misinterpret as a successful request.
            FsBinaryApplier::write_atomic(&request, b"")?;
            Ok(())
        })
        .await??;
        Ok(())
    }

    async fn rollback_available(&self) -> bool {
        self.old_path().exists()
    }
}
