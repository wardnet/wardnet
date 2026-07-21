//! Tests for the signed `.mobileconfig` generator (issue #914).

use cms::content_info::ContentInfo;
use cms::signed_data::SignedData;
use const_oid::ObjectIdentifier;
use der::asn1::OctetString;
use der::{Decode, Encode};
use p256::ecdsa::signature::Verifier;
use p256::ecdsa::{DerSignature, SigningKey, VerifyingKey};
use p256::pkcs8::DecodePrivateKey;

use crate::private_dns::profile::{build_plist, build_signed_profile, sign};

/// id-signedData (`1.2.840.113549.1.7.2`).
const ID_SIGNED_DATA: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.7.2");

const GOLDEN_HOST: &str = "abcdefghijklmnop.casa.my.wardnet.services";

/// A self-signed P-256 leaf + PKCS#8 key, standing in for the box's live
/// Let's Encrypt pair (rcgen defaults to P-256, matching the real leaf).
fn self_signed_pem() -> (Vec<u8>, Vec<u8>) {
    let key_pair = rcgen::KeyPair::generate().expect("keypair");
    let params = rcgen::CertificateParams::new(vec![GOLDEN_HOST.to_owned()]).expect("cert params");
    let cert = params.self_signed(&key_pair).expect("self-sign");
    (
        cert.pem().into_bytes(),
        key_pair.serialize_pem().into_bytes(),
    )
}

/// The value of the first `<key>PayloadUUID</key>` in a plist (the payload's).
fn extract_first_uuid(plist: &str) -> &str {
    let marker = "<key>PayloadUUID</key>";
    let after = &plist[plist.find(marker).expect("PayloadUUID key") + marker.len()..];
    let open = after.find("<string>").expect("uuid open") + "<string>".len();
    let close = after.find("</string>").expect("uuid close");
    &after[open..close]
}

#[test]
fn plist_matches_golden() {
    let golden = include_str!("profile.golden.mobileconfig");
    assert_eq!(build_plist(GOLDEN_HOST), golden);
}

#[test]
fn plist_omits_addresses_and_ondemand() {
    // DDNS-friendly bootstrap: never pin an address, and let iOS handle
    // captive portals rather than an always-on OnDemand rule.
    let plist = build_plist(GOLDEN_HOST);
    assert!(plist.contains("<key>DNSProtocol</key>"));
    assert!(plist.contains("<string>TLS</string>"));
    assert!(plist.contains(&format!("<string>{GOLDEN_HOST}</string>")));
    assert!(!plist.contains("ServerAddresses"));
    assert!(!plist.contains("OnDemandRules"));
}

#[test]
fn identifiers_are_stable_across_rebuilds() {
    // Byte-identical output for the same host is what lets a re-download
    // replace the installed profile instead of stacking a duplicate.
    assert_eq!(build_plist(GOLDEN_HOST), build_plist(GOLDEN_HOST));
}

#[test]
fn identifiers_differ_between_hosts() {
    let a = build_plist("aaaa1111bbbb2222.casa.my.wardnet.services");
    let b = build_plist("cccc3333dddd4444.casa.my.wardnet.services");
    assert_ne!(a, b);
    // The UUIDs specifically must differ (not just the ServerName), or iOS
    // would treat two devices' profiles as the same installed payload.
    let uuid_a = extract_first_uuid(&a);
    let uuid_b = extract_first_uuid(&b);
    assert_ne!(uuid_a, uuid_b);
}

#[test]
fn xml_metacharacters_are_escaped() {
    // A hostname can't contain these today, but a mangled grant must never
    // break out of the <string> element.
    let plist = build_plist("a&b<c>d\"e'f.example.test");
    assert!(plist.contains("a&amp;b&lt;c&gt;d&quot;e&apos;f.example.test"));
    // No raw metacharacter leaked into the interpolated value.
    assert!(!plist.contains("a&b<c>d"));
}

#[test]
fn signed_profile_is_valid_cms_signeddata() {
    let (chain, key) = self_signed_pem();
    let signed = build_signed_profile(GOLDEN_HOST, &chain, &key).expect("sign");

    let ci = ContentInfo::from_der(&signed).expect("parse ContentInfo");
    assert_eq!(ci.content_type, ID_SIGNED_DATA);

    let sd = SignedData::from_der(&ci.content.to_der().unwrap()).expect("parse SignedData");

    // The leaf is embedded so iOS can build the path to the ISRG root.
    let certs = sd.certificates.as_ref().expect("certificates present");
    assert_eq!(certs.0.len(), 1);

    // Exactly one signer, over the attached plist.
    assert_eq!(sd.signer_infos.0.len(), 1);
    let plist = build_plist(GOLDEN_HOST);
    let econtent = sd
        .encap_content_info
        .econtent
        .as_ref()
        .expect("attached content");
    let octets = OctetString::from_der(&econtent.to_der().unwrap()).unwrap();
    assert_eq!(octets.as_bytes(), plist.as_bytes());
}

#[test]
fn signed_profile_signature_verifies() {
    let key_pair = rcgen::KeyPair::generate().unwrap();
    let params = rcgen::CertificateParams::new(vec![GOLDEN_HOST.to_owned()]).unwrap();
    let cert = params.self_signed(&key_pair).unwrap();
    let chain = cert.pem().into_bytes();
    let key = key_pair.serialize_pem().into_bytes();

    let der = sign(b"hello profile", &chain, &key).expect("sign");
    let ci = ContentInfo::from_der(&der).unwrap();
    let sd = SignedData::from_der(&ci.content.to_der().unwrap()).unwrap();
    let info = sd.signer_infos.0.iter().next().unwrap();

    // Re-encode the signed attributes as the SET they are signed over
    // (RFC 5652 §5.4) and verify with the leaf's public key.
    let tbs = info
        .signed_attrs
        .as_ref()
        .expect("signed attrs")
        .to_der()
        .unwrap();
    let sig = DerSignature::try_from(info.signature.as_bytes()).unwrap();
    let verifying_key = VerifyingKey::from(
        &SigningKey::from_pkcs8_pem(std::str::from_utf8(&key).unwrap()).unwrap(),
    );
    verifying_key
        .verify(&tbs, &sig)
        .expect("signature verifies");
}
