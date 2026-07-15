//! Build script for `wardnetd-services`.
//!
//! Sets the `WARDNET_VERSION` and `WARDNET_RELEASE_VERSION` compile-time
//! environment variables used by `version.rs`.

use std::env;
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
