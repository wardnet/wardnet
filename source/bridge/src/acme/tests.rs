use chrono::Utc;

use super::parse_not_after;

/// `parse_not_after` reads the leaf's expiry from a real (self-signed) PEM chain.
/// The live ACME order dance is exercised only by the deferred bridge-live
/// integration test, never in `make check-bridge`.
#[test]
fn parse_not_after_reads_leaf_expiry() {
    let key_pair = rcgen::KeyPair::generate().unwrap();
    let params =
        rcgen::CertificateParams::new(vec!["bridge.test.wardnet.network".to_owned()]).unwrap();
    let cert = params.self_signed(&key_pair).unwrap();
    let pem = cert.pem();

    let not_after = parse_not_after(pem.as_bytes()).unwrap();
    assert!(
        not_after > Utc::now(),
        "a freshly self-signed cert should expire in the future"
    );
}

#[test]
fn parse_not_after_rejects_garbage() {
    assert!(parse_not_after(b"not a pem certificate").is_err());
}
