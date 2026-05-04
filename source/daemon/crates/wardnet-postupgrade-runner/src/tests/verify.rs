//! Tests for `read_artifacts` + `verify`. Each test generates a fresh
//! minisign keypair so fixtures stay hermetic and no private key is
//! checked into the repo.

use std::io::Cursor;

use minisign::{KeyPair, sign};
use tempfile::TempDir;

use crate::verify::{read_artifacts, verify};

/// Helper: generate a keypair, sign `payload`, return
/// `(public_key_text, signature_text)`. Uses an unencrypted keypair —
/// these keys live for the duration of one test.
fn signed(payload: &[u8]) -> (String, String) {
    let keypair = KeyPair::generate_unencrypted_keypair().expect("generate keypair");
    let pk_text = keypair.pk.to_box().expect("pk to_box").into_string();
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

#[test]
fn verify_accepts_valid_signature() {
    let payload = b"a small synthetic payload";
    let (pk_text, sig_text) = signed(payload);
    verify(&pk_text, payload, sig_text.as_bytes()).expect("verify ok");
}

#[test]
fn verify_rejects_byte_flipped_payload() {
    let payload = b"a small synthetic payload";
    let (pk_text, sig_text) = signed(payload);
    let mut tampered = payload.to_vec();
    tampered[0] ^= 0x01;
    let err = verify(&pk_text, &tampered, sig_text.as_bytes())
        .expect_err("tampered payload must not verify");
    let chain = format!("{err:#}");
    assert!(
        chain.contains("verification failed"),
        "expected verification-failure message, got: {chain}"
    );
}

#[test]
fn verify_rejects_byte_flipped_signature() {
    let payload = b"a small synthetic payload";
    let (pk_text, sig_text) = signed(payload);
    // Flip a byte well past the comment header to land inside the
    // base64-encoded signature body.
    let mut tampered = sig_text.into_bytes();
    let target = tampered.len() - 8;
    tampered[target] = if tampered[target] == b'A' { b'B' } else { b'A' };
    let err =
        verify(&pk_text, payload, &tampered).expect_err("byte-flipped signature must not verify");
    // Either decoding bails ("invalid signature format") or
    // verification bails ("signature verification failed"); both
    // outcomes count as a rejection.
    drop(err);
}

#[test]
fn verify_rejects_signature_from_wrong_key() {
    let payload = b"a small synthetic payload";
    let (pk_a, _sig_a) = signed(payload);
    let (_pk_b, sig_b) = signed(payload);
    let err =
        verify(&pk_a, payload, sig_b.as_bytes()).expect_err("wrong-key signature must not verify");
    let chain = format!("{err:#}");
    assert!(
        chain.contains("verification failed"),
        "expected verification-failure message, got: {chain}"
    );
}

#[test]
fn read_artifacts_returns_none_on_missing_payload() {
    let dir = TempDir::new().expect("tempdir");
    // signature exists, payload does not
    std::fs::write(dir.path().join("sig"), b"sig").unwrap();
    let result = read_artifacts(&dir.path().join("missing.bin"), &dir.path().join("sig"));
    assert!(result.is_none());
}

#[test]
fn read_artifacts_returns_none_on_missing_signature() {
    let dir = TempDir::new().expect("tempdir");
    std::fs::write(dir.path().join("payload.bin"), b"payload").unwrap();
    let result = read_artifacts(
        &dir.path().join("payload.bin"),
        &dir.path().join("missing.minisig"),
    );
    assert!(result.is_none());
}

#[test]
fn read_artifacts_returns_bytes_when_both_present() {
    let dir = TempDir::new().expect("tempdir");
    std::fs::write(dir.path().join("payload.bin"), b"payload").unwrap();
    std::fs::write(dir.path().join("payload.minisig"), b"sig").unwrap();
    let (p, s) = read_artifacts(
        &dir.path().join("payload.bin"),
        &dir.path().join("payload.minisig"),
    )
    .expect("both files present");
    assert_eq!(p, b"payload");
    assert_eq!(s, b"sig");
}
