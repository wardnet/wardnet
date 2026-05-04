//! Filesystem [`BinaryApplier`] — extract, stage, rename-swap.

use std::io::Read;
use std::path::PathBuf;

use async_trait::async_trait;
use flate2::read::GzDecoder;
use tokio::task;

use crate::update::applier::{BinaryApplier, SwapOutcome};

/// File names extracted from the release tarball. Anything else
/// (`wardnet-postupgrade-runner`, `wardnet-postupgrade.service`,
/// `LICENSE`, …) is silently ignored during auto-update — the runner
/// is the trust anchor and may only be replaced by `install.sh`.
const ARCHIVE_NAME_DAEMON: &str = "wardnetd";
const ARCHIVE_NAME_POSTUPGRADE_BIN: &str = "wardnet-postupgrade.bin";
const ARCHIVE_NAME_POSTUPGRADE_SIG: &str = "wardnet-postupgrade.minisig";

/// File names placed under [`PostupgradeConfig::dir`] on a successful
/// swap.
const POSTUPGRADE_BIN_FILENAME: &str = "wardnet-postupgrade.bin";
const POSTUPGRADE_SIG_FILENAME: &str = "wardnet-postupgrade.minisig";

/// Optional post-upgrade pipeline: where to place the wardnet-writable
/// payload + signature, and which embedded public key to verify them
/// against. Constructed via [`FsBinaryApplier::with_postupgrade`].
struct PostupgradeConfig {
    dir: PathBuf,
    public_key: &'static str,
}

/// Write-then-rename applier bound to a live binary path and staging dir.
///
/// The live binary and the staging directory must live on the same
/// filesystem so the final `rename(2)` is atomic. `apply()` leaves the
/// previous binary at `<live>.old`, which is the target `rollback()`
/// swaps back into place.
///
/// When configured via [`Self::with_postupgrade`], the applier also
/// extracts `wardnet-postupgrade.bin` + `wardnet-postupgrade.minisig`
/// from the tarball, **pre-flight verifies** the signature against
/// the embedded key before any rename, then swaps them into the
/// post-upgrade dir after `wardnetd`. A pre-flight verification
/// failure aborts the entire apply (wardnetd is not touched).
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
    wardnetd: Option<Vec<u8>>,
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

    fn extract_and_swap(&self, tarball: &[u8]) -> anyhow::Result<SwapOutcome> {
        std::fs::create_dir_all(&self.staging_dir)?;
        let plucked = Self::pluck(tarball)?;

        let wardnetd_bytes = plucked.wardnetd.ok_or_else(|| {
            anyhow::anyhow!("tarball did not contain a `wardnetd` entry at any path")
        })?;

        // Pre-flight verify postupgrade artifacts (when configured AND
        // both files are present). A failed verify aborts the apply
        // before any rename. A missing pair is "lenient" — we leave
        // the on-disk postupgrade alone and only swap wardnetd, which
        // tolerates downgrades to pre-framework releases and CI
        // regressions that briefly omit the artifacts.
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

        // Stage wardnetd (write to staging, set executable). All of
        // this runs before any rename so a failure here also leaves
        // the live tree untouched.
        let staged_daemon = self.staging_dir.join("wardnetd.staged");
        Self::write_file_executable(&staged_daemon, &wardnetd_bytes)?;

        // Swap wardnetd: move live → live.old, staged → live.
        let old_path = self.old_path();
        if self.live_path.exists() {
            if old_path.exists() {
                std::fs::remove_file(&old_path)?;
            }
            std::fs::rename(&self.live_path, &old_path)?;
        }
        std::fs::rename(&staged_daemon, &self.live_path)?;

        // Swap postupgrade artifacts (best-effort: a rename failure
        // here is logged but does not roll back wardnetd, since the
        // existing on-disk postupgrade is still consistent with
        // either the previous or new wardnetd — and OnFailure=
        // wardnetd-rollback covers any downstream daemon crash).
        if let Some((cfg, bin, sig)) = postupgrade_pair
            && let Err(e) = self.swap_postupgrade(cfg, &bin, &sig)
        {
            tracing::warn!(
                error = %e,
                dir = %cfg.dir.display(),
                "postupgrade artifacts not swapped: dir={dir}, error={e}",
                dir = cfg.dir.display(),
            );
        }

        Ok(SwapOutcome {
            previous_binary: old_path,
        })
    }

    /// Single pass over the tarball. Plucks the three known names into
    /// memory; everything else is skipped.
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
                ARCHIVE_NAME_DAEMON => Some(&mut out.wardnetd),
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

    fn write_file_executable(path: &std::path::Path, bytes: &[u8]) -> anyhow::Result<()> {
        let mut out = std::fs::File::create(path)?;
        std::io::Write::write_all(&mut out, bytes)?;
        drop(out);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(path)?.permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(path, perms)?;
        }
        Ok(())
    }

    fn swap_postupgrade(
        &self,
        cfg: &PostupgradeConfig,
        bin: &[u8],
        sig: &[u8],
    ) -> anyhow::Result<()> {
        std::fs::create_dir_all(&cfg.dir)?;

        let staged_bin = self.staging_dir.join("wardnet-postupgrade.bin.staged");
        let staged_sig = self.staging_dir.join("wardnet-postupgrade.minisig.staged");
        std::fs::write(&staged_bin, bin)?;
        std::fs::write(&staged_sig, sig)?;

        // Rename the binary first, then the signature. If the second
        // rename fails after the first succeeds we are left with a
        // mismatched (.bin from this release, .minisig from the
        // previous) pair — the runner will reject that on the next
        // boot, blocking a malformed payload from running, which is
        // exactly the failure mode we want.
        std::fs::rename(&staged_bin, cfg.dir.join(POSTUPGRADE_BIN_FILENAME))?;
        std::fs::rename(&staged_sig, cfg.dir.join(POSTUPGRADE_SIG_FILENAME))?;
        Ok(())
    }
}

#[async_trait]
impl BinaryApplier for FsBinaryApplier {
    async fn apply(&self, tarball: &[u8]) -> anyhow::Result<SwapOutcome> {
        let tarball = tarball.to_vec();
        let live = self.live_path.clone();
        let staging = self.staging_dir.clone();
        let postupgrade = self.postupgrade.as_ref().map(|cfg| PostupgradeConfig {
            dir: cfg.dir.clone(),
            public_key: cfg.public_key,
        });
        task::spawn_blocking(move || {
            let applier = FsBinaryApplier {
                live_path: live,
                staging_dir: staging,
                postupgrade,
            };
            applier.extract_and_swap(&tarball)
        })
        .await?
    }

    async fn rollback(&self) -> anyhow::Result<()> {
        let old = self.old_path();
        if !old.exists() {
            return Err(anyhow::anyhow!(
                "no previous binary at {}",
                old.to_string_lossy()
            ));
        }
        let live = self.live_path.clone();
        task::spawn_blocking(move || -> anyhow::Result<()> {
            if live.exists() {
                std::fs::remove_file(&live)?;
            }
            std::fs::rename(&old, &live)?;
            Ok(())
        })
        .await?
    }

    async fn rollback_available(&self) -> bool {
        self.old_path().exists()
    }
}
