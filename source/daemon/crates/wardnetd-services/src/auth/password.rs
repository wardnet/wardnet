//! Credential primitives shared by every sign-in path.
//!
//! Extracted out of `auth/service.rs` because the household-identity work
//! (ADR-0031) gives these three operations more than one caller: the legacy
//! local-admin login, the household-user login, enrolment redemption, and
//! password changes all hash the same way, and the session-token derivation is
//! shared by login, refresh, logout and validation.
//!
//! The reason to centralise is not tidiness. `hash_token` in particular must
//! have exactly one definition: every write, lookup and delete derives the
//! storage key from the raw token, so two derivations would make a session
//! created under one scheme unreachable — and undeletable — under the other.

use std::sync::LazyLock;

use argon2::PasswordHasher;
use argon2::password_hash::rand_core::OsRng;
use sha2::{Digest, Sha256};

use crate::error::AppError;

/// Derive the storage key for a raw session token.
///
/// SHA-256 rather than Argon2id on purpose, and the difference from
/// [`hash_password`] is deliberate: a session token is 32 bytes of CSPRNG
/// output, so it has no guessable structure for a slow hash to protect, and
/// this runs on **every authenticated request**. Passwords are low-entropy and
/// human-chosen, so they get the slow hash.
#[must_use]
pub fn hash_token(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

/// Fixed decoy password hash verified on a login path when the subject is
/// unknown.
///
/// It uses the same default Argon2id parameters as a real credential, so a
/// failed verify against it costs the same as a failed verify against a genuine
/// record — closing the timing side channel an attacker could otherwise use to
/// tell which usernames or email addresses exist. Computed once on first use.
///
/// This is why [`find_for_login`](wardnetd_data::repository::user_credential::UserCredentialRepository::find_for_login)
/// returns `None` for both "no such credential" and "user is disabled": the
/// caller runs the same constant-work path for both, so a disabled account
/// cannot be probed either.
static DECOY_PASSWORD_HASH: LazyLock<String> = LazyLock::new(|| {
    let salt = argon2::password_hash::SaltString::from_b64("d2FyZG5ldGRlY295c2FsdA")
        .expect("decoy salt is valid non-padded base64");
    argon2::Argon2::default()
        .hash_password(b"login-decoy", &salt)
        .expect("decoy hash generation with default params cannot fail")
        .to_string()
});

/// Burn the same amount of CPU a real verify would, then discard the result.
///
/// Call this on every "no such credential" branch **before** returning
/// `Unauthorized`. Short-circuiting instead would leak, via response latency,
/// which subjects exist.
pub fn verify_decoy(password: &str) {
    let decoy = argon2::PasswordHash::new(&DECOY_PASSWORD_HASH)
        .expect("decoy hash is a valid argon2 PHC string");
    let _ = argon2::PasswordVerifier::verify_password(
        &argon2::Argon2::default(),
        password.as_bytes(),
        &decoy,
    );
}

/// Hash a plaintext password with Argon2id and a fresh random salt.
pub fn hash_password(password: &str) -> Result<String, AppError> {
    // `OsRng` comes from the `password_hash` crate (re-exported via `argon2`)
    // to avoid the `rand_core` version conflict between `rand 0.10` and
    // `password-hash 0.5`, which depends on `rand_core 0.6`.
    let salt = argon2::password_hash::SaltString::generate(&mut OsRng);
    Ok(argon2::Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("failed to hash password: {e}")))?
        .to_string())
}

/// Verify a plaintext password against a stored Argon2id PHC string.
///
/// A malformed stored hash is [`AppError::Internal`], not a failed login: it
/// means the row is corrupt, and reporting "invalid credentials" would send the
/// operator hunting for a forgotten password instead of a broken database.
pub fn verify_password(password: &str, stored_hash: &str) -> Result<(), AppError> {
    let parsed = argon2::PasswordHash::new(stored_hash)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("invalid stored hash: {e}")))?;

    argon2::PasswordVerifier::verify_password(
        &argon2::Argon2::default(),
        password.as_bytes(),
        &parsed,
    )
    .map_err(|_| AppError::Unauthorized("invalid credentials".to_owned()))
}

/// Minimum accepted password length, applied on every path that sets one.
pub const MIN_PASSWORD_LENGTH: usize = 8;

/// Validate a password against the policy shared by setup, enrolment and
/// password change.
pub fn validate_password(password: &str) -> Result<(), AppError> {
    if password.len() < MIN_PASSWORD_LENGTH {
        return Err(AppError::BadRequest(format!(
            "password must be at least {MIN_PASSWORD_LENGTH} characters"
        )));
    }
    Ok(())
}

/// Generate a fresh session token and its storage hash.
///
/// Returns `(raw_token, token_hash)`. The raw token is handed to the client
/// exactly once; only the hash is ever persisted.
#[must_use]
pub fn new_session_token() -> (String, String) {
    use base64::Engine;

    let bytes: [u8; 32] = rand::random();
    let token = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes);
    let hash = hash_token(&token);
    (token, hash)
}
