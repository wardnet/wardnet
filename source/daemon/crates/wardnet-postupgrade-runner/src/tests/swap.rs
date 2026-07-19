//! Tests for the privileged swap step. Each test builds an in-memory
//! gzip-tar release tarball, signs it with a freshly-generated minisign
//! keypair, and exercises `swap::run` against tempdir paths.

use std::io::Cursor;

use flate2::Compression;
use flate2::write::GzEncoder;
use minisign::{KeyPair, sign};
use tempfile::TempDir;

use crate::swap::{self, SwapOutcome, SwapPaths};

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

fn paths(dir: &std::path::Path) -> SwapPaths {
    SwapPaths {
        pending_tarball: dir.join("pending.tar.gz"),
        pending_signature: dir.join("pending.tar.gz.minisig"),
        rollback_request: dir.join("rollback.request"),
        live_binary: dir.join("wardnetd"),
    }
}

#[test]
fn no_pending_files_is_noop() {
    let dir = TempDir::new().unwrap();
    let p = paths(dir.path());
    let outcome = swap::run(&p, "untrusted comment: x\nAAAA");
    assert!(matches!(outcome, SwapOutcome::NoOp), "got {outcome:?}");
}

#[test]
fn unreadable_pending_tarball_fails_instead_of_noop() {
    // A staged-but-unreadable tarball (here: the pending path is a
    // directory) must resolve to `Failed`, not `NoOp` — otherwise a
    // botched staging looks like "no update pending" and the host
    // silently boots the old binary with exit code 0.
    let dir = TempDir::new().unwrap();
    let p = paths(dir.path());
    std::fs::write(&p.live_binary, b"OLD wardnetd").unwrap();

    std::fs::create_dir(&p.pending_tarball).unwrap();
    std::fs::write(&p.pending_signature, b"sig").unwrap();

    let outcome = swap::run(&p, "untrusted comment: x\nAAAA");
    let SwapOutcome::Failed(err) = outcome else {
        panic!("expected Failed, got {outcome:?}");
    };
    let chain = format!("{err:#}");
    assert!(
        chain.contains("read pending update artifacts"),
        "expected artifact-read context, got: {chain}"
    );
    assert_eq!(std::fs::read(&p.live_binary).unwrap(), b"OLD wardnetd");
}

#[test]
fn signature_mismatch_fails_and_preserves_live_binary() {
    let dir = TempDir::new().unwrap();
    let p = paths(dir.path());

    std::fs::write(&p.live_binary, b"OLD wardnetd").unwrap();

    let tarball = make_tarball(&[("wardnetd", b"NEW wardnetd")]);
    let (pk_a, _) = signed_pair(&tarball);
    // Sign with a *different* key; runner's embedded key is pk_a.
    let (_, sig_b) = signed_pair(&tarball);
    std::fs::write(&p.pending_tarball, &tarball).unwrap();
    std::fs::write(&p.pending_signature, sig_b).unwrap();

    let outcome = swap::run(&p, &pk_a);
    let SwapOutcome::Failed(err) = outcome else {
        panic!("expected Failed, got {outcome:?}");
    };
    let chain = format!("{err:#}");
    assert!(
        chain.contains("signature verification failed"),
        "expected verify-failed message, got: {chain}"
    );
    assert_eq!(std::fs::read(&p.live_binary).unwrap(), b"OLD wardnetd");
    // Pending files must NOT be cleared on failure — operator may want
    // to inspect and the next boot's runner will retry only when a
    // fresh pair lands, but right now we leave them so the failure is
    // diagnosable.
    assert!(p.pending_tarball.exists());
    assert!(p.pending_signature.exists());
}

#[test]
fn happy_path_swaps_and_preserves_old() {
    let dir = TempDir::new().unwrap();
    let p = paths(dir.path());

    std::fs::write(&p.live_binary, b"OLD wardnetd").unwrap();

    let tarball = make_tarball(&[
        ("wardnetd", b"NEW wardnetd"),
        ("wardnet-postupgrade-runner", b"runner ignored here"),
    ]);
    let (pk, sig) = signed_pair(&tarball);
    std::fs::write(&p.pending_tarball, &tarball).unwrap();
    std::fs::write(&p.pending_signature, sig).unwrap();

    let outcome = swap::run(&p, &pk);
    assert!(
        matches!(outcome, SwapOutcome::Swapped),
        "expected Swapped, got {outcome:?}"
    );

    assert_eq!(std::fs::read(&p.live_binary).unwrap(), b"NEW wardnetd");
    let old = dir.path().join("wardnetd.old");
    assert_eq!(std::fs::read(&old).unwrap(), b"OLD wardnetd");
    // Pending files cleared on success.
    assert!(!p.pending_tarball.exists());
    assert!(!p.pending_signature.exists());
}

#[test]
fn happy_path_with_no_existing_live_binary() {
    // Fresh-install case: the runner can be run against a system that
    // has never had wardnetd before. The swap creates the live binary
    // and skips the .old preservation step.
    let dir = TempDir::new().unwrap();
    let p = paths(dir.path());

    let tarball = make_tarball(&[("wardnetd", b"NEW wardnetd")]);
    let (pk, sig) = signed_pair(&tarball);
    std::fs::write(&p.pending_tarball, &tarball).unwrap();
    std::fs::write(&p.pending_signature, sig).unwrap();

    let outcome = swap::run(&p, &pk);
    assert!(
        matches!(outcome, SwapOutcome::Swapped),
        "expected Swapped, got {outcome:?}"
    );
    assert_eq!(std::fs::read(&p.live_binary).unwrap(), b"NEW wardnetd");
    assert!(!dir.path().join("wardnetd.old").exists());
}

#[test]
fn tarball_without_wardnetd_entry_fails_with_path_in_chain() {
    let dir = TempDir::new().unwrap();
    let p = paths(dir.path());
    std::fs::write(&p.live_binary, b"OLD wardnetd").unwrap();

    let tarball = make_tarball(&[("README", b"unrelated")]);
    let (pk, sig) = signed_pair(&tarball);
    std::fs::write(&p.pending_tarball, &tarball).unwrap();
    std::fs::write(&p.pending_signature, sig).unwrap();

    let outcome = swap::run(&p, &pk);
    let SwapOutcome::Failed(err) = outcome else {
        panic!("expected Failed, got {outcome:?}");
    };
    assert!(
        format!("{err:#}").contains("wardnetd"),
        "expected wardnetd-missing message"
    );
    assert_eq!(std::fs::read(&p.live_binary).unwrap(), b"OLD wardnetd");
}

#[test]
fn rollback_request_renames_old_back_into_place() {
    let dir = TempDir::new().unwrap();
    let p = paths(dir.path());

    std::fs::write(&p.live_binary, b"BAD wardnetd").unwrap();
    std::fs::write(dir.path().join("wardnetd.old"), b"GOOD wardnetd").unwrap();
    std::fs::write(&p.rollback_request, b"").unwrap();

    let outcome = swap::run(&p, "untrusted comment: x\nAAAA");
    assert!(
        matches!(outcome, SwapOutcome::RolledBack),
        "expected RolledBack, got {outcome:?}"
    );
    assert_eq!(std::fs::read(&p.live_binary).unwrap(), b"GOOD wardnetd");
    assert!(!dir.path().join("wardnetd.old").exists());
    assert!(!p.rollback_request.exists(), "request marker cleared");
}

#[test]
fn rollback_request_takes_precedence_over_pending_swap() {
    // If both a pending swap and a rollback request are present, the
    // rollback wins — operator intent overrides automation. Coincident
    // state shouldn't happen but the runner is the only thing the
    // operator can lean on if it does.
    let dir = TempDir::new().unwrap();
    let p = paths(dir.path());

    std::fs::write(&p.live_binary, b"CURRENT wardnetd").unwrap();
    std::fs::write(dir.path().join("wardnetd.old"), b"PREVIOUS wardnetd").unwrap();

    let tarball = make_tarball(&[("wardnetd", b"NEW wardnetd")]);
    let (pk, sig) = signed_pair(&tarball);
    std::fs::write(&p.pending_tarball, &tarball).unwrap();
    std::fs::write(&p.pending_signature, sig).unwrap();
    std::fs::write(&p.rollback_request, b"").unwrap();

    let outcome = swap::run(&p, &pk);
    assert!(
        matches!(outcome, SwapOutcome::RolledBack),
        "expected RolledBack, got {outcome:?}"
    );
    assert_eq!(std::fs::read(&p.live_binary).unwrap(), b"PREVIOUS wardnetd");
    // Pending tarball + sig left in place — the rollback path doesn't
    // touch them. Operator can clear or another swap cycle will replace.
    assert!(p.pending_tarball.exists());
}

#[test]
fn fails_when_live_binary_path_has_no_parent() {
    // `Path::new("/").parent()` is None; an empty path likewise. The
    // `parent()` ok_or_else arm builds an anyhow describing the
    // unusable live path so the operator gets a diagnostic instead of
    // a silent no-op.
    let dir = TempDir::new().unwrap();
    let mut p = paths(dir.path());
    p.live_binary = std::path::PathBuf::from("/");

    let tarball = make_tarball(&[("wardnetd", b"NEW wardnetd")]);
    let (pk, sig) = signed_pair(&tarball);
    std::fs::write(&p.pending_tarball, &tarball).unwrap();
    std::fs::write(&p.pending_signature, sig).unwrap();

    let outcome = swap::run(&p, &pk);
    let SwapOutcome::Failed(err) = outcome else {
        panic!("expected Failed, got {outcome:?}");
    };
    assert!(
        format!("{err:#}").contains("no parent"),
        "expected no-parent message"
    );
}

#[test]
fn happy_path_removes_pre_existing_old_before_renaming() {
    // A previous failed swap can leave `<live>.old` behind. The next
    // successful swap must `remove_file` the stale .old before the
    // rename, otherwise `rename(<live> -> <live>.old)` fails with
    // EEXIST on Linux's strict-mode tmpfs and on file systems where
    // overwrite semantics aren't guaranteed.
    let dir = TempDir::new().unwrap();
    let p = paths(dir.path());

    std::fs::write(&p.live_binary, b"OLD wardnetd").unwrap();
    std::fs::write(dir.path().join("wardnetd.old"), b"STALE old").unwrap();

    let tarball = make_tarball(&[("wardnetd", b"NEW wardnetd")]);
    let (pk, sig) = signed_pair(&tarball);
    std::fs::write(&p.pending_tarball, &tarball).unwrap();
    std::fs::write(&p.pending_signature, sig).unwrap();

    let outcome = swap::run(&p, &pk);
    assert!(
        matches!(outcome, SwapOutcome::Swapped),
        "expected Swapped, got {outcome:?}"
    );
    assert_eq!(std::fs::read(&p.live_binary).unwrap(), b"NEW wardnetd");
    assert_eq!(
        std::fs::read(dir.path().join("wardnetd.old")).unwrap(),
        b"OLD wardnetd",
        ".old should hold the just-replaced binary, not the stale one",
    );
}

#[test]
fn rollback_without_old_binary_clears_request_and_succeeds() {
    let dir = TempDir::new().unwrap();
    let p = paths(dir.path());

    std::fs::write(&p.live_binary, b"CURRENT wardnetd").unwrap();
    std::fs::write(&p.rollback_request, b"").unwrap();

    let outcome = swap::run(&p, "untrusted comment: x\nAAAA");
    assert!(
        matches!(outcome, SwapOutcome::RolledBack),
        "expected RolledBack, got {outcome:?}"
    );
    assert_eq!(std::fs::read(&p.live_binary).unwrap(), b"CURRENT wardnetd");
    assert!(!p.rollback_request.exists());
}
