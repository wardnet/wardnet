//! Read + verify the signed payload. Pure I/O + crypto, no exec.

use std::path::Path;

use anyhow::Context;

/// Read the payload + detached signature from disk. Returns `Ok(None)`
/// if either file is missing — the caller maps this to exit-0 (nothing
/// to do) per the framework's "tolerate missing payload" contract.
/// Any other I/O error is surfaced as `Err`: a staged-but-unreadable
/// artifact must fail loudly, not masquerade as "nothing staged".
pub fn read_artifacts(
    payload: &Path,
    signature: &Path,
) -> anyhow::Result<Option<(Vec<u8>, Vec<u8>)>> {
    // Signature first: it is a few hundred bytes, while the payload
    // can be a multi-megabyte tarball. A half-staged update (payload
    // present, signature never written) short-circuits here without
    // paying a full payload read on every boot.
    let Some(signature_bytes) = read_optional(signature, "signature")? else {
        return Ok(None);
    };
    let Some(payload_bytes) = read_optional(payload, "payload")? else {
        return Ok(None);
    };
    Ok(Some((payload_bytes, signature_bytes)))
}

/// Read a file that is allowed to be absent. `NotFound` is the benign
/// `None`; any other I/O error is a hard `Err` so both artifact reads
/// keep the same missing-vs-unreadable policy.
fn read_optional(path: &Path, what: &str) -> anyhow::Result<Option<Vec<u8>>> {
    match std::fs::read(path) {
        Ok(b) => Ok(Some(b)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => {
            Err(anyhow::Error::from(e)
                .context(format!("failed to read {what} at {}", path.display())))
        }
    }
}

/// Verify a detached minisign signature over `payload` using the
/// PEM-style minisign public key text (two lines: untrusted comment,
/// then base64 key). Mirrors the daemon's `Sha256MinisignVerifier`
/// so production and post-upgrade trust the same key material.
pub fn verify(public_key_text: &str, payload: &[u8], signature: &[u8]) -> anyhow::Result<()> {
    let pk = minisign_verify::PublicKey::decode(public_key_text.trim())
        .context("invalid embedded public key")?;
    let sig_text = std::str::from_utf8(signature).context("signature is not utf-8")?;
    let sig = minisign_verify::Signature::decode(sig_text).context("invalid signature format")?;
    pk.verify(payload, &sig, /* allow_legacy */ false)
        .context("signature verification failed")?;
    Ok(())
}
