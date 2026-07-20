//! Signed iOS configuration-profile (`.mobileconfig`) generation (issue #914).
//!
//! iOS has no programmatic Private-DNS API, so a granted device installs an
//! encrypted-DNS **configuration profile**: a `com.apple.dnsSettings.managed`
//! payload carrying `DNSProtocol=TLS` and the device's secret `ServerName`.
//! `ServerAddresses` is deliberately omitted — the hostname is a DDNS name that
//! split-horizon resolves to the Pi on the LAN and to the regional edge while
//! roaming, so pinning an address would break one of those paths. No
//! `OnDemandRules` either: iOS already auto-exempts captive-portal probing for
//! managed DNS, and an always-on rule would strand the phone behind hotel
//! logins.
//!
//! The profile is **CMS-signed with the box's live Let's Encrypt leaf** so iOS
//! renders it green ("Verified") instead of the scary "Not Signed" banner. The
//! signature is a standard PKCS#7 / RFC 5652 `SignedData` over the plist, with
//! the leaf (and its issuing intermediate) embedded so the phone can build a
//! path to the ISRG root already in its trust store.
//!
//! Identifiers are **`UUIDv5`**, derived from the hostname: re-downloading the
//! profile yields byte-identical identifiers, so iOS *replaces* the existing
//! profile in place rather than stacking a second copy.

use const_oid::ObjectIdentifier;
use der::asn1::{OctetString, SetOfVec};
use der::{Any, Encode, Tag};
use p256::ecdsa::signature::Signer;
use p256::ecdsa::{DerSignature, SigningKey};
use p256::pkcs8::DecodePrivateKey;
use sha2::{Digest, Sha256};
use spki::AlgorithmIdentifierOwned;
use uuid::Uuid;
use x509_cert::Certificate;
use x509_cert::attr::Attribute;

use cms::cert::{CertificateChoices, IssuerAndSerialNumber};
use cms::content_info::{CmsVersion, ContentInfo};
use cms::signed_data::{
    CertificateSet, EncapsulatedContentInfo, SignedData, SignerIdentifier, SignerInfo, SignerInfos,
};

/// Apple's content type for a downloadable configuration profile. Safari keys
/// its install flow off this exact string, so the endpoint must serve it
/// verbatim.
pub const MOBILECONFIG_CONTENT_TYPE: &str = "application/x-apple-aspen-config";

/// Reverse-DNS prefix for the profile's payload identifiers. Combined with the
/// per-device hostname it yields a stable, collision-free identity per grant.
const PAYLOAD_ID_PREFIX: &str = "services.wardnet.private-dns";

/// User-facing name shown in iOS Settings › General › VPN & Device Management.
const DISPLAY_NAME: &str = "wardnet Private DNS";

/// Fixed namespace for the `UUIDv5` payload identifiers. A private, arbitrary
/// constant — its only job is to keep the derived UUIDs stable across
/// re-downloads and disjoint from any other `UUIDv5` space.
const PROFILE_UUID_NAMESPACE: Uuid = Uuid::from_u128(0x9f2c_6d1a_4b83_5e70_a1c4_8f6b_2d09_e357);

/// id-data (`1.2.840.113549.1.7.1`) — the CMS encapsulated content type for an
/// opaque octet payload (the plist).
const ID_DATA: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.7.1");

/// id-signedData (`1.2.840.113549.1.7.2`) — the outer `ContentInfo` type.
const ID_SIGNED_DATA: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.7.2");

/// id-sha256 (`2.16.840.1.101.3.4.2.1`) — the digest algorithm for the
/// `SignedData` and the signer's `messageDigest` attribute.
const ID_SHA256: ObjectIdentifier = ObjectIdentifier::new_unwrap("2.16.840.1.101.3.4.2.1");

/// ecdsa-with-SHA256 (`1.2.840.10045.4.3.2`) — the leaf's signature algorithm.
const ID_ECDSA_SHA256: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.10045.4.3.2");

/// id-contentType (`1.2.840.113549.1.9.3`) — the signed attribute that pins the
/// wrapped content type (RFC 5652 §11.1).
const ID_CONTENT_TYPE: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.9.3");

/// id-messageDigest (`1.2.840.113549.1.9.4`) — the signed attribute carrying the
/// content digest (RFC 5652 §11.2).
const ID_MESSAGE_DIGEST: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.9.4");

/// Something went wrong producing the signed profile. All variants are
/// operator-facing (missing/garbled cert material, a signer that can't sign);
/// none are reachable through user input.
#[derive(Debug, thiserror::Error)]
pub enum ProfileError {
    /// The stored certificate chain was empty or not valid PEM.
    #[error("could not parse the certificate chain: {0}")]
    Certificate(String),
    /// The stored private key was not a PKCS#8 P-256 key.
    #[error("could not parse the signing key: {0}")]
    Key(String),
    /// Assembling or DER-encoding the CMS `SignedData` failed.
    #[error("could not build the CMS signature: {0}")]
    Cms(String),
}

/// Render the (unsigned) `.mobileconfig` plist for `hostname`.
///
/// Pure and deterministic — the same hostname always yields byte-identical
/// output (identifiers included), which is what lets a re-download replace the
/// installed profile instead of stacking a duplicate.
#[must_use]
pub fn build_plist(hostname: &str) -> String {
    let escaped = xml_escape(hostname);
    let top_uuid = payload_uuid("config", hostname);
    let dns_uuid = payload_uuid("dns", hostname);
    let top_id = format!("{PAYLOAD_ID_PREFIX}.{hostname}");
    let dns_id = format!("{top_id}.dns");
    let top_id = xml_escape(&top_id);
    let dns_id = xml_escape(&dns_id);

    // Hand-rolled rather than via a plist crate: the schema is tiny and fixed,
    // and a literal template keeps the golden-file test honest about the exact
    // bytes iOS receives.
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>PayloadContent</key>
	<array>
		<dict>
			<key>DNSSettings</key>
			<dict>
				<key>DNSProtocol</key>
				<string>TLS</string>
				<key>ServerName</key>
				<string>{escaped}</string>
			</dict>
			<key>PayloadDisplayName</key>
			<string>{DISPLAY_NAME}</string>
			<key>PayloadIdentifier</key>
			<string>{dns_id}</string>
			<key>PayloadType</key>
			<string>com.apple.dnsSettings.managed</string>
			<key>PayloadUUID</key>
			<string>{dns_uuid}</string>
			<key>PayloadVersion</key>
			<integer>1</integer>
		</dict>
	</array>
	<key>PayloadDisplayName</key>
	<string>{DISPLAY_NAME}</string>
	<key>PayloadIdentifier</key>
	<string>{top_id}</string>
	<key>PayloadRemovalDisallowed</key>
	<false/>
	<key>PayloadType</key>
	<string>Configuration</string>
	<key>PayloadUUID</key>
	<string>{top_uuid}</string>
	<key>PayloadVersion</key>
	<integer>1</integer>
</dict>
</plist>
"#
    )
}

/// Build the signed `.mobileconfig` for `hostname`: render the plist, then wrap
/// it in a CMS `SignedData` under the box's live cert/key.
///
/// `cert_chain_pem` is the leaf-first Let's Encrypt chain and `key_pem` the
/// leaf's PKCS#8 private key — exactly the pair
/// [`crate::tls::load_stored_cert`] returns.
pub fn build_signed_profile(
    hostname: &str,
    cert_chain_pem: &[u8],
    key_pem: &[u8],
) -> Result<Vec<u8>, ProfileError> {
    let plist = build_plist(hostname);
    sign(plist.as_bytes(), cert_chain_pem, key_pem)
}

/// CMS-sign `payload` with the leaf of `cert_chain_pem` (and embed the full
/// chain), producing a DER `ContentInfo` of type `signedData` with the payload
/// attached.
pub fn sign(
    payload: &[u8],
    cert_chain_pem: &[u8],
    key_pem: &[u8],
) -> Result<Vec<u8>, ProfileError> {
    let certs = Certificate::load_pem_chain(cert_chain_pem)
        .map_err(|e| ProfileError::Certificate(e.to_string()))?;
    let leaf = certs
        .first()
        .ok_or_else(|| ProfileError::Certificate("empty chain".to_owned()))?;

    let key_pem = std::str::from_utf8(key_pem)
        .map_err(|e| ProfileError::Key(format!("key PEM is not UTF-8: {e}")))?;
    let signing_key =
        SigningKey::from_pkcs8_pem(key_pem).map_err(|e| ProfileError::Key(e.to_string()))?;

    // Attach the plist as the encapsulated content (id-data OCTET STRING).
    let econtent = Any::new(Tag::OctetString, payload).map_err(cms_err)?;
    let encap = EncapsulatedContentInfo {
        econtent_type: ID_DATA,
        econtent: Some(econtent),
    };

    let sha256 = AlgorithmIdentifierOwned {
        oid: ID_SHA256,
        parameters: None,
    };

    // Signed attributes: contentType (must equal the eContentType) and
    // messageDigest = SHA-256 of the eContent *value* octets (RFC 5652 §5.4).
    let content_type_attr = attribute(
        ID_CONTENT_TYPE,
        Any::encode_from(&ID_DATA).map_err(cms_err)?,
    )?;
    let digest = Sha256::digest(payload);
    let message_digest_attr = attribute(
        ID_MESSAGE_DIGEST,
        Any::new(Tag::OctetString, digest.as_slice()).map_err(cms_err)?,
    )?;
    let signed_attrs: SetOfVec<Attribute> =
        SetOfVec::try_from(vec![content_type_attr, message_digest_attr]).map_err(cms_err)?;

    // Sign the DER of the attributes as a universal SET (0x31) — distinct from
    // the [0] IMPLICIT tag they carry on the wire inside the SignerInfo.
    let tbs = signed_attrs.to_der().map_err(cms_err)?;
    let signature: DerSignature = signing_key.sign(&tbs);
    let signature = OctetString::new(signature.to_bytes()).map_err(cms_err)?;

    // The signer is identified by the leaf's issuer + serial; the leaf's key
    // signs, so its algorithm (ECDSA-P256-SHA256) is what iOS verifies against.
    let signer_info = SignerInfo {
        version: CmsVersion::V1,
        sid: SignerIdentifier::IssuerAndSerialNumber(IssuerAndSerialNumber {
            issuer: leaf.tbs_certificate.issuer.clone(),
            serial_number: leaf.tbs_certificate.serial_number.clone(),
        }),
        digest_alg: sha256.clone(),
        signed_attrs: Some(signed_attrs),
        signature_algorithm: AlgorithmIdentifierOwned {
            oid: ID_ECDSA_SHA256,
            parameters: None,
        },
        signature,
        unsigned_attrs: None,
    };

    // Embed the leaf (+ issuing intermediate) so iOS can build a path to the
    // ISRG root already in its trust store.
    let certificate_set = CertificateSet::try_from(
        certs
            .into_iter()
            .map(CertificateChoices::Certificate)
            .collect::<Vec<_>>(),
    )
    .map_err(cms_err)?;

    let signed_data = SignedData {
        version: CmsVersion::V1,
        digest_algorithms: SetOfVec::try_from(vec![sha256]).map_err(cms_err)?,
        encap_content_info: encap,
        certificates: Some(certificate_set),
        crls: None,
        signer_infos: SignerInfos(SetOfVec::try_from(vec![signer_info]).map_err(cms_err)?),
    };

    let content_info = ContentInfo {
        content_type: ID_SIGNED_DATA,
        content: Any::encode_from(&signed_data).map_err(cms_err)?,
    };
    content_info.to_der().map_err(cms_err)
}

/// Wrap a single-valued CMS attribute (`oid` → `{ value }`).
fn attribute(oid: ObjectIdentifier, value: Any) -> Result<Attribute, ProfileError> {
    Ok(Attribute {
        oid,
        values: SetOfVec::try_from(vec![value]).map_err(cms_err)?,
    })
}

/// Map any DER error from CMS assembly onto [`ProfileError::Cms`].
fn cms_err(e: der::Error) -> ProfileError {
    ProfileError::Cms(e.to_string())
}

/// Derive a stable `UUIDv5` for `role` (`config` / `dns`) under `hostname`, so
/// re-downloads replace rather than stack the installed profile.
fn payload_uuid(role: &str, hostname: &str) -> Uuid {
    Uuid::new_v5(
        &PROFILE_UUID_NAMESPACE,
        format!("{role}:{hostname}").as_bytes(),
    )
}

/// Escape the five XML metacharacters. The interpolated values (hostname,
/// identifiers) are daemon-minted and never contain them today, but a mangled
/// grant must never be able to break out of a `<string>` element.
fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}
