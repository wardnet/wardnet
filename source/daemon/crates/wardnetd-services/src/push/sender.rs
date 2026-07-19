//! Web Push delivery: VAPID signing + RFC 8291 payload encryption + HTTP POST.
//!
//! This is the cryptographic edge of the push subsystem and the focus of the
//! security review. Payload encryption is delegated to [`web_push_native`]
//! (`RustCrypto`: `p256` / `aes-gcm` / `hkdf`); the RFC 8292 VAPID token is
//! signed here with the re-exported `p256` directly, because web-push-native's
//! `vapid` feature would pull the whole jwt-simple stack (and with it the
//! `rsa` crate, RUSTSEC-2023-0071) for a JWT with three claims. Beyond that,
//! this module only marshals a stored subscription into a builder call and
//! classifies the push service's HTTP response.
//!
//! The [`WebPushSender`] trait is the seam the [`PushService`](super) tests and
//! the mock daemon substitute — the real [`ReqwestWebPushSender`] performs
//! network I/O, so unit tests never touch it.

use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use base64::Engine;
use base64::engine::general_purpose::{URL_SAFE, URL_SAFE_NO_PAD};
use web_push_native::p256::PublicKey;
use web_push_native::p256::ecdsa::signature::Signer;
use web_push_native::p256::ecdsa::{Signature, SigningKey};
use web_push_native::p256::elliptic_curve::rand_core::OsRng;
use web_push_native::{Auth, WebPushBuilder};

/// Decode a base64url value, accepting both unpadded and padded input —
/// browsers differ on whether `p256dh` / `auth` carry `=` padding.
fn decode_b64url(value: &str) -> Result<Vec<u8>, base64::DecodeError> {
    URL_SAFE_NO_PAD
        .decode(value)
        .or_else(|_| URL_SAFE.decode(value))
}

/// The daemon's VAPID key pair. Wraps a P-256 [`SigningKey`] so the rest of
/// the service never depends on the ECDSA crates directly.
pub struct VapidKey {
    signing_key: SigningKey,
}

impl VapidKey {
    /// Generate a fresh P-256 VAPID key pair (done once, on first use).
    #[must_use]
    pub fn generate() -> Self {
        Self {
            signing_key: SigningKey::random(&mut OsRng),
        }
    }

    /// Reconstruct a key pair from the raw bytes produced by [`Self::to_bytes`].
    pub fn from_bytes(raw: &[u8]) -> Result<Self, anyhow::Error> {
        // The raw P-256 private scalar is 32 bytes; keep the explicit length
        // check so a corrupt/truncated stored secret yields a diagnosable
        // error instead of a generic decode failure.
        if raw.len() != 32 {
            anyhow::bail!("VAPID key must be 32 bytes, got {}", raw.len());
        }
        let signing_key = SigningKey::from_slice(raw)
            .map_err(|e| anyhow::anyhow!("invalid stored VAPID key: {e}"))?;
        Ok(Self { signing_key })
    }

    /// Serialize the private key for storage in the [`SecretStore`](crate::secret_store::SecretStore).
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        self.signing_key.to_bytes().to_vec()
    }

    /// The uncompressed SEC1 public point, base64url-unpadded — the value a
    /// browser passes as `applicationServerKey` to `PushManager.subscribe`.
    #[must_use]
    pub fn public_key_base64url(&self) -> String {
        let uncompressed = self.signing_key.verifying_key().to_encoded_point(false);
        URL_SAFE_NO_PAD.encode(uncompressed.as_bytes())
    }
}

/// How long a minted VAPID token stays valid. Matches the 12-hour TTL
/// [`WebPushBuilder`] puts on the request itself.
const VAPID_TOKEN_LIFETIME_SECS: u64 = 12 * 60 * 60;

/// Build the RFC 8292 `Authorization` header value: `vapid t=<jwt>, k=<pub>`.
///
/// The token is a plain ES256 JWS over the three claims push services
/// validate — `aud` (endpoint origin), `exp`, and `sub` (our contact
/// address). P-256 JWS signatures are the fixed-size `r || s` form, so the
/// signature bytes go into the token as-is.
fn vapid_authorization(
    endpoint: &http::Uri,
    contact: &str,
    vapid: &VapidKey,
) -> Result<String, anyhow::Error> {
    let scheme = endpoint
        .scheme_str()
        .ok_or_else(|| anyhow::anyhow!("push endpoint has no scheme"))?;
    let host = endpoint
        .host()
        .ok_or_else(|| anyhow::anyhow!("push endpoint has no host"))?;
    let audience = format!("{scheme}://{host}");

    let expiry = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| anyhow::anyhow!("system clock is before the Unix epoch: {e}"))?
        .as_secs()
        + VAPID_TOKEN_LIFETIME_SECS;

    let header = URL_SAFE_NO_PAD.encode(br#"{"typ":"JWT","alg":"ES256"}"#);
    let claims = serde_json::json!({
        "aud": audience,
        "exp": expiry,
        "sub": contact,
    });
    let claims = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&claims)?);

    let signing_input = format!("{header}.{claims}");
    let signature: Signature = vapid.signing_key.sign(signing_input.as_bytes());
    let signature = URL_SAFE_NO_PAD.encode(signature.to_bytes());

    Ok(format!(
        "vapid t={signing_input}.{signature}, k={}",
        vapid.public_key_base64url()
    ))
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

        let authorization = vapid_authorization(&endpoint, &self.contact, vapid)?;
        let mut request = WebPushBuilder::new(endpoint, ua_public, ua_auth)
            .build(payload)
            .map_err(|e| anyhow::anyhow!("web push encryption failed: {e}"))?;
        request.headers_mut().insert(
            http::header::AUTHORIZATION,
            authorization
                .try_into()
                .map_err(|e| anyhow::anyhow!("bad VAPID header value: {e}"))?,
        );
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
