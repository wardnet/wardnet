//! Ed25519 **proof-of-possession (`PoP`)** signing for wardnet-cloud requests.
//!
//! Every authenticated daemon call carries a signature over a canonical payload
//! that binds the request's method, path, a fresh timestamp, and a hash of the
//! body to the daemon's enrolled Ed25519 key. The cloud rebuilds the same
//! payload and verifies it against the key bound to the JWT (`cnf`), so a sniffed
//! JWT is useless without the private key and a captured request cannot be
//! replayed or tampered with.
//!
//! The canonical payload MUST byte-match the cloud verifier
//! (`wardnet-cloud …/common/src/auth` `canonical_request_payload`):
//! `"<METHOD>\n<path_and_query>\n<timestamp>\n<hex-sha256(body)>"`. The two repos
//! are separate workspaces, so [`tests`](super::tests) pins this format by
//! constructing a signature and verifying its bytes.

use base64::Engine as _;
use ed25519_dalek::{Signer, SigningKey};
use sha2::{Digest, Sha256};

/// HTTP header carrying the request's Unix-second timestamp (cloud enforces a
/// ±window).
pub const TIMESTAMP_HEADER: &str = "X-Wardnet-Timestamp";
/// HTTP header carrying the base64 Ed25519 signature over the canonical payload.
pub const SIGNATURE_HEADER: &str = "X-Wardnet-Signature";

/// Build the canonical request payload the cloud verifier reconstructs.
#[must_use]
pub fn canonical_payload(
    method: &str,
    path_and_query: &str,
    timestamp: i64,
    body: &[u8],
) -> String {
    let body_hash = hex::encode(Sha256::digest(body));
    format!("{method}\n{path_and_query}\n{timestamp}\n{body_hash}")
}

/// Sign the canonical payload, returning the **standard-base64** signature.
#[must_use]
pub fn sign(
    key: &SigningKey,
    method: &str,
    path_and_query: &str,
    timestamp: i64,
    body: &[u8],
) -> String {
    let payload = canonical_payload(method, path_and_query, timestamp, body);
    let signature = key.sign(payload.as_bytes());
    base64::engine::general_purpose::STANDARD.encode(signature.to_bytes())
}
