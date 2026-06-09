use super::{CertResolver, PLACEHOLDER_VERSION};

fn self_signed_pem(fqdn: &str) -> (String, String) {
    let key_pair = rcgen::KeyPair::generate().unwrap();
    let params = rcgen::CertificateParams::new(vec![fqdn.to_owned()]).unwrap();
    let cert = params.self_signed(&key_pair).unwrap();
    (cert.pem(), key_pair.serialize_pem())
}

#[test]
fn placeholder_is_not_provisioned() {
    crate::tls::install_crypto_provider();
    let resolver = CertResolver::with_placeholder().unwrap();
    assert_eq!(resolver.served_version(), PLACEHOLDER_VERSION);
    assert!(!resolver.is_provisioned());
    // a usable config is available even pre-provisioning
    let _ = resolver.current();
}

#[test]
fn install_swaps_and_marks_provisioned() {
    crate::tls::install_crypto_provider();
    let resolver = CertResolver::with_placeholder().unwrap();
    let (chain, key) = self_signed_pem("bridge.svc.prod.use1.wardnet.network");

    resolver
        .install(chain.as_bytes(), key.as_bytes(), 1)
        .unwrap();

    assert_eq!(resolver.served_version(), 1);
    assert!(resolver.is_provisioned());
}

#[test]
fn install_rejects_garbage_pem() {
    crate::tls::install_crypto_provider();
    let resolver = CertResolver::with_placeholder().unwrap();
    assert!(resolver.install(b"not pem", b"not pem", 1).is_err());
    // remains on the placeholder
    assert!(!resolver.is_provisioned());
}
