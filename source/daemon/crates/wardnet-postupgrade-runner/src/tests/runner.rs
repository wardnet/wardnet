//! End-to-end tests for `Runner::run` covering its return variants:
//! `NoPayload` (either file missing), `ArtifactReadFailed` (staged
//! file present but unreadable), `VerifyFailed` (signature mismatched
//! against embedded pubkey), `ExecFailed` (verify passes but `fexecve`
//! rejects the bytes as a non-executable image — driven here with a
//! throwaway keypair signing garbage bytes), and the swap arms.
//!
//! Successful exec replaces the test process, so the success branch
//! is only reachable from a real systemd unit invocation. The
//! integration test in `source/end2end-tests/daemon/` covers it.

use std::ffi::CString;
use std::io::Cursor;

use flate2::Compression;
use flate2::write::GzEncoder;
use minisign::{KeyPair, sign};
use tempfile::TempDir;

use crate::swap::SwapPaths;
use crate::{RunOutcome, Runner};

/// `&'static str` is what `Runner::public_key` expects. Tests leak
/// the freshly-generated pubkey for the duration of the process.
fn leak(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

/// Generate a keypair, sign `payload`, return `(pk_text, sig_text)`.
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

fn build_runner(dir: &std::path::Path, public_key: &'static str) -> Runner {
    Runner {
        payload_path: dir.join("postupgrade.bin"),
        signature_path: dir.join("postupgrade.minisig"),
        state_path: dir.join("state.json"),
        // Tests that don't exercise the swap step point swap paths
        // into a sub-tempdir whose pending files don't exist; the swap
        // module short-circuits to NoOp without touching live_binary.
        swap_paths: SwapPaths {
            pending_tarball: dir.join("swap-unused/pending.tar.gz"),
            pending_signature: dir.join("swap-unused/pending.tar.gz.minisig"),
            rollback_request: dir.join("swap-unused/rollback.request"),
            live_binary: dir.join("swap-unused/wardnetd"),
        },
        public_key,
    }
}

#[test]
fn no_payload_when_bin_file_missing() {
    let dir = TempDir::new().unwrap();
    // sig present, bin absent
    std::fs::write(dir.path().join("postupgrade.minisig"), b"sig").unwrap();
    let runner = build_runner(dir.path(), "untrusted comment: x\nAAAA");
    let argv = [CString::new("p").unwrap()];
    let outcome = runner.run(&argv);
    assert!(
        matches!(outcome, RunOutcome::NoPayload),
        "expected NoPayload, got {outcome:?}"
    );
}

#[test]
fn no_payload_when_signature_file_missing() {
    let dir = TempDir::new().unwrap();
    // bin present, sig absent
    std::fs::write(dir.path().join("postupgrade.bin"), b"bytes").unwrap();
    let runner = build_runner(dir.path(), "untrusted comment: x\nAAAA");
    let argv = [CString::new("p").unwrap()];
    let outcome = runner.run(&argv);
    assert!(
        matches!(outcome, RunOutcome::NoPayload),
        "expected NoPayload, got {outcome:?}"
    );
}

#[test]
fn unreadable_payload_returns_artifact_read_failed() {
    // Payload path exists but is a directory: unlike the missing-file
    // cases above this must NOT resolve to `NoPayload` — an update is
    // staged and can't be read, so the runner has to exit nonzero
    // instead of reporting a clean no-op.
    let dir = TempDir::new().unwrap();
    std::fs::create_dir(dir.path().join("postupgrade.bin")).unwrap();
    std::fs::write(dir.path().join("postupgrade.minisig"), b"sig").unwrap();

    let runner = build_runner(dir.path(), "untrusted comment: x\nAAAA");
    let argv = [CString::new("p").unwrap()];
    let outcome = runner.run(&argv);
    let RunOutcome::ArtifactReadFailed(err) = outcome else {
        panic!("expected ArtifactReadFailed, got {outcome:?}");
    };
    assert!(
        format!("{err:#}").contains("failed to read payload"),
        "expected payload-read context"
    );

    // The failure is diagnosable from state.json, not only journald.
    let state: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.path().join("state.json")).unwrap()).unwrap();
    assert_eq!(state["last_runner_failure"]["stage"], "artifact-read");
}

#[test]
fn verify_failed_records_state_and_returns_verify_failed() {
    let dir = TempDir::new().unwrap();
    let payload = b"some payload bytes";
    let (pk_a, _sig_a) = signed_pair(payload);
    let (_pk_b, sig_b) = signed_pair(payload);
    // Signature was made with key B but the runner's embedded key
    // is key A — verification rejects.
    std::fs::write(dir.path().join("postupgrade.bin"), payload).unwrap();
    std::fs::write(dir.path().join("postupgrade.minisig"), sig_b).unwrap();

    let runner = build_runner(dir.path(), leak(pk_a));
    let argv = [CString::new("p").unwrap()];
    let outcome = runner.run(&argv);
    let RunOutcome::VerifyFailed(err) = outcome else {
        panic!("expected VerifyFailed, got {outcome:?}");
    };
    let chain = format!("{err:#}");
    assert!(
        chain.contains("verification failed"),
        "expected verification-failure message, got: {chain}"
    );

    // state.json was created best-effort with the failure recorded.
    let state_bytes = std::fs::read(dir.path().join("state.json")).expect("state.json written");
    let state: serde_json::Value = serde_json::from_slice(&state_bytes).unwrap();
    assert!(
        state["last_verification_failure"]["error"].is_string(),
        "expected last_verification_failure to be set, got {state}"
    );
}

#[test]
fn exec_failed_when_verified_payload_is_not_an_executable() {
    let dir = TempDir::new().unwrap();
    // Sign a non-executable payload. Verification passes (the
    // signature matches the embedded pubkey), then `fexecve` fails
    // because the bytes are not a valid executable image — exactly
    // the rare-but-real case the runner's `ExecFailed` arm handles.
    let bogus_payload = b"not-an-elf-file";
    let (pk, sig) = signed_pair(bogus_payload);
    std::fs::write(dir.path().join("postupgrade.bin"), bogus_payload).unwrap();
    std::fs::write(dir.path().join("postupgrade.minisig"), sig).unwrap();

    let runner = build_runner(dir.path(), leak(pk));
    let argv = [CString::new("p").unwrap()];
    let outcome = runner.run(&argv);
    let RunOutcome::ExecFailed(err) = outcome else {
        panic!("expected ExecFailed, got {outcome:?}");
    };
    assert!(
        format!("{err:#}").contains("fexecve failed"),
        "expected fexecve-failure message"
    );

    let state: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.path().join("state.json")).unwrap()).unwrap();
    assert_eq!(state["last_runner_failure"]["stage"], "exec");
}

#[test]
fn with_default_paths_uses_production_constants() {
    let runner = Runner::with_default_paths("untrusted comment: x\nAAAA");
    assert_eq!(
        runner.payload_path,
        std::path::PathBuf::from(crate::DEFAULT_PAYLOAD_PATH)
    );
    assert_eq!(
        runner.signature_path,
        std::path::PathBuf::from(crate::DEFAULT_SIGNATURE_PATH)
    );
    assert_eq!(
        runner.state_path,
        std::path::PathBuf::from(crate::DEFAULT_STATE_PATH)
    );
    assert_eq!(
        runner.swap_paths.live_binary,
        std::path::PathBuf::from(crate::DEFAULT_LIVE_BINARY_PATH)
    );
    assert_eq!(
        runner.swap_paths.pending_tarball,
        std::path::PathBuf::from(crate::swap::DEFAULT_PENDING_TARBALL_PATH)
    );
}

/// Install a process-global tracing subscriber once for the test
/// binary. Without an active subscriber the structured fields inside
/// `tracing::info!` / `warn!` macros expand to no-ops, so coverage
/// tools mark the success-log arms as unhit. We use a global default
/// rather than a thread-local guard because each tracing callsite is
/// registered at first-touch and the registration result sticks for
/// the rest of the process — a guard installed inside one test is
/// raced by the callsite register from another test running in
/// parallel.
fn ensure_global_subscriber() {
    use std::sync::Once;
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::TRACE)
            .with_test_writer()
            .finish();
        let _ = tracing::subscriber::set_global_default(subscriber);
    });
}

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

#[test]
fn swap_succeeds_then_no_payload_returns_no_payload() {
    // Drives the `SwapOutcome::Swapped` arm in `Runner::run`. The
    // postupgrade payload is absent, so after the successful binary
    // swap the run resolves to `NoPayload` — exercising the info-log
    // arm without needing a real fexecve.
    let dir = TempDir::new().unwrap();
    let live = dir.path().join("wardnetd");
    std::fs::write(&live, b"OLD wardnetd").unwrap();

    let tarball = make_tarball(&[("wardnetd", b"NEW wardnetd")]);
    let (pk, sig) = signed_pair(&tarball);
    let pending_tarball = dir.path().join("pending.tar.gz");
    let pending_sig = dir.path().join("pending.tar.gz.minisig");
    std::fs::write(&pending_tarball, &tarball).unwrap();
    std::fs::write(&pending_sig, sig).unwrap();

    let mut runner = build_runner(dir.path(), leak(pk));
    runner.swap_paths = SwapPaths {
        pending_tarball,
        pending_signature: pending_sig,
        rollback_request: dir.path().join("rollback.request"),
        live_binary: live.clone(),
    };
    let argv = [CString::new("p").unwrap()];
    ensure_global_subscriber();
    let outcome = runner.run(&argv);
    // The swap succeeded; no postupgrade payload is staged, so the
    // pipeline short-circuits to NoPayload.
    assert!(
        matches!(outcome, RunOutcome::NoPayload),
        "expected NoPayload after successful swap, got {outcome:?}"
    );
    assert_eq!(std::fs::read(&live).unwrap(), b"NEW wardnetd");
}

#[test]
fn rollback_request_then_no_payload_returns_no_payload() {
    // Drives the `SwapOutcome::RolledBack` arm in `Runner::run`. A
    // request marker is present, the runner clears it, and the rest
    // of the pipeline resolves to `NoPayload`.
    let dir = TempDir::new().unwrap();
    let live = dir.path().join("wardnetd");
    std::fs::write(&live, b"BAD wardnetd").unwrap();
    std::fs::write(dir.path().join("wardnetd.old"), b"GOOD wardnetd").unwrap();
    let rollback_request = dir.path().join("rollback.request");
    std::fs::write(&rollback_request, b"").unwrap();

    let mut runner = build_runner(dir.path(), "untrusted comment: x\nAAAA");
    runner.swap_paths = SwapPaths {
        pending_tarball: dir.path().join("pending.tar.gz"),
        pending_signature: dir.path().join("pending.tar.gz.minisig"),
        rollback_request: rollback_request.clone(),
        live_binary: live.clone(),
    };
    let argv = [CString::new("p").unwrap()];
    ensure_global_subscriber();
    let outcome = runner.run(&argv);
    assert!(
        matches!(outcome, RunOutcome::NoPayload),
        "expected NoPayload after rollback, got {outcome:?}"
    );
    assert_eq!(std::fs::read(&live).unwrap(), b"GOOD wardnetd");
    assert!(!rollback_request.exists());
}

#[test]
fn verify_failed_with_unwritable_state_path_still_returns_verify_failed() {
    // Regression for the "best-effort" state recording in
    // `Runner::run`: a state.json that lives under a non-existent
    // parent directory makes `record_verification_failure` fail. The
    // run must still return `VerifyFailed` and the warn arm must
    // execute (otherwise the journal would lose the failure mode).
    let dir = TempDir::new().unwrap();
    let payload = b"some payload bytes";
    let (pk_a, _) = signed_pair(payload);
    let (_, sig_b) = signed_pair(payload);
    std::fs::write(dir.path().join("postupgrade.bin"), payload).unwrap();
    std::fs::write(dir.path().join("postupgrade.minisig"), sig_b).unwrap();

    let mut runner = build_runner(dir.path(), leak(pk_a));
    // Point state.json at a path whose parent component is a regular
    // file — `record_verification_failure` calls `create_dir_all`
    // which fails with ENOTDIR.
    let blocker = dir.path().join("blocker");
    std::fs::write(&blocker, b"not a dir").unwrap();
    runner.state_path = blocker.join("state.json");

    let argv = [CString::new("p").unwrap()];
    ensure_global_subscriber();
    let outcome = runner.run(&argv);
    assert!(
        matches!(outcome, RunOutcome::VerifyFailed(_)),
        "expected VerifyFailed despite state-write failure, got {outcome:?}"
    );
}

#[test]
fn swap_failure_short_circuits_run() {
    // When the pending tarball is present but its signature doesn't
    // verify, `Runner::run` must return `SwapFailed` without touching
    // the migration payload — gating wardnetd start on a clean swap.
    let dir = TempDir::new().unwrap();
    let pending = dir.path().join("pending.tar.gz");
    let pending_sig = dir.path().join("pending.tar.gz.minisig");
    let live = dir.path().join("wardnetd");
    std::fs::write(&live, b"OLD wardnetd").unwrap();
    let tarball = b"any tarball bytes";
    let (pk_a, _) = signed_pair(tarball);
    let (_, sig_b) = signed_pair(tarball);
    std::fs::write(&pending, tarball).unwrap();
    std::fs::write(&pending_sig, sig_b).unwrap();

    let mut runner = build_runner(dir.path(), leak(pk_a));
    runner.swap_paths = SwapPaths {
        pending_tarball: pending,
        pending_signature: pending_sig,
        rollback_request: dir.path().join("rollback.request"),
        live_binary: live.clone(),
    };

    let argv = [CString::new("p").unwrap()];
    let outcome = runner.run(&argv);
    assert!(
        matches!(outcome, RunOutcome::SwapFailed(_)),
        "expected SwapFailed, got {outcome:?}"
    );
    assert_eq!(
        std::fs::read(&live).unwrap(),
        b"OLD wardnetd",
        "live binary must be untouched on swap failure",
    );

    let state: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.path().join("state.json")).unwrap()).unwrap();
    assert_eq!(state["last_runner_failure"]["stage"], "swap");
}
