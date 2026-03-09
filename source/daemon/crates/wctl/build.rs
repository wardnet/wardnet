//! Build script for `wctl`.
//!
//! Derives a SemVer-compliant version string from `git describe` and exposes it
//! as the `WARDNET_VERSION` compile-time environment variable.
//!
//! The parsing logic is shared with `wardnetd` via the `build-support/version.rs`
//! file at the workspace root. Tests for the parsing live in
//! `wardnetd/src/tests/version.rs`.

use std::process::Command;

// Pull in the shared version-parsing helpers from the workspace build-support directory.
include!("../../build-support/version.rs");

fn main() {
    // Rerun when the git HEAD or any ref changes.
    println!("cargo:rerun-if-changed=../../../../.git/HEAD");
    println!("cargo:rerun-if-changed=../../../../.git/refs/");

    let version = git_version().unwrap_or_else(cargo_pkg_version);
    println!("cargo:rustc-env=WARDNET_VERSION={version}");
}

/// Attempt to derive a version string from `git describe`.
///
/// Returns `None` if git is unavailable or the command fails.
fn git_version() -> Option<String> {
    let output = Command::new("git")
        .args(["describe", "--tags", "--always", "--dirty"])
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
