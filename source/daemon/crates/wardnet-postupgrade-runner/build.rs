// Resolves the minisign public key path used to verify the postupgrade
// payload at runtime, then exposes it to `src/main.rs` via
// `cargo:rustc-env=WARDNET_POSTUPGRADE_PUBKEY=…` so `include_str!`
// can pull the file contents at compile time.
//
// Production builds use the same release key as wardnetd
// (`deploy/keys/wardnet-release.pub`). The end-to-end test image
// generates an ephemeral keypair during the Docker build and points
// `WARDNET_POSTUPGRADE_PUBKEY_PATH` at the test pubkey so the runner
// trusts the locally-signed test payload without leaking the test
// private key into the trust anchor on real Pis.

use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-env-changed=WARDNET_POSTUPGRADE_PUBKEY_PATH");

    let path = std::env::var("WARDNET_POSTUPGRADE_PUBKEY_PATH").unwrap_or_else(|_| {
        // Default: walk back from this crate's manifest dir to the
        // repo root and pick up the same key wardnetd embeds.
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        manifest_dir
            .join("../../../../deploy/keys/wardnet-release.pub")
            .to_string_lossy()
            .into_owned()
    });

    let canonical = std::fs::canonicalize(&path).unwrap_or_else(|e| {
        panic!("WARDNET_POSTUPGRADE_PUBKEY_PATH resolution failed for {path}: {e}")
    });

    println!("cargo:rerun-if-changed={}", canonical.display());
    println!(
        "cargo:rustc-env=WARDNET_POSTUPGRADE_PUBKEY={}",
        canonical.display()
    );
}
