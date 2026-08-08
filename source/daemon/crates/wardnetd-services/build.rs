//! Build script for `wardnetd-services`.
//!
//! Sets the `WARDNET_VERSION` and `WARDNET_RELEASE_VERSION` compile-time
//! environment variables used by `version.rs`, and resolves the minisign
//! public key `update/mod.rs` embeds to verify release tarballs.

use std::env;
use std::path::PathBuf;
use std::process::Command;

include!("../../build-support/version.rs");

fn main() {
    println!("cargo:rerun-if-changed=../../../../.git/HEAD");
    println!("cargo:rerun-if-changed=../../../../.git/refs/");
    println!("cargo:rerun-if-changed=../../../../CALVER");
    println!("cargo:rerun-if-env-changed=WARDNET_VERSION_OVERRIDE");
    println!("cargo:rerun-if-env-changed=WARDNET_RELEASE_VERSION_OVERRIDE");

    let version = env::var("WARDNET_VERSION_OVERRIDE")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| git_version().unwrap_or_else(cargo_pkg_version));
    println!("cargo:rustc-env=WARDNET_VERSION={version}");

    let release_version = env::var("WARDNET_RELEASE_VERSION_OVERRIDE")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(read_calver);
    println!("cargo:rustc-env=WARDNET_RELEASE_VERSION={release_version}");

    emit_release_pubkey();
}

/// Resolve the minisign public key `update/mod.rs` embeds via
/// `include_str!(env!("WARDNET_RELEASE_PUBKEY"))`, and expose its path
/// as a compile-time env var.
///
/// Production builds use `deploy/keys/wardnet-release.pub`. The
/// end-to-end test image generates an ephemeral keypair during the
/// Docker build and points `WARDNET_RELEASE_PUBKEY_PATH` at it, so the
/// e2e daemon trusts the locally-signed test tarball without the test
/// private key ever becoming a trust anchor on real Pis — and without
/// the suite having to switch `[update] require_signature` off, which
/// would skip the verify this seam exists to keep exercising.
///
/// Deliberately mirrors `wardnet-postupgrade-runner`'s build script:
/// the daemon and the runner verify the *same* release tarball against
/// what must be the *same* key, so the two overrides are set together
/// or not at all. The default relative path is identical from both
/// crate roots.
fn emit_release_pubkey() {
    println!("cargo:rerun-if-env-changed=WARDNET_RELEASE_PUBKEY_PATH");

    let path = env::var("WARDNET_RELEASE_PUBKEY_PATH")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| {
            let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
            manifest_dir
                .join("../../../../deploy/keys/wardnet-release.pub")
                .to_string_lossy()
                .into_owned()
        });

    let canonical = std::fs::canonicalize(&path).unwrap_or_else(|e| {
        panic!("WARDNET_RELEASE_PUBKEY_PATH resolution failed for {path}: {e}")
    });

    println!("cargo:rerun-if-changed={}", canonical.display());
    println!(
        "cargo:rustc-env=WARDNET_RELEASE_PUBKEY={}",
        canonical.display()
    );
}

fn git_version() -> Option<String> {
    let output = Command::new("git")
        // `--match v*` is load-bearing: release tags are `v<calver>`, but the
        // repo also carries `edge-v*` tags (ADR-0023) that point at branch
        // commits. Once such a branch merges, an unfiltered `--tags` describe
        // would pick the nearer edge tag, and `parse_git_describe` — which
        // requires a leading `v` — would fall back to a garbage
        // `0.2.0-dev+gedge-v…` version for every dev and CI build.
        .args(["describe", "--tags", "--always", "--dirty", "--match", "v*"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let describe = String::from_utf8(output.stdout).ok()?.trim().to_owned();
    if describe.is_empty() {
        return None;
    }

    Some(parse_git_describe(&describe))
}
