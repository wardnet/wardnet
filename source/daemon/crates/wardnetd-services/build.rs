//! Build script for `wardnetd-services`.
//!
//! Sets the `WARDNET_VERSION` compile-time environment variable used by
//! `version.rs`.

use std::process::Command;

include!("../../build-support/version.rs");

fn main() {
    println!("cargo:rerun-if-changed=../../../../.git/HEAD");
    println!("cargo:rerun-if-changed=../../../../.git/refs/");

    let version = git_version().unwrap_or_else(cargo_pkg_version);
    println!("cargo:rustc-env=WARDNET_VERSION={version}");
}

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
