//! Web Push delivery: VAPID signing + RFC 8291 payload encryption + HTTP POST.
//!
//! This is the cryptographic edge of the push subsystem and the focus of the
//! security review. All key material and encryption is delegated to
//! [`web_push_native`] (`RustCrypto`: `p256` / `aes-gcm` / `hkdf`); this module
//! only marshals a stored subscription into a builder call and classifies the
//! push service's HTTP response.
//!
//! The [`WebPushSender`] trait is the seam the [`PushService`](super) tests and
//! the mock daemon substitute — the real [`ReqwestWebPushSender`] performs
//! network I/O, so unit tests never touch it.

use async_trait::async_trait;
use base64::Engine;
use base64::engine::general_purpose::{URL_SAFE, URL_SAFE_NO_PAD};
use web_push_native::jwt_simple::algorithms::{ECDSAP256PublicKeyLike, ES256KeyPair};
use web_push_native::p256::PublicKey;
use web_push_native::{Auth, WebPushBuilder};

/// Decode a base64url value, accepting both unpadded and padded input —
/// browsers differ on whether `p256dh` / `auth` carry `=` padding.
fn decode_b64url(value: &str) -> Result<Vec<u8>, base64::DecodeError> {
    URL_SAFE_NO_PAD
        .decode(value)
        .or_else(|_| URL_SAFE.decode(value))
}

/// The daemon's VAPID key pair. Wraps [`ES256KeyPair`] so the rest of the
/// service never depends on the JWT crate directly.
pub struct VapidKey {
    key_pair: ES256KeyPair,
}

impl VapidKey {
    /// Generate a fresh P-256 VAPID key pair (done once, on first use).
    #[must_use]
    pub fn generate() -> Self {
        Self {
            key_pair: ES256KeyPair::generate(),
        }
    }

    /// Reconstruct a key pair from the raw bytes produced by [`Self::to_bytes`].
    pub fn from_bytes(raw: &[u8]) -> Result<Self, anyhow::Error> {
        // The raw P-256 private scalar is 32 bytes; `ES256KeyPair::from_bytes`
        // *panics* on a shorter slice, so reject a corrupt/truncated stored
        // secret here instead of crashing the daemon.
        if raw.len() != 32 {
            anyhow::bail!("VAPID key must be 32 bytes, got {}", raw.len());
        }
        let key_pair = ES256KeyPair::from_bytes(raw)
            .map_err(|e| anyhow::anyhow!("invalid stored VAPID key: {e}"))?;
        Ok(Self { key_pair })
    }

    /// Serialize the private key for storage in the [`SecretStore`](crate::secret_store::SecretStore).
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        self.key_pair.to_bytes()
    }

    /// The uncompressed SEC1 public point, base64url-unpadded — the value a
    /// browser passes as `applicationServerKey` to `PushManager.subscribe`.
    #[must_use]
    pub fn public_key_base64url(&self) -> String {
        let uncompressed = self
            .key_pair
            .public_key()
            .public_key()
            .to_bytes_uncompressed();
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(uncompressed)
    }
}

/// A single delivery target — one stored subscription's addressing + keys.
#[derive(Debug, Clone)]
pub struct PushTarget<'a> {
    pub endpoint: &'a str,
    /// Base64url subscriber ECDH public key (`p256dh`).
    pub p256dh: &'a str,
    /// Base64url subscriber auth secret (`auth`).
    pub auth: &'a str,
}

/// Outcome of a single push delivery attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SendOutcome {
    /// The push service accepted the message (2xx).
    Delivered,
    /// The subscription is dead (404/410) — the caller must prune it.
    Gone,
    /// Transient failure (network error, 4xx other than 404/410, or 5xx).
    /// Push is best-effort: log and drop, keep the subscription.
    TransientFailure,
}

/// Sends encrypted Web Push messages. Abstracted so service logic is testable
/// without network I/O and the mock daemon can no-op delivery.
#[async_trait]
pub trait WebPushSender: Send + Sync {
    /// Encrypt `payload` for `target`, attach a VAPID signature from `vapid`,
    /// and POST to the endpoint. Never returns an error: every failure maps to
    /// a [`SendOutcome`] the caller acts on.
    async fn send(&self, vapid: &VapidKey, target: PushTarget<'_>, payload: Vec<u8>)
    -> SendOutcome;
}

/// Real [`WebPushSender`] over [`reqwest`].
pub struct ReqwestWebPushSender {
    client: reqwest::Client,
    /// VAPID `sub` contact, e.g. `mailto:...` — identifies this server to the
    /// push service operator.
    contact: String,
}

impl ReqwestWebPushSender {
    #[must_use]
    pub fn new(client: reqwest::Client, contact: String) -> Self {
        Self { client, contact }
    }

    /// Build the encrypted, VAPID-signed request. Separated from the HTTP send
    /// so failures in key parsing/encryption are classified as build errors.
    pub(crate) fn build_request(
        &self,
        vapid: &VapidKey,
        target: &PushTarget<'_>,
        payload: Vec<u8>,
    ) -> Result<http::Request<Vec<u8>>, anyhow::Error> {
        let endpoint = target
            .endpoint
            .parse::<http::Uri>()
            .map_err(|e| anyhow::anyhow!("bad push endpoint: {e}"))?;

        let p256dh =
            decode_b64url(target.p256dh).map_err(|e| anyhow::anyhow!("bad p256dh: {e}"))?;
        let ua_public = PublicKey::from_sec1_bytes(&p256dh)
            .map_err(|e| anyhow::anyhow!("bad p256dh point: {e}"))?;

        let auth =
            decode_b64url(target.auth).map_err(|e| anyhow::anyhow!("bad auth secret: {e}"))?;
        // The Web Push auth secret is exactly 16 bytes; `Auth::clone_from_slice`
        // panics otherwise, so validate the length before handing it over.
        if auth.len() != 16 {
            anyhow::bail!("auth secret must be 16 bytes, got {}", auth.len());
        }
        let ua_auth = Auth::clone_from_slice(&auth);

        let request = WebPushBuilder::new(endpoint, ua_public, ua_auth)
            .with_vapid(&vapid.key_pair, &self.contact)
            .build(payload)
            .map_err(|e| anyhow::anyhow!("web push encryption failed: {e}"))?;
        Ok(request)
    }
}

#[async_trait]
impl WebPushSender for ReqwestWebPushSender {
    async fn send(
        &self,
        vapid: &VapidKey,
        target: PushTarget<'_>,
        payload: Vec<u8>,
    ) -> SendOutcome {
        let endpoint = target.endpoint.to_owned();
        let request = match self.build_request(vapid, &target, payload) {
            Ok(request) => request,
            Err(error) => {
                // A malformed stored subscription can never succeed; treat it as
                // Gone so the caller prunes it rather than retrying forever.
                tracing::warn!(%error, endpoint = %endpoint, "push: dropping unbuildable subscription");
                return SendOutcome::Gone;
            }
        };

        let (parts, body) = request.into_parts();
        let mut builder = self.client.post(parts.uri.to_string()).body(body);
        for (name, value) in &parts.headers {
            builder = builder.header(name, value);
        }

        match builder.send().await {
            Ok(response) => {
                let status = response.status();
                if status.is_success() {
                    SendOutcome::Delivered
                } else if status == reqwest::StatusCode::NOT_FOUND
                    || status == reqwest::StatusCode::GONE
                {
                    tracing::debug!(endpoint = %endpoint, %status, "push: subscription gone, pruning");
                    SendOutcome::Gone
                } else {
                    tracing::warn!(endpoint = %endpoint, %status, "push: delivery rejected");
                    SendOutcome::TransientFailure
                }
            }
            Err(error) => {
                tracing::warn!(%error, endpoint = %endpoint, "push: delivery network error");
                SendOutcome::TransientFailure
            }
        }
    }
}
