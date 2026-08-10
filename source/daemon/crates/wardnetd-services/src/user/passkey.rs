//! Passkeys (ADR-0031 §8).
//!
//! # The RP ID is pinned, and that has a visible consequence
//!
//! WebAuthn binds a credential to a **Relying Party ID** — a domain. A passkey
//! registered for `happy-einstein.wardnet.app` is unusable at any other origin,
//! by design and enforced by the browser. So the RP ID is pinned into
//! `system_config` at first registration and never silently changed: if it were,
//! every existing passkey in the household would stop working with no
//! explanation. Divergence from the live canonical FQDN therefore fails loudly
//! and the admin gets an explicit "reset passkeys" action.
//!
//! It is built with `allow_subdomains(true)` so one passkey covers the
//! published-app subdomains #1149 will add.
//!
//! # Why `:7411` cannot have passkeys, and why that is fine
//!
//! WebAuthn requires a secure context and a real domain. The plain-HTTP
//! pre-provisioning surface on `:7411`, and any bare-LAN-IP access, cannot
//! satisfy that — so passkey ceremonies return `412 Precondition Failed` there
//! rather than half-working. This is exactly why the local password can never be
//! removed: it is the only credential that works on a box with no certificate
//! and no public hostname.
//!
//! # Sign counts
//!
//! An authenticator that reports a counter must not report one that has gone
//! backwards; that is the signal of a cloned credential. `webauthn-rs` detects
//! it and we persist the updated counter, so the check has something to compare
//! against next time. Many passkey authenticators report a constant zero, which
//! is legal and is not a regression.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use webauthn_rs::prelude::*;

use crate::error::AppError;

/// `system_config` key holding the pinned Relying Party ID.
pub const KEY_PASSKEY_RP_ID: &str = "identity_passkey_rp_id";

/// Kind-specific JSON persisted alongside a passkey credential.
///
/// Holds the serialised `webauthn-rs` credential — the COSE public key and its
/// counters — because verification needs the whole thing back, not just an id.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PasskeyMetadata {
    /// The serialised credential.
    pub credential: Passkey,
    /// Last observed signature counter, mirrored out of the credential so a
    /// regression is auditable without deserialising.
    pub sign_count: u32,
    /// Whether the authenticator says the credential is backed up (synced to a
    /// platform keychain). Shown in the credential list, because a synced
    /// passkey and a hardware-bound one are different things to a person
    /// deciding whether to keep it.
    pub backup_eligible: bool,
    pub backup_state: bool,
}

/// Why a passkey ceremony cannot run.
///
/// Separate from a generic error so the API layer can map the precondition case
/// to `412` and say something actionable, instead of a bare 500.
#[derive(Debug)]
pub enum PasskeyUnavailable {
    /// No canonical public hostname, so there is no RP ID to use.
    NoCanonicalHostname,
    /// The request arrived at a host that is not the pinned RP ID.
    HostMismatch { pinned: String, requested: String },
}

impl From<PasskeyUnavailable> for AppError {
    fn from(value: PasskeyUnavailable) -> Self {
        match value {
            PasskeyUnavailable::NoCanonicalHostname => Self::PreconditionFailed(
                "passkeys need a public hostname and HTTPS; set up remote access first, \
                 and sign in with your password until then"
                    .to_owned(),
            ),
            PasskeyUnavailable::HostMismatch { pinned, requested } => Self::PreconditionFailed(
                format!(
                    "passkeys on this box are registered for {pinned}, but this request \
                     arrived at {requested}. Reach Wardnet at {pinned}, or ask an admin \
                     to reset passkeys."
                ),
            ),
        }
    }
}

/// The state carried across a registration ceremony.
pub struct PendingRegistration {
    pub user_id: Uuid,
    pub state: PasskeyRegistration,
}

/// The state carried across an authentication ceremony.
///
/// `DiscoverableAuthentication`, not `PasskeyAuthentication`: sign-in supplies no
/// username, so there is no allow-list of credentials to authenticate against —
/// the authenticator picks, and the assertion tells us which it chose.
pub struct PendingAuthentication {
    pub state: DiscoverableAuthentication,
}

/// Builds a `Webauthn` bound to the pinned RP ID.
///
/// A thin wrapper so the pinning rule and the `allow_subdomains` decision live
/// in exactly one place, and so every ceremony is forced through the host check.
pub struct PasskeyRelyingParty {
    system_config: Arc<dyn wardnetd_data::repository::SystemConfigRepository>,
}

impl PasskeyRelyingParty {
    #[must_use]
    pub fn new(
        system_config: Arc<dyn wardnetd_data::repository::SystemConfigRepository>,
    ) -> Self {
        Self { system_config }
    }

    /// The pinned RP ID, if one has been set.
    pub async fn pinned_rp_id(&self) -> Result<Option<String>, AppError> {
        self.system_config
            .get(KEY_PASSKEY_RP_ID)
            .await
            .map_err(AppError::Internal)
    }

    /// Pin `rp_id` as the household's Relying Party ID.
    ///
    /// Only ever called when nothing is pinned yet. Overwriting silently would
    /// invalidate every existing passkey with no explanation, which is why
    /// changing it is a separate, explicit admin action.
    pub async fn pin(&self, rp_id: &str) -> Result<(), AppError> {
        self.system_config
            .set(KEY_PASSKEY_RP_ID, rp_id)
            .await
            .map_err(AppError::Internal)?;
        tracing::info!(rp_id, "pinned passkey relying-party id: rp_id={rp_id}");
        Ok(())
    }

    /// Forget the pinned RP ID. Paired with deleting every passkey credential —
    /// on its own this would leave passkeys that can never be used again.
    pub async fn unpin(&self) -> Result<(), AppError> {
        self.system_config
            .delete(KEY_PASSKEY_RP_ID)
            .await
            .map_err(AppError::Internal)?;
        tracing::warn!("cleared the pinned passkey relying-party id");
        Ok(())
    }

    /// Build a `Webauthn` for a ceremony arriving at `request_host`.
    ///
    /// `canonical_fqdn` is the box's public hostname. The rules, in order:
    ///
    /// 1. No canonical hostname → refuse. There is nothing to be a Relying
    ///    Party for.
    /// 2. Nothing pinned yet → pin the canonical hostname now (first
    ///    registration wins).
    /// 3. Something pinned → the request host must match it. This is what makes
    ///    a `:7411` or bare-LAN-IP attempt fail loudly instead of registering a
    ///    passkey nobody can ever use.
    pub async fn for_request(
        &self,
        canonical_fqdn: Option<&str>,
        request_host: &str,
    ) -> Result<Webauthn, AppError> {
        let canonical = canonical_fqdn.ok_or(PasskeyUnavailable::NoCanonicalHostname)?;

        let rp_id = match self.pinned_rp_id().await? {
            Some(pinned) => pinned,
            None => {
                self.pin(canonical).await?;
                canonical.to_owned()
            }
        };

        // The `Host` header may carry a port; the RP ID never does.
        let host = request_host
            .split(':')
            .next()
            .unwrap_or(request_host)
            .to_lowercase();

        // A subdomain of the pinned RP ID is legitimate — that is the point of
        // `allow_subdomains`, and what lets one passkey cover published apps.
        let matches = host == rp_id || host.ends_with(&format!(".{rp_id}"));
        if !matches {
            return Err(PasskeyUnavailable::HostMismatch {
                pinned: rp_id,
                requested: host,
            }
            .into());
        }

        let origin = Url::parse(&format!("https://{rp_id}")).map_err(|e| {
            AppError::Internal(anyhow::anyhow!("pinned rp id is not a valid origin: {e}"))
        })?;

        WebauthnBuilder::new(&rp_id, &origin)
            .and_then(|b| {
                // One passkey should cover the published-app subdomains #1149
                // adds, rather than forcing a fresh registration per app.
                Ok(b.allow_subdomains(true)
                    .rp_name("Wardnet")
                    .build()?)
            })
            .map_err(|e| AppError::Internal(anyhow::anyhow!("failed to build webauthn: {e}")))
    }
}
