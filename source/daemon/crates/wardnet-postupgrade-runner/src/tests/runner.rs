//! End-to-end tests for `Runner::run` covering all three return
//! variants: `NoPayload` (either file missing), `VerifyFailed`
//! (signature mismatched against embedded pubkey), and `ExecFailed`
//! (verify passes but `fexecve` rejects the bytes as a non-executable
//! image — driven here with a throwaway keypair signing garbage
//! bytes).
//!
//! Successful exec replaces the test process, so the success branch
//! is only reachable from a real systemd unit invocation. The
//! integration test in `source/end2end-tests/daemon/` covers it.

use std::ffi::CString;
use std::io::Cursor;

use minisign::{KeyPair, sign};
use tempfile::TempDir;

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
}
