//! Tests for `FsBinaryApplier`. Each test generates a fresh minisign
//! keypair, builds an in-memory gzip-tar with the requested entries,
//! and exercises the apply path in a tempdir. No fixtures committed
//! to disk.

use std::io::Cursor;
use std::path::Path;

use flate2::Compression;
use flate2::write::GzEncoder;
use minisign::{KeyPair, sign};
use tempfile::TempDir;

use crate::update::applier::BinaryApplier;
use crate::update::fs_applier::FsBinaryApplier;

/// Build a gzip-compressed tar archive in memory with the given
/// `(name, bytes)` entries at the archive root.
fn make_tarball(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut gz = GzEncoder::new(Vec::new(), Compression::fast());
    {
        let mut tar = tar::Builder::new(&mut gz);
        for (name, bytes) in entries {
            let mut header = tar::Header::new_gnu();
            header.set_size(bytes.len() as u64);
            header.set_mode(0o755);
            header.set_cksum();
            tar.append_data(&mut header, name, *bytes).unwrap();
        }
        tar.finish().unwrap();
    }
    gz.finish().unwrap()
}

/// Generate a minisign keypair and sign `payload`. Returns
/// `(public_key_text, signature_text)`.
fn signed_pair(payload: &[u8]) -> (String, String) {
    let keypair = KeyPair::generate_unencrypted_keypair().expect("keypair");
    let pk_text = keypair.pk.to_box().expect("pk box").into_string();
    let mut reader = Cursor::new(payload);
    let signature_box = sign(
        None,
        &keypair.sk,
        &mut reader,
        Some("test trusted"),
        Some("test untrusted"),
    )
    .expect("sign");
    (pk_text, signature_box.into_string())
}

/// `&'static str` is what `with_postupgrade` expects (the embedded
/// public key has 'static lifetime in production). Tests work around
/// the constraint by leaking each generated key — the leak is bounded
/// by the test process's lifetime.
fn leak(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

#[tokio::test]
async fn valid_postupgrade_artifacts_swap_both() {
    let dir = TempDir::new().unwrap();
    let live_path = dir.path().join("wardnetd");
    let staging = dir.path().join("staging");
    let postupgrade = dir.path().join("postupgrade");

    let postupgrade_bin: &[u8] = b"the migration runner binary bytes";
    let (pk_text, sig_text) = signed_pair(postupgrade_bin);

    let tarball = make_tarball(&[
        ("wardnetd", b"the wardnetd binary bytes"),
        ("wardnet-postupgrade.bin", postupgrade_bin),
        ("wardnet-postupgrade.minisig", sig_text.as_bytes()),
    ]);

    let applier = FsBinaryApplier::new(live_path.clone(), staging)
        .with_postupgrade(postupgrade.clone(), leak(pk_text));

    applier.apply(&tarball).await.expect("apply ok");

    assert_eq!(
        std::fs::read(&live_path).unwrap(),
        b"the wardnetd binary bytes",
    );
    assert_eq!(
        std::fs::read(postupgrade.join("wardnet-postupgrade.bin")).unwrap(),
        postupgrade_bin,
    );
    assert_eq!(
        std::fs::read(postupgrade.join("wardnet-postupgrade.minisig")).unwrap(),
        sig_text.as_bytes(),
    );
}

#[tokio::test]
async fn corrupted_postupgrade_signature_aborts_apply() {
    let dir = TempDir::new().unwrap();
    let live_path = dir.path().join("wardnetd");
    let staging = dir.path().join("staging");
    let postupgrade = dir.path().join("postupgrade");

    // Pre-existing files we expect untouched after the failed apply.
    std::fs::write(&live_path, b"OLD wardnetd").unwrap();
    std::fs::create_dir_all(&postupgrade).unwrap();
    std::fs::write(
        postupgrade.join("wardnet-postupgrade.bin"),
        b"OLD postupgrade bin",
    )
    .unwrap();
    std::fs::write(postupgrade.join("wardnet-postupgrade.minisig"), b"OLD sig").unwrap();

    let postupgrade_bin: &[u8] = b"the migration runner binary bytes";
    let (pk_text, sig_text) = signed_pair(postupgrade_bin);
    let mut tampered = sig_text.into_bytes();
    let target = tampered.len() - 8;
    tampered[target] = if tampered[target] == b'A' { b'B' } else { b'A' };

    let tarball = make_tarball(&[
        ("wardnetd", b"NEW wardnetd"),
        ("wardnet-postupgrade.bin", postupgrade_bin),
        ("wardnet-postupgrade.minisig", &tampered),
    ]);

    let applier = FsBinaryApplier::new(live_path.clone(), staging)
        .with_postupgrade(postupgrade.clone(), leak(pk_text));
    let err = applier.apply(&tarball).await.unwrap_err();
    let chain = format!("{err:#}");
    assert!(
        chain.contains("verification failed") || chain.contains("decode failed"),
        "expected verification failure, got: {chain}"
    );

    // Live binary and on-disk postupgrade artifacts remain at their
    // pre-apply state — no half-finished swap.
    assert_eq!(std::fs::read(&live_path).unwrap(), b"OLD wardnetd");
    assert_eq!(
        std::fs::read(postupgrade.join("wardnet-postupgrade.bin")).unwrap(),
        b"OLD postupgrade bin",
    );
    assert_eq!(
        std::fs::read(postupgrade.join("wardnet-postupgrade.minisig")).unwrap(),
        b"OLD sig",
    );
    assert!(!exists(&dir.path().join("wardnetd.old")));
}

#[tokio::test]
async fn tarball_without_postupgrade_only_swaps_wardnetd() {
    let dir = TempDir::new().unwrap();
    let live_path = dir.path().join("wardnetd");
    let staging = dir.path().join("staging");
    let postupgrade = dir.path().join("postupgrade");

    // Pre-place an existing postupgrade payload to confirm it is left
    // untouched (lenient downgrade tolerance).
    std::fs::create_dir_all(&postupgrade).unwrap();
    std::fs::write(
        postupgrade.join("wardnet-postupgrade.bin"),
        b"existing payload",
    )
    .unwrap();

    let (pk_text, _sig_text) = signed_pair(b"unused");
    let tarball = make_tarball(&[("wardnetd", b"NEW wardnetd")]);

    let applier = FsBinaryApplier::new(live_path.clone(), staging)
        .with_postupgrade(postupgrade.clone(), leak(pk_text));
    applier.apply(&tarball).await.expect("apply ok");

    assert_eq!(std::fs::read(&live_path).unwrap(), b"NEW wardnetd");
    // Existing payload is preserved — applier never touched it.
    assert_eq!(
        std::fs::read(postupgrade.join("wardnet-postupgrade.bin")).unwrap(),
        b"existing payload",
    );
}

#[tokio::test]
async fn tarball_missing_wardnetd_returns_err() {
    let dir = TempDir::new().unwrap();
    let live_path = dir.path().join("wardnetd");
    let staging = dir.path().join("staging");

    let tarball = make_tarball(&[("README", b"unrelated entry")]);

    let applier = FsBinaryApplier::new(live_path.clone(), staging);
    let err = applier.apply(&tarball).await.unwrap_err();
    let chain = format!("{err:#}");
    assert!(
        chain.contains("wardnetd"),
        "expected wardnetd-missing error, got: {chain}"
    );
    assert!(!exists(&live_path));
}

#[tokio::test]
async fn unconfigured_postupgrade_ignores_artifacts_in_tarball() {
    // Backwards-compat: an applier built without `.with_postupgrade(...)`
    // must skip the .bin/.minisig in the tarball entirely, even if
    // they're present. Useful for legacy callers and the in-tree
    // wardnetd-mock when run without a postupgrade fixture path.
    let dir = TempDir::new().unwrap();
    let live_path = dir.path().join("wardnetd");
    let staging = dir.path().join("staging");

    let tarball = make_tarball(&[
        ("wardnetd", b"daemon"),
        ("wardnet-postupgrade.bin", b"would-be-payload"),
        ("wardnet-postupgrade.minisig", b"would-be-signature"),
    ]);

    let applier = FsBinaryApplier::new(live_path.clone(), staging);
    applier.apply(&tarball).await.expect("apply ok");
    assert_eq!(std::fs::read(&live_path).unwrap(), b"daemon");
}

fn exists(p: &Path) -> bool {
    p.exists()
}
