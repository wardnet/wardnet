//! The daemon's cloud **identity**: its Ed25519 key plus the short-lived
//! identity JWT minted from it.
//!
//! One [`DaemonIdentity`] is built per operation from the persisted 32-byte seed
//! and a shared [`TenantsClient`]. It owns three things:
//!
//! * the `SigningKey` used for every `PoP` signature;
//! * an **in-memory** JWT cache — the token is never persisted (it is cheap to
//!   re-mint and always re-mintable from the key), and is refreshed when within
//!   [`REFRESH_SKEW_SECS`] of its `exp` (read by base64url-decoding the JWT
//!   payload, *without* verifying it — the cloud verifies, the daemon only needs
//!   to know when to refresh);
//! * an **entitlement** flag, flipped to lost the moment token minting is
//!   refused with `403` ("subscription not active") and back to active on the
//!   next successful mint. This is the daemon's only signal of its subscription
//!   state; the watchdog/Suspended layer reads [`DaemonIdentity::is_entitled`].
//!
//! Minting a network-scoped token after registering a network is a matter of
//! [`DaemonIdentity::forget_token`] then the next [`DaemonIdentity::token`].

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use base64::Engine as _;
use chrono::Utc;
use ed25519_dalek::SigningKey;

use super::CloudError;
use super::tenants::TenantsClient;

/// Refresh the JWT when it has fewer than this many seconds left, so a token
/// never expires mid-flight.
const REFRESH_SKEW_SECS: i64 = 120;
/// Fallback lifetime assumed when a minted JWT has no decodable `exp` (defensive;
/// real cloud tokens always carry one).
const FALLBACK_TTL_SECS: i64 = 300;

/// The daemon's cloud identity — Ed25519 key + cached JWT + entitlement flag.
pub struct DaemonIdentity {
    signing_key: SigningKey,
    public_key_b64: String,
    tenants: Arc<TenantsClient>,
    /// `(token, exp_unix)` — `None` until first mint.
    cached: Mutex<Option<(String, i64)>>,
    /// `true` while the last token mint succeeded; `false` once refused for
    /// entitlement.
    entitled: AtomicBool,
}

impl DaemonIdentity {
    /// Build an identity from the persisted 32-byte Ed25519 seed.
    #[must_use]
    pub fn from_seed(seed: [u8; 32], tenants: Arc<TenantsClient>) -> Arc<Self> {
        let signing_key = SigningKey::from_bytes(&seed);
        let public_key_b64 = base64::engine::general_purpose::STANDARD
            .encode(signing_key.verifying_key().to_bytes());
        Arc::new(Self {
            signing_key,
            public_key_b64,
            tenants,
            cached: Mutex::new(None),
            entitled: AtomicBool::new(true),
        })
    }

    /// The base64 Ed25519 public key — the value the cloud binds to (`cnf`/`sub`)
    /// and the body of enroll / token requests.
    #[must_use]
    pub fn public_key_b64(&self) -> &str {
        &self.public_key_b64
    }

    pub(crate) fn signing_key(&self) -> &SigningKey {
        &self.signing_key
    }

    /// Whether the daemon currently believes it is entitled (its last token mint
    /// was not refused). Read by the Suspended/entitlement layer.
    #[must_use]
    pub fn is_entitled(&self) -> bool {
        self.entitled.load(Ordering::Acquire)
    }

    pub(crate) fn mark_entitled(&self) {
        self.entitled.store(true, Ordering::Release);
    }

    pub(crate) fn mark_unentitled(&self) {
        self.entitled.store(false, Ordering::Release);
    }

    /// Drop the cached token so the next [`token`](Self::token) re-mints — used
    /// after registering a network to upgrade from a tenant-scoped to a
    /// network-scoped JWT.
    pub fn forget_token(&self) {
        *self.cached.lock().unwrap() = None;
    }

    /// Return a valid JWT, minting (and caching) a fresh one if absent or near
    /// expiry. Propagates [`CloudError::EntitlementLost`] when the subscription
    /// has lapsed.
    pub async fn token(&self) -> Result<String, CloudError> {
        let now = Utc::now().timestamp();
        if let Some((token, exp)) = self.cached.lock().unwrap().as_ref()
            && exp - now > REFRESH_SKEW_SECS
        {
            return Ok(token.clone());
        }
        // Lock is released before the await above; mint without holding it.
        // `Box::pin` breaks the static async cycle send→token→mint_token→send
        // (the token-mint path uses PoP-only auth and never re-enters `token`,
        // but the compiler can't see that).
        let token = Box::pin(self.tenants.mint_token(self)).await?;
        let exp = decode_exp(&token).unwrap_or(now + FALLBACK_TTL_SECS);
        *self.cached.lock().unwrap() = Some((token.clone(), exp));
        Ok(token)
    }
}

/// Read the `exp` claim from a JWT **without verifying** it (the cloud verifies;
/// the daemon only needs the expiry to schedule a refresh). Returns `None` if the
/// token is malformed.
fn decode_exp(jwt: &str) -> Option<i64> {
    let payload_b64 = jwt.split('.').nth(1)?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload_b64)
        .ok()?;
    let value: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    value.get("exp")?.as_i64()
}
