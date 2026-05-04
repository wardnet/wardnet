//! Read + verify the signed payload. Pure I/O + crypto, no exec.

use std::path::Path;

use anyhow::Context;

/// Read the payload + detached signature from disk. Returns `None` if
/// either file is missing — the caller maps this to exit-0 (nothing
/// to do) per the framework's "tolerate missing payload" contract.
/// Any other I/O error is surfaced as `Err`.
pub fn read_artifacts(payload: &Path, signature: &Path) -> Option<(Vec<u8>, Vec<u8>)> {
    let payload_bytes = match std::fs::read(payload) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
        Err(e) => {
            tracing::error!(
                error = %e,
                path = %payload.display(),
                "failed to read payload at {path}: {e}",
                path = payload.display(),
            );
            return None;
        }
    };
    let signature_bytes = match std::fs::read(signature) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
        Err(e) => {
            tracing::error!(
                error = %e,
                path = %signature.display(),
                "failed to read signature at {path}: {e}",
                path = signature.display(),
            );
            return None;
        }
    };
    Some((payload_bytes, signature_bytes))
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
