//! Pure `WireGuard` (Curve25519 / X25519) keypair generation (issue #809).
//!
//! `WireGuard` identities are X25519 keys. The server keypair (generated once on
//! first enable) and every per-peer keypair (generated on the daemon, returned
//! once, never stored) are produced here. Kept dependency-light and
//! platform-independent so the service layer — which builds on macOS for the
//! mock — can generate keys without pulling in the Linux-only
//! `wireguard-control` crate.

use x25519_dalek::{X25519_BASEPOINT_BYTES, x25519};

/// Generate a fresh `WireGuard` keypair as `(private, public)` raw 32-byte arrays.
///
/// The private key is 32 random bytes with the standard Curve25519 clamping
/// applied (matching `wg genkey`); the public key is the clamped scalar times
/// the X25519 base point.
#[must_use]
pub fn generate_keypair() -> ([u8; 32], [u8; 32]) {
    let mut private = [0u8; 32];
    rand::fill(&mut private);
    // Curve25519 clamping — see WireGuard's `curve25519_clamp_secret`.
    private[0] &= 0b1111_1000;
    private[31] &= 0b0111_1111;
    private[31] |= 0b0100_0000;
    // `x25519` applies clamping internally too, so the derived public key is
    // consistent whether the peer clamps again or not.
    let public = x25519(private, X25519_BASEPOINT_BYTES);
    (private, public)
}
