//! Build script for `wctl`.
//!
//! Derives a SemVer-compliant version string from `git describe` and exposes it
//! as the `WARDNET_VERSION` compile-time environment variable.
//!
//! The parsing logic is shared with `wardnetd` via the `build-support/version.rs`
//! file at the workspace root. Tests for the parsing live in
//! `wardnetd/src/tests/version.rs`.

use std::env;
use std::process::Command;

// Pull in the shared version-parsing helpers from the workspace build-support directory.
include!("../../build-support/version.rs");

fn main() {
    // Rerun when the git HEAD or any ref changes.
    println!("cargo:rerun-if-changed=../../../../.git/HEAD");
    println!("cargo:rerun-if-changed=../../../../.git/refs/");
    println!("cargo:rerun-if-changed=../../../../CALVER");
    println!("cargo:rerun-if-env-changed=WARDNET_RELEASE_VERSION_OVERRIDE");

    let version = git_version().unwrap_or_else(cargo_pkg_version);
    println!("cargo:rustc-env=WARDNET_VERSION={version}");

    let release_version = env::var("WARDNET_RELEASE_VERSION_OVERRIDE")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(read_calver);
    println!("cargo:rustc-env=WARDNET_RELEASE_VERSION={release_version}");
}

/// Attempt to derive a version string from `git describe`.
///
/// Returns `None` if git is unavailable or the command fails.
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
