use super::{NONCE_LEN, open, seal};

#[test]
fn seal_open_round_trip() {
    let key = [42u8; 32];
    let plaintext = b"account-credentials + chain + leaf key";
    let (ct, nonce) = seal(&key, plaintext).unwrap();
    assert_eq!(nonce.len(), NONCE_LEN);
    assert_ne!(
        ct.as_slice(),
        plaintext,
        "ciphertext must differ from input"
    );
    let recovered = open(&key, &nonce, &ct).unwrap();
    assert_eq!(recovered, plaintext);
}

#[test]
fn distinct_nonces_per_seal() {
    let key = [1u8; 32];
    let (_, n1) = seal(&key, b"x").unwrap();
    let (_, n2) = seal(&key, b"x").unwrap();
    assert_ne!(n1, n2, "each seal must use a fresh nonce");
}

#[test]
fn wrong_key_fails() {
    let (ct, nonce) = seal(&[1u8; 32], b"secret").unwrap();
    assert!(open(&[2u8; 32], &nonce, &ct).is_err());
}

#[test]
fn tampered_ciphertext_fails() {
    let key = [9u8; 32];
    let (mut ct, nonce) = seal(&key, b"secret").unwrap();
    ct[0] ^= 0xff;
    assert!(open(&key, &nonce, &ct).is_err());
}

#[test]
fn wrong_nonce_length_fails() {
    let (ct, _) = seal(&[0u8; 32], b"secret").unwrap();
    assert!(open(&[0u8; 32], &[0u8; 8], &ct).is_err());
}
