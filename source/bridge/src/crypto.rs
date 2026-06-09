//! AES-256-GCM sealing for TLS material at rest.
//!
//! The bridge persists its ACME account credentials, certificate chain, and leaf
//! private key in the regional Postgres so the cert survives restarts and can be
//! shared across hosts. The account key and leaf key are secrets, so the blob is
//! sealed with AES-256-GCM under the per-region `ENCRYPTION_KEY` (the same key on
//! every host in the region — see [`crate::config::Config::encryption_key`]).
//!
//! A fresh random 96-bit nonce is generated per seal and stored alongside the
//! ciphertext; GCM's authentication tag is appended to the ciphertext, so a
//! tampered blob fails to open.

use aes_gcm::aead::{Aead as _, KeyInit as _};
use aes_gcm::{Aes256Gcm, Key, Nonce};

/// GCM nonce length in bytes (96-bit, the AES-GCM standard).
pub const NONCE_LEN: usize = 12;

/// Seal `plaintext` under `key`, returning `(ciphertext_with_tag, nonce)`.
///
/// # Errors
/// Returns an error only if the AEAD primitive fails (not expected for valid
/// inputs).
pub fn seal(key: &[u8; 32], plaintext: &[u8]) -> anyhow::Result<(Vec<u8>, [u8; NONCE_LEN])> {
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    let mut nonce_bytes = [0u8; NONCE_LEN];
    rand::fill(&mut nonce_bytes);
    let ciphertext = cipher
        .encrypt(Nonce::from_slice(&nonce_bytes), plaintext)
        .map_err(|e| anyhow::anyhow!("AES-GCM seal failed: {e}"))?;
    Ok((ciphertext, nonce_bytes))
}

/// Open a sealed blob produced by [`seal`].
///
/// # Errors
/// Returns an error if `nonce` is not [`NONCE_LEN`] bytes, or if authentication
/// fails (wrong key or tampered ciphertext).
pub fn open(key: &[u8; 32], nonce: &[u8], ciphertext: &[u8]) -> anyhow::Result<Vec<u8>> {
    if nonce.len() != NONCE_LEN {
        anyhow::bail!(
            "AES-GCM nonce must be {NONCE_LEN} bytes, got {}",
            nonce.len()
        );
    }
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    cipher
        .decrypt(Nonce::from_slice(nonce), ciphertext)
        .map_err(|_| anyhow::anyhow!("AES-GCM open failed: wrong key or tampered data"))
}

#[cfg(test)]
mod tests;
