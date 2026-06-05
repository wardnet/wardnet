//! The wardnet **bridge** DDNS provider (default, zero-config).
//!
//! Two concerns live here:
//!
//! * **Registration** ([`register_install`], [`check_name_available`]) — the
//!   one-time bootstrap that mints an installation on a bridge. It generates a
//!   fresh Ed25519 keypair, solves the bridge's proof-of-work challenge, and
//!   `POST`s `/v1/register`. The returned [`BridgeIdentity`] (install id, bearer
//!   token, assigned FQDN, region, signing seed) is what the service persists.
//!   Registration is *not* on the [`DnsProvider`] trait — only the bridge
//!   registers; Cloudflare does not.
//!
//! * **Steady-state publishing** ([`BridgeProvider`]) — once registered, every
//!   call to the install-owned endpoints (`/v1/installs/{id}/…`) is signed with
//!   the install's Ed25519 key. The bridge keys records by **install id**, so
//!   the provider never constructs an FQDN; the bridge owns it.
//!
//! The canonical signing payload and `PoW` derivation here must match the
//! bridge's verifier byte-for-byte (`source/bridge/src/auth/middleware.rs`,
//! `…/api/challenge.rs`) — the daemon and bridge are separate workspaces, so
//! the provider tests pin the format by cryptographically verifying it.

use std::fmt::Write as _;
use std::net::Ipv4Addr;

use async_trait::async_trait;
use base64::Engine as _;
use chrono::Utc;
use ed25519_dalek::{Signer, SigningKey};
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::provider::DnsProvider;

/// HTTP header carrying the request's Unix-second timestamp (±60 s window).
const TIMESTAMP_HEADER: &str = "X-Wardnet-Timestamp";
/// HTTP header carrying the base64 Ed25519 signature over the canonical payload.
const SIGNATURE_HEADER: &str = "X-Wardnet-Signature";

/// Identity minted by a successful bridge registration.
///
/// The non-secret fields are persisted in `system_config`; `signing_key_seed`
/// and `bearer_token` go to the on-Pi [`SecretStore`](crate::secret_store::SecretStore).
#[derive(Clone)]
pub struct BridgeIdentity {
    /// Server-assigned installation UUID; appears in every `/v1/installs/{id}/…` path.
    pub install_id: String,
    /// Opaque bearer token. The bridge stores only `SHA-256(token)`; this is the
    /// only time the raw value is available.
    pub bearer_token: String,
    /// Fully-qualified subdomain assigned by the bridge, e.g.
    /// `happy-einstein.my.us.wardnet.network`. The bridge owns this string.
    pub subdomain: String,
    /// Region the registering bridge serves (its own slug, e.g. `us`).
    pub region: String,
    /// 32-byte Ed25519 seed. Never leaves the Pi.
    pub signing_key_seed: [u8; 32],
}

// Manual `Debug` that redacts the two secret fields, so a stray `?identity`
// log or error-wrap can never leak the bearer token or Ed25519 seed. Mirrors
// the redaction `SecretEntry` already applies in the secret store.
impl std::fmt::Debug for BridgeIdentity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BridgeIdentity")
            .field("install_id", &self.install_id)
            .field("bearer_token", &"<redacted>")
            .field("subdomain", &self.subdomain)
            .field("region", &self.region)
            .field("signing_key_seed", &"<redacted>")
            .finish()
    }
}

/// A bridge provider bound to one registered installation.
///
/// Construct with [`BridgeProvider::new`] from the persisted [`BridgeIdentity`]
/// fields. All trait calls sign with the install's Ed25519 key.
pub struct BridgeProvider {
    client: reqwest::Client,
    /// Bridge endpoint base URL, e.g. `https://bridge.use1.wardnet.network`.
    base_url: String,
    install_id: String,
    bearer_token: String,
    signing_key: SigningKey,
}

impl BridgeProvider {
    /// Build a provider from a persisted identity.
    #[must_use]
    pub fn new(
        client: reqwest::Client,
        base_url: String,
        install_id: String,
        bearer_token: String,
        signing_key_seed: [u8; 32],
    ) -> Self {
        Self {
            client,
            base_url,
            install_id,
            bearer_token,
            signing_key: SigningKey::from_bytes(&signing_key_seed),
        }
    }

    /// Compute the base64 Ed25519 signature over the canonical payload
    /// `"<METHOD>\n<path_and_query>\n<ts>\n<hex-sha256(body)>"`.
    fn sign(&self, method: &str, path_and_query: &str, timestamp: i64, body: &[u8]) -> String {
        let body_hash = hex::encode(Sha256::digest(body));
        let payload = format!("{method}\n{path_and_query}\n{timestamp}\n{body_hash}");
        let signature = self.signing_key.sign(payload.as_bytes());
        base64::engine::general_purpose::STANDARD.encode(signature.to_bytes())
    }

    /// Send a signed request to an install-owned endpoint.
    ///
    /// `path` is the request path (no query for our endpoints) and doubles as
    /// the signed `path_and_query`. When `body` is `Some`, it is serialized
    /// once and the *same* bytes are both signed and sent, so the bridge's
    /// `hex-sha256(body)` matches.
    async fn send_signed(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<serde_json::Value>,
    ) -> anyhow::Result<()> {
        let timestamp = Utc::now().timestamp();
        let body_bytes = match &body {
            Some(value) => serde_json::to_vec(value)?,
            None => Vec::new(),
        };
        let signature = self.sign(method.as_str(), path, timestamp, &body_bytes);

        let mut request = self
            .client
            .request(method, format!("{}{path}", self.base_url))
            .header(AUTHORIZATION, format!("Bearer {}", self.bearer_token))
            .header(TIMESTAMP_HEADER, timestamp.to_string())
            .header(SIGNATURE_HEADER, signature);
        if body.is_some() {
            request = request
                .header(CONTENT_TYPE, "application/json")
                .body(body_bytes);
        }

        let response = request.send().await?;
        ensure_success(response, path).await
    }

    /// Path of this install's IP-update endpoint.
    fn ip_path(&self) -> String {
        format!("/v1/installs/{}/ip", self.install_id)
    }

    /// Path of this install's ACME-challenge endpoint.
    fn acme_path(&self) -> String {
        format!("/v1/installs/{}/acme-challenge", self.install_id)
    }
}

#[async_trait]
impl DnsProvider for BridgeProvider {
    async fn upsert_a(&self, ip: Ipv4Addr) -> anyhow::Result<()> {
        let body = serde_json::json!({ "ip": ip.to_string() });
        self.send_signed(reqwest::Method::PUT, &self.ip_path(), Some(body))
            .await
    }

    async fn set_txt(&self, value: &str) -> anyhow::Result<()> {
        let body = serde_json::json!({ "value": value });
        self.send_signed(reqwest::Method::PUT, &self.acme_path(), Some(body))
            .await
    }

    async fn delete_txt(&self) -> anyhow::Result<()> {
        self.send_signed(reqwest::Method::DELETE, &self.acme_path(), None)
            .await
    }
}

// ── Registration (bridge-only bootstrap) ───────────────────────────────────────

/// Register a new installation on the bridge at `base_url` under `name`.
///
/// Generates a fresh Ed25519 keypair, fetches and solves the `PoW` challenge,
/// and posts `/v1/register`. Returns the minted [`BridgeIdentity`]. The caller
/// persists it; this function holds no state.
pub async fn register_install(
    client: &reqwest::Client,
    base_url: &str,
    name: &str,
) -> anyhow::Result<BridgeIdentity> {
    // 1. Generate the keypair from random seed bytes (avoids depending on
    //    ed25519-dalek's `rand_core` feature, which pins an older rand_core).
    let mut seed = [0u8; 32];
    rand::fill(&mut seed);
    let signing_key = SigningKey::from_bytes(&seed);
    let public_key_b64 =
        base64::engine::general_purpose::STANDARD.encode(signing_key.verifying_key().to_bytes());

    // 2. Fetch a PoW challenge.
    let challenge: ChallengeResponse = client
        .get(format!("{base_url}/v1/register/challenge"))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    // 3. Solve the PoW (difficulty comes from the challenge).
    let proof = solve_pow(
        &challenge.nonce,
        name,
        &public_key_b64,
        challenge.difficulty,
    )?;

    // 4. Register.
    let body = RegisterRequest {
        name: name.to_owned(),
        public_key: public_key_b64,
        challenge_id: challenge.challenge_id,
        proof,
    };
    let response = client
        .post(format!("{base_url}/v1/register"))
        .json(&body)
        .send()
        .await?;
    if !response.status().is_success() {
        let status = response.status();
        let detail = response.text().await.unwrap_or_default();
        anyhow::bail!("bridge registration failed: HTTP {status}: {detail}");
    }
    let registered: RegisterResponse = response.json().await?;

    Ok(BridgeIdentity {
        install_id: registered.id,
        bearer_token: registered.bearer_token,
        subdomain: registered.subdomain,
        region: registered.region,
        signing_key_seed: seed,
    })
}

/// Check whether `name` is available on the bridge at `base_url`.
pub async fn check_name_available(
    client: &reqwest::Client,
    base_url: &str,
    name: &str,
) -> anyhow::Result<bool> {
    let response: NameAvailabilityResponse = client
        .get(format!("{base_url}/v1/names/{name}/available"))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(response.available)
}

/// Upper bound on the `PoW` difficulty the daemon will attempt.
///
/// The bridge's legitimate value is 24 bits (~16 M hashes, ~160 ms on a Pi).
/// Capping the accepted difficulty stops a misconfigured or hostile bridge
/// from pinning a CPU core indefinitely with an unsatisfiable challenge — the
/// expected work is `2^difficulty`, so an uncapped 64-bit demand would never
/// return. 28 bits leaves generous headroom over the real value.
const MAX_POW_DIFFICULTY: u32 = 28;

/// Solve the bridge proof-of-work: find a `u64` proof such that
/// `SHA256(nonce\nname\npublic_key\nproof_decimal)` has at least `difficulty`
/// leading zero bits. Mirrors `verify_pow` in the bridge.
///
/// Returns an error if `difficulty` exceeds [`MAX_POW_DIFFICULTY`], bounding the
/// work the daemon will perform on a bridge's say-so.
pub fn solve_pow(
    nonce: &str,
    name: &str,
    public_key: &str,
    difficulty: u32,
) -> anyhow::Result<u64> {
    if difficulty > MAX_POW_DIFFICULTY {
        anyhow::bail!(
            "bridge demanded PoW difficulty {difficulty}, above the {MAX_POW_DIFFICULTY}-bit cap"
        );
    }
    let prefix = format!("{nonce}\n{name}\n{public_key}\n");
    // Reuse one buffer for the decimal proof rather than allocating a fresh
    // `String` per iteration (the loop can run millions of times).
    let mut decimal = String::with_capacity(20);
    let mut proof = 0u64;
    loop {
        decimal.clear();
        write!(decimal, "{proof}").expect("writing into a String is infallible");
        let mut hasher = Sha256::new();
        hasher.update(prefix.as_bytes());
        hasher.update(decimal.as_bytes());
        if leading_zero_bits(&hasher.finalize()) >= difficulty {
            return Ok(proof);
        }
        proof += 1;
    }
}

/// Count leading zero bits of a hash, byte by byte. Matches the bridge.
#[must_use]
fn leading_zero_bits(hash: &[u8]) -> u32 {
    let mut bits = 0u32;
    for byte in hash {
        let z = byte.leading_zeros();
        bits += z;
        if z < 8 {
            break;
        }
    }
    bits
}

/// Map a non-success bridge response into an error carrying the body text.
async fn ensure_success(response: reqwest::Response, path: &str) -> anyhow::Result<()> {
    let status = response.status();
    if status.is_success() {
        return Ok(());
    }
    let detail = response.text().await.unwrap_or_default();
    anyhow::bail!("bridge request to {path} failed: HTTP {status}: {detail}")
}

// ── Wire types (mirror the bridge API contract) ────────────────────────────────

#[derive(Debug, Deserialize)]
struct ChallengeResponse {
    challenge_id: String,
    nonce: String,
    difficulty: u32,
    #[allow(dead_code)]
    expires_at: String,
}

#[derive(Debug, Serialize)]
struct RegisterRequest {
    name: String,
    public_key: String,
    challenge_id: String,
    proof: u64,
}

#[derive(Debug, Deserialize)]
struct RegisterResponse {
    id: String,
    bearer_token: String,
    subdomain: String,
    region: String,
}

#[derive(Debug, Deserialize)]
struct NameAvailabilityResponse {
    available: bool,
}
