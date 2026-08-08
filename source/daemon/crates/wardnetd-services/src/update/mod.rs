//! Auto-update subsystem: release discovery, verification, install, rollback.
//!
//! The subsystem is layered:
//! - Trait definitions ([`ReleaseSource`], [`ReleaseVerifier`], [`BinaryApplier`])
//!   live in [`release_source`], [`verifier`], [`applier`].
//! - The orchestrator ([`UpdateService`]) lives in [`service`].
//! - The background poller ([`UpdateRunner`]) lives in [`runner`].
//! - Concrete production implementations
//!   ([`HttpsManifestSource`], [`Sha256MinisignVerifier`], [`FsBinaryApplier`])
//!   live in [`manifest`], [`https_verifier`], [`fs_applier`] and are shared
//!   by both the daemon and the mock binary so local dev mirrors prod.

pub mod applier;
pub mod fs_applier;
pub mod https_verifier;
pub mod manifest;
pub mod release_source;
pub mod runner;
pub mod service;
pub mod verifier;

pub use applier::{BinaryApplier, SwapOutcome};
pub use fs_applier::FsBinaryApplier;
pub use https_verifier::Sha256MinisignVerifier;
pub use manifest::HttpsManifestSource;
pub use release_source::ReleaseSource;
pub use runner::UpdateRunner;
pub use service::{UpdateService, UpdateServiceImpl};
pub use verifier::ReleaseVerifier;

/// Minisign public key used to verify release tarballs.
///
/// Baked into the binary at compile time from `deploy/keys/wardnet-release.pub`.
/// Embedding the key is the authenticity anchor: an attacker who hijacks
/// DNS or the manifest server still can't forge a valid signature without
/// the private counterpart, and a daemon built from this commit only trusts
/// this key. Both the daemon and the mock bind their verifier to this
/// constant so local dev exercises the same verification code path as prod.
///
/// The path is resolved by `build.rs`, which honours a
/// `WARDNET_RELEASE_PUBKEY_PATH` override. The only consumer of that
/// override is `source/daemon/Dockerfile.test`, which points it at an
/// ephemeral keypair generated during the image build so the e2e suite
/// can serve a signed release the daemon actually accepts — see
/// `docs/adr/0027-e2e-auto-update-version-skew.md`. Production builds
/// leave it unset and get the release key.
pub const EMBEDDED_PUBLIC_KEY: &str = include_str!(env!("WARDNET_RELEASE_PUBKEY"));

/// Default directory for the wardnet-writable post-upgrade payload
/// (`wardnet-postupgrade.bin` + `.minisig`). `FsBinaryApplier` writes
/// freshly-extracted artifacts here on every successful auto-update;
/// `wardnet-postupgrade-runner` reads them at boot to verify and exec.
/// Wardnet-owned because auto-update runs as the wardnet user; the
/// signature-verification check at boot is what makes that safe.
pub const POSTUPGRADE_DIR: &str = "/var/lib/wardnet/postupgrade";

/// Map a cargo target triple to the short arch name used in release asset
/// filenames. Returns `None` for targets that aren't part of the release
/// matrix.
#[must_use]
pub fn short_arch(target: &str) -> Option<&'static str> {
    match target {
        "aarch64-unknown-linux-gnu" | "aarch64-unknown-linux-musl" => Some("aarch64"),
        "x86_64-unknown-linux-gnu" | "x86_64-unknown-linux-musl" => Some("x86_64"),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
