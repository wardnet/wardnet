//! Tests for `FsBinaryApplier` — the staging side of the runner-
//! mediated swap. Each test generates a fresh minisign keypair, builds
//! an in-memory gzip-tar with the requested entries, and exercises the
//! stage path in a tempdir. The actual privileged rename happens in
//! the runner crate's swap tests; here we only assert that the
//! daemon stages the right artefacts in the right places.

use std::io::Cursor;
use std::path::Path;

use flate2::Compression;
use flate2::write::GzEncoder;
use minisign::{KeyPair, sign};
use tempfile::TempDir;

use crate::update::applier::BinaryApplier;
use crate::update::fs_applier::FsBinaryApplier;

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

fn leak(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

#[tokio::test]
async fn apply_stages_pending_tarball_and_signature() {
    let dir = TempDir::new().unwrap();
    let live_path = dir.path().join("wardnetd");
    let staging = dir.path().join("staging");

    let tarball = make_tarball(&[("wardnetd", b"new wardnetd bytes")]);
    let outer_sig = b"outer tarball signature";

    let applier = FsBinaryApplier::new(live_path.clone(), staging.clone());
    let outcome = applier.apply(&tarball, outer_sig).await.expect("apply ok");

    // Pending tarball + sig land at the runner's expected names.
    assert_eq!(
        std::fs::read(staging.join("wardnetd-pending.tar.gz")).unwrap(),
        tarball,
    );
    assert_eq!(
        std::fs::read(staging.join("wardnetd-pending.tar.gz.minisig")).unwrap(),
        outer_sig,
    );
    // Daemon must NOT have touched the live binary — the runner does
    // that under root on the next restart.
    assert!(!live_path.exists());
    // The previous-binary path the runner *will* produce on swap.
    assert_eq!(outcome.previous_binary, dir.path().join("wardnetd.old"));
}

#[tokio::test]
async fn apply_with_postupgrade_swaps_payload_and_stages_tarball() {
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

    let applier = FsBinaryApplier::new(live_path.clone(), staging.clone())
        .with_postupgrade(postupgrade.clone(), leak(pk_text));

    applier
        .apply(&tarball, b"outer-sig")
        .await
        .expect("apply ok");

    // Postupgrade artefacts swap into place (wardnet-writable, no
    // runner round-trip needed).
    assert_eq!(
        std::fs::read(postupgrade.join("wardnet-postupgrade.bin")).unwrap(),
        postupgrade_bin,
    );
    assert_eq!(
        std::fs::read(postupgrade.join("wardnet-postupgrade.minisig")).unwrap(),
        sig_text.as_bytes(),
    );
    // Pending tarball is staged verbatim (the runner will re-verify
    // the OUTER signature, not the embedded postupgrade one).
    assert_eq!(
        std::fs::read(staging.join("wardnetd-pending.tar.gz")).unwrap(),
        tarball,
    );
    assert!(!live_path.exists());
}

#[tokio::test]
async fn corrupted_postupgrade_signature_aborts_apply() {
    let dir = TempDir::new().unwrap();
    let live_path = dir.path().join("wardnetd");
    let staging = dir.path().join("staging");
    let postupgrade = dir.path().join("postupgrade");

    // Pre-existing files we expect untouched after the failed apply.
    std::fs::create_dir_all(&postupgrade).unwrap();
    std::fs::write(
        postupgrade.join("wardnet-postupgrade.bin"),
        b"OLD postupgrade bin",
    )
    .unwrap();

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

    let applier = FsBinaryApplier::new(live_path.clone(), staging.clone())
        .with_postupgrade(postupgrade.clone(), leak(pk_text));
    let err = applier.apply(&tarball, b"outer-sig").await.unwrap_err();
    let chain = format!("{err:#}");
    assert!(
        chain.contains("verification failed") || chain.contains("decode failed"),
        "expected verification failure, got: {chain}"
    );

    // Pre-apply postupgrade payload + missing pending tarball — the
    // failed pre-flight must not have written anything.
    assert_eq!(
        std::fs::read(postupgrade.join("wardnet-postupgrade.bin")).unwrap(),
        b"OLD postupgrade bin",
    );
    assert!(!staging.join("wardnetd-pending.tar.gz").exists());
}

#[tokio::test]
async fn apply_without_postupgrade_pair_still_stages_tarball() {
    // Lenient downgrade tolerance: a tarball that omits the
    // postupgrade pair stages cleanly. The on-disk postupgrade files
    // are left in place; the runner will swap the wardnetd binary on
    // next boot and the existing payload stays consistent with the
    // new daemon.
    let dir = TempDir::new().unwrap();
    let live_path = dir.path().join("wardnetd");
    let staging = dir.path().join("staging");
    let postupgrade = dir.path().join("postupgrade");

    std::fs::create_dir_all(&postupgrade).unwrap();
    std::fs::write(
        postupgrade.join("wardnet-postupgrade.bin"),
        b"existing payload",
    )
    .unwrap();

    let (pk_text, _sig_text) = signed_pair(b"unused");
    let tarball = make_tarball(&[("wardnetd", b"NEW wardnetd")]);

    let applier = FsBinaryApplier::new(live_path, staging.clone())
        .with_postupgrade(postupgrade.clone(), leak(pk_text));
    applier
        .apply(&tarball, b"outer-sig")
        .await
        .expect("apply ok");

    assert_eq!(
        std::fs::read(postupgrade.join("wardnet-postupgrade.bin")).unwrap(),
        b"existing payload",
    );
    assert!(staging.join("wardnetd-pending.tar.gz").exists());
}

#[tokio::test]
async fn apply_clears_stale_rollback_request() {
    // A pending rollback request from a previous failed install must
    // not race a new install — staging the next forward swap clears
    // the marker so the runner doesn't roll back instead.
    let dir = TempDir::new().unwrap();
    let live_path = dir.path().join("wardnetd");
    let staging = dir.path().join("staging");
    std::fs::create_dir_all(&staging).unwrap();
    std::fs::write(staging.join("wardnetd-rollback.request"), b"").unwrap();

    let tarball = make_tarball(&[("wardnetd", b"NEW wardnetd")]);
    let applier = FsBinaryApplier::new(live_path, staging.clone());
    applier
        .apply(&tarball, b"outer-sig")
        .await
        .expect("apply ok");

    assert!(!staging.join("wardnetd-rollback.request").exists());
    assert!(staging.join("wardnetd-pending.tar.gz").exists());
}

#[tokio::test]
async fn rollback_writes_request_marker_when_old_binary_present() {
    let dir = TempDir::new().unwrap();
    let live_path = dir.path().join("wardnetd");
    let staging = dir.path().join("staging");
    let old_path = dir.path().join("wardnetd.old");

    std::fs::write(&old_path, b"previous").unwrap();

    let applier = FsBinaryApplier::new(live_path, staging.clone());
    applier.rollback().await.expect("rollback request ok");

    assert!(staging.join("wardnetd-rollback.request").exists());
    // Request is empty (marker only, no body).
    assert!(
        std::fs::read(staging.join("wardnetd-rollback.request"))
            .unwrap()
            .is_empty(),
    );
}

#[tokio::test]
async fn rollback_without_old_binary_returns_err() {
    let dir = TempDir::new().unwrap();
    let live_path = dir.path().join("wardnetd");
    let staging = dir.path().join("staging");

    let applier = FsBinaryApplier::new(live_path, staging);
    let err = applier.rollback().await.unwrap_err();
    assert!(
        format!("{err:#}").contains("no previous binary"),
        "expected no-previous-binary error"
    );
}

#[tokio::test]
async fn rollback_available_reflects_old_binary_existence() {
    let dir = TempDir::new().unwrap();
    let live_path = dir.path().join("wardnetd");
    let staging = dir.path().join("staging");
    let applier = FsBinaryApplier::new(live_path.clone(), staging);

    assert!(!applier.rollback_available().await);
    std::fs::write(dir.path().join("wardnetd.old"), b"x").unwrap();
    assert!(applier.rollback_available().await);
}

#[tokio::test]
async fn fs_error_during_apply_carries_failing_path() {
    // Issue #309 regression: a forced fs failure during staging must
    // surface the offending PathBuf in the error chain so operators
    // can diagnose without scraping kernel logs.
    let dir = TempDir::new().unwrap();
    let live_path = dir.path().join("wardnetd");
    // Pre-create staging as a regular file so `create_dir_all` bails
    // with ENOTDIR — a deterministic stand-in for the EACCES the user
    // hit in production.
    let staging = dir.path().join("staging-is-a-file");
    std::fs::write(&staging, b"not a directory").unwrap();

    let tarball = make_tarball(&[("wardnetd", b"daemon")]);
    let applier = FsBinaryApplier::new(live_path, staging.clone());
    let err = applier.apply(&tarball, b"outer-sig").await.unwrap_err();
    let chain = format!("{err:#}");
    assert!(
        chain.contains(staging.to_string_lossy().as_ref()),
        "expected failing path in error chain, got: {chain}"
    );
}

#[allow(dead_code)]
fn exists(p: &Path) -> bool {
    p.exists()
}
