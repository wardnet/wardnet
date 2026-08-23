//! Private DNS (issue #912) — per-device grants for the `DoT` `:853` listener.
//!
//! ## Shape
//!
//! ```text
//! admin API / `DoT` runner ──(auth ctx)──▶ PrivateDnsService ──▶ private_dns_grants
//!                                          (auth-gated)      ├─▶ system_config (private_dns_enabled)
//!                                                            └─▶ DnsLocalService (split-horizon wildcard)
//! ```
//!
//! The admin grants a device → the service mints a high-entropy base-32
//! `token` that becomes the device's secret hostname label
//! (`<token>.<fqdn>`, covered by the existing wildcard SAN so it never
//! appears in CT logs). The `DoT` listener authenticates every connection by
//! resolving the SNI's token label back to the granted device — unknown
//! token or the bare apex means the connection is closed (closed resolver,
//! see #910).
//!
//! Premium-only v1: enabling requires the wardnet DNS provider, an active
//! certificate, and an active entitlement. Disabling (and revoking grants)
//! is always allowed, and entitlement loss persists a disable on the next
//! [`PrivateDnsService::reconcile`] — mirroring inbound-WireGuard.
//!
//! On enable, the service seeds a split-horizon wildcard `*.<fqdn>` A
//! record (source `System`) pointing at the LAN IP, so granted phones on
//! the LAN resolve their secret hostname straight to the Pi and reach the
//! local `:853` listener without hairpinning.

pub mod profile;
pub mod runner;

#[cfg(test)]
mod tests;

use std::net::Ipv4Addr;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use uuid::Uuid;
use wardnet_common::api::UpsertRecordRequest;
use wardnet_common::auth::AuthContext;
use wardnet_common::dns::{DnsRecordSource, DnsRecordType};
use wardnet_common::event::WardnetEvent;
use wardnetd_data::repository::private_dns::DeviceAlreadyGrantedPrivateDnsError;
use wardnetd_data::repository::{
    PrivateDnsGrantRepository, PrivateDnsGrantRow, SystemConfigRepository,
};

use crate::auth_context;
use crate::ddns::DdnsService;
use crate::device::DeviceService;
use crate::dns_local::DnsLocalService;
use crate::entitlement::Entitlement;
use crate::error::AppError;
use crate::event::EventPublisher;
use crate::push::PushService;
use crate::secret_store::SecretStore;
use crate::tls::{TlsService, TlsStatus};

/// TTL (seconds) for the split-horizon wildcard system record. Mirrors the
/// TLS split-horizon apex record.
const SYSTEM_RECORD_TTL_SECS: u32 = 300;

/// Random bytes per minted token: 10 bytes = 80 bits of entropy = exactly
/// 16 base-32 characters, the minimum label length the design calls for.
const TOKEN_BYTES: usize = 10;

/// RFC 4648 base-32 alphabet, lowercased — the token is a DNS label, and DNS
/// names are case-insensitive, so a mixed-case alphabet would silently halve
/// the effective entropy for clients that lowercase the name (they all do).
const BASE32_ALPHABET: &[u8; 32] = b"abcdefghijklmnopqrstuvwxyz234567";

/// Current Private-DNS state, surfaced to the admin UI and the `DoT` runner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrivateDnsStatus {
    /// Whether the feature (and therefore the `:853` listener) is enabled.
    pub enabled: bool,
    /// The canonical FQDN granted hostnames live under (`<token>.<domain>`),
    /// when the wardnet provider is configured.
    pub domain: Option<String>,
}

/// One per-device grant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrivateDnsGrant {
    pub id: Uuid,
    pub device_id: Uuid,
    /// The secret hostname label the device dials (`<token>.<fqdn>`).
    pub token: String,
    /// RFC 3339 creation timestamp.
    pub created_at: String,
}

impl PrivateDnsGrant {
    fn from_row(row: &PrivateDnsGrantRow) -> Result<Self, AppError> {
        Ok(Self {
            id: Uuid::parse_str(&row.id)
                .map_err(|e| AppError::Internal(anyhow::anyhow!("malformed grant id: {e}")))?,
            device_id: Uuid::parse_str(&row.device_id)
                .map_err(|e| AppError::Internal(anyhow::anyhow!("malformed device id: {e}")))?,
            token: row.token.clone(),
            created_at: row.created_at.clone(),
        })
    }
}

/// Abstraction over the `DoT` `:853` listener. Implemented in `wardnetd`
/// (which owns the serving stack); kept as a trait here so the lifecycle
/// runner and the health check stay unit-testable, mirroring
/// [`DnsServer`](crate::dns::server::DnsServer).
#[async_trait]
pub trait DotServer: Send + Sync {
    /// Bind `:853` and start accepting `DoT` connections.
    async fn start(&self) -> anyhow::Result<()>;

    /// Stop the listener and drain in-flight connections.
    async fn stop(&self) -> anyhow::Result<()>;

    /// Whether the listener is currently running.
    fn is_running(&self) -> bool;

    /// Terminate every live connection authenticated as `device_id` — called
    /// when the device's grant is revoked, so an established (and otherwise
    /// only handshake-time authenticated) session is cut off at once rather
    /// than surviving until it reconnects. A no-op default for test doubles;
    /// the real listener cancels the matching connections.
    fn disconnect_device(&self, device_id: Uuid) {
        let _ = device_id;
    }
}

/// Whether a [`TlsStatus`] means an issued certificate is live on `:443`
/// (and therefore available for the `DoT` listener to derive its config
/// from). `NeedsRenewal` counts — a cert in the renewal window still
/// serves, and the renewal runner is already on it.
#[must_use]
pub fn cert_ready(status: &TlsStatus) -> bool {
    matches!(
        status,
        TlsStatus::UpToDate { .. } | TlsStatus::NeedsRenewal { .. }
    )
}

/// Auth-gated Private-DNS operations (issue #912).
///
/// Every method requires an admin context except [`Self::reconcile`]
/// (startup/restore exception). The `DoT` listener calls
/// [`Self::resolve_token`] under the runner's nil-admin context, the same
/// way every background component reaches an auth-gated service.
#[async_trait]
pub trait PrivateDnsService: Send + Sync {
    /// Current enabled state + the domain granted hostnames live under.
    async fn status(&self) -> Result<PrivateDnsStatus, AppError>;

    /// Just the enabled flag — a single `system_config` read with no DDNS
    /// round-trip. The `DotRunner` and the health check gate on this rather
    /// than [`Self::status`] so a transient DDNS/config read error (which
    /// only affects the unused `domain`) can't flap a healthy listener.
    async fn is_enabled(&self) -> Result<bool, AppError>;

    /// Enable or disable Private DNS.
    ///
    /// Enabling is Premium-gated: it requires the wardnet DNS provider, an
    /// active (issued) certificate, and an active entitlement, and seeds the
    /// split-horizon wildcard record. Disabling is always allowed and removes
    /// the wildcard record best-effort.
    async fn set_enabled(&self, enabled: bool) -> Result<PrivateDnsStatus, AppError>;

    /// Grant a device encrypted-DNS access: mint its secret hostname token.
    /// Requires the feature to be enabled and one grant per device.
    async fn grant_device(&self, device_id: Uuid) -> Result<PrivateDnsGrant, AppError>;

    /// Revoke a grant. Deletes the token and, via
    /// [`WardnetEvent::PrivateDnsGrantRevoked`], tears down the device's
    /// live `DoT` sessions so revocation takes effect immediately, not only
    /// on the device's next reconnect.
    async fn revoke_grant(&self, grant_id: Uuid) -> Result<(), AppError>;

    /// Every grant, oldest first.
    async fn list_grants(&self) -> Result<Vec<PrivateDnsGrant>, AppError>;

    /// This device's grant, or `None` if it has none. Backs the device-keyed
    /// `GET /api/private-dns/me` surface, whose handler has already resolved
    /// the device by source IP and calls this under an internal admin context.
    /// A default `Ok(None)` keeps test doubles that predate it compiling.
    async fn device_grant(&self, device_id: Uuid) -> Result<Option<PrivateDnsGrant>, AppError> {
        let _ = device_id;
        Ok(None)
    }

    /// Nudge a granted device's household member to set up Private DNS via a
    /// device-keyed push deep-linking the user PWA to `/settings#private-dns`,
    /// where the Private DNS card carries the setup steps (#916). Returns
    /// whether any push subscription was targeted (`delivered`); a granted
    /// device with no subscription yields `Ok(false)`, not an error. A device
    /// with no grant is a 404 ([`AppError::NotFound`]), mirroring
    /// [`Self::device_grant`]-backed `delete_grant`. Admin only. A default
    /// `Ok(false)` keeps test doubles that predate it compiling.
    async fn notify_device(&self, device_id: Uuid) -> Result<bool, AppError> {
        let _ = device_id;
        Ok(false)
    }

    /// The signed `.mobileconfig` for this device, or `None` when the device
    /// isn't granted, the feature is disabled, or the wardnet domain / cert
    /// isn't available yet (all of which surface as a 404). Backs the
    /// device-keyed `GET /api/private-dns/me/profile`. Defaults to `Ok(None)`.
    async fn device_profile(&self, device_id: Uuid) -> Result<Option<Vec<u8>>, AppError> {
        let _ = device_id;
        Ok(None)
    }

    /// Resolve a `DoT` SNI token label to its grant, or `None` for an unknown
    /// token (the listener closes those connections). Pure lookup — the
    /// listener decides policy.
    async fn resolve_token(&self, token: &str) -> Result<Option<PrivateDnsGrant>, AppError>;

    /// Startup reconcile: persist a disable when the entitlement lapsed while
    /// the daemon was down, so a lapsed box never resurrects a Premium
    /// feature. No auth guard — runs before the system is ready (startup /
    /// restore exception), and touches only repositories.
    async fn reconcile(&self) -> Result<(), AppError>;
}

/// The concrete [`PrivateDnsService`].
pub struct PrivateDnsServiceImpl {
    grants: Arc<dyn PrivateDnsGrantRepository>,
    system_config: Arc<dyn SystemConfigRepository>,
    devices: Arc<dyn DeviceService>,
    ddns: Arc<dyn DdnsService>,
    tls: Arc<dyn TlsService>,
    /// Used to maintain the split-horizon wildcard `*.<fqdn> → lan_ip` system
    /// record. Reached through the auth-gated service (never a repository);
    /// calls inherit the caller's admin `auth_context`.
    dns_local: Arc<dyn DnsLocalService>,
    entitlement: Arc<Entitlement>,
    events: Arc<dyn EventPublisher>,
    /// Source of the live Let's Encrypt cert/key used to CMS-sign the iOS
    /// `.mobileconfig` (issue #914). Reached only from `device_profile`.
    secrets: Arc<dyn SecretStore>,
    /// Delivers the device-keyed "Private DNS is ready" nudge. A direct
    /// service-to-service call (not a synthetic event): notifying is an explicit
    /// admin resend action, not an organic state change (issue #915).
    push: Arc<dyn PushService>,
    /// The Pi's LAN-interface IPv4 — the value of the wildcard record.
    lan_ip: Ipv4Addr,
}

impl PrivateDnsServiceImpl {
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        grants: Arc<dyn PrivateDnsGrantRepository>,
        system_config: Arc<dyn SystemConfigRepository>,
        devices: Arc<dyn DeviceService>,
        ddns: Arc<dyn DdnsService>,
        tls: Arc<dyn TlsService>,
        dns_local: Arc<dyn DnsLocalService>,
        entitlement: Arc<Entitlement>,
        events: Arc<dyn EventPublisher>,
        secrets: Arc<dyn SecretStore>,
        push: Arc<dyn PushService>,
        lan_ip: Ipv4Addr,
    ) -> Self {
        Self {
            grants,
            system_config,
            devices,
            ddns,
            tls,
            dns_local,
            entitlement,
            events,
            secrets,
            push,
            lan_ip,
        }
    }

    /// This device's grant row mapped to a [`PrivateDnsGrant`], or `None`.
    async fn grant_for_device(&self, device_id: Uuid) -> Result<Option<PrivateDnsGrant>, AppError> {
        let row = self
            .grants
            .find_by_device_id(&device_id.to_string())
            .await
            .map_err(AppError::Internal)?;
        row.as_ref().map(PrivateDnsGrant::from_row).transpose()
    }

    fn require_entitled(&self) -> Result<(), AppError> {
        if self.entitlement.is_entitled() {
            Ok(())
        } else {
            Err(AppError::Forbidden(
                "Private DNS requires an active Premium subscription".to_owned(),
            ))
        }
    }

    fn publish_changed(&self, enabled: bool) {
        self.events.publish(WardnetEvent::PrivateDnsChanged {
            enabled,
            timestamp: Utc::now(),
        });
    }

    async fn enabled(&self) -> Result<bool, AppError> {
        self.system_config
            .private_dns_enabled()
            .await
            .map_err(AppError::Internal)
    }

    /// The wildcard domain granted hostnames live under — the wardnet FQDN.
    /// `None` when DDNS is unconfigured or on a non-wardnet provider.
    ///
    /// Requires an admin context (`DdnsService::status` is auth-gated).
    async fn wardnet_domain(&self) -> Result<Option<String>, AppError> {
        let status = self.ddns.status().await?;
        if status.provider.as_deref() == Some("wardnet") {
            Ok(status.fqdn)
        } else {
            Ok(None)
        }
    }

    /// Seed the split-horizon wildcard `*.<domain> → lan_ip` system record so
    /// granted phones on the LAN resolve their secret hostname to the Pi.
    ///
    /// **Non-fatal:** the grant/enable is already persisted by design intent;
    /// a local-DNS failure is logged, and roaming (which never consults the
    /// LAN view) is unaffected. Mirrors `TlsServiceImpl::reconcile_split_horizon`.
    async fn ensure_wildcard_record(&self, domain: &str) {
        let lan_ip = self.lan_ip;
        let wildcard = format!("*.{domain}");
        let req = UpsertRecordRequest {
            zone_id: None,
            domain: wildcard.clone(),
            record_type: DnsRecordType::A,
            value: lan_ip.to_string(),
            ttl: SYSTEM_RECORD_TTL_SECS,
            enabled: true,
            source: DnsRecordSource::System,
        };
        match self.dns_local.upsert_record(req).await {
            Ok(_) => tracing::info!(
                domain = %wildcard, %lan_ip,
                "seeded split-horizon wildcard record {wildcard} -> {lan_ip}"
            ),
            Err(e) => tracing::warn!(
                error = %e, domain = %wildcard,
                "failed to seed split-horizon wildcard record for {wildcard}; LAN DoT clients will hairpin via WAN"
            ),
        }
    }

    /// Remove the daemon-seeded wildcard `System` A record for `domain`.
    /// Non-fatal — a leftover record is harmless once the listener is down.
    async fn remove_wildcard_record(&self, domain: &str) {
        let wildcard = format!("*.{domain}");
        let records = match self.dns_local.list_records().await {
            Ok(resp) => resp.records,
            Err(e) => {
                tracing::warn!(error = %e, domain = %wildcard, "failed to list records to remove wildcard record for {wildcard}");
                return;
            }
        };
        for rec in records.into_iter().filter(|r| {
            r.domain == wildcard
                && r.record_type == DnsRecordType::A
                && r.source == DnsRecordSource::System
        }) {
            if let Err(e) = self.dns_local.delete_record(rec.id).await {
                tracing::warn!(error = %e, domain = %wildcard, "failed to delete wildcard record for {wildcard}");
            } else {
                tracing::info!(domain = %wildcard, "removed split-horizon wildcard record for {wildcard}");
            }
        }
    }
}

/// Encode `bytes` as lowercase RFC 4648 base-32 (no padding).
fn encode_base32(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(5) * 8);
    let mut acc: u32 = 0;
    let mut bits = 0u32;
    for &b in bytes {
        acc = (acc << 8) | u32::from(b);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            out.push(char::from(BASE32_ALPHABET[((acc >> bits) & 0x1f) as usize]));
        }
    }
    if bits > 0 {
        out.push(char::from(
            BASE32_ALPHABET[((acc << (5 - bits)) & 0x1f) as usize],
        ));
    }
    out
}

/// Mint a fresh token: [`TOKEN_BYTES`] bytes from the CSPRNG, base-32 encoded.
fn mint_token() -> String {
    let bytes: [u8; TOKEN_BYTES] = rand::random();
    encode_base32(&bytes)
}

#[async_trait]
impl PrivateDnsService for PrivateDnsServiceImpl {
    async fn status(&self) -> Result<PrivateDnsStatus, AppError> {
        auth_context::require_admin()?;
        Ok(PrivateDnsStatus {
            enabled: self.enabled().await?,
            domain: self.wardnet_domain().await?,
        })
    }

    async fn is_enabled(&self) -> Result<bool, AppError> {
        auth_context::require_admin()?;
        self.enabled().await
    }

    async fn set_enabled(&self, enabled: bool) -> Result<PrivateDnsStatus, AppError> {
        auth_context::require_admin()?;

        if enabled {
            // Enabling is Premium-gated; disabling is always allowed so a
            // lapsed or reconfigured box can still be switched off cleanly.
            self.require_entitled()?;

            let Some(domain) = self.wardnet_domain().await? else {
                return Err(AppError::Conflict(
                    "Private DNS requires the wardnet DNS provider".to_owned(),
                ));
            };
            if !cert_ready(&self.tls.status().await?) {
                return Err(AppError::Conflict(
                    "Private DNS requires an issued TLS certificate".to_owned(),
                ));
            }

            self.ensure_wildcard_record(&domain).await;
            self.system_config
                .set_private_dns_enabled(true)
                .await
                .map_err(AppError::Internal)?;
            self.publish_changed(true);
            return Ok(PrivateDnsStatus {
                enabled: true,
                domain: Some(domain),
            });
        }

        let domain = self.wardnet_domain().await.unwrap_or_else(|e| {
            // Best-effort: a torn-down provider must not block the disable.
            tracing::warn!(error = %e, "could not resolve domain while disabling Private DNS");
            None
        });
        if let Some(domain) = &domain {
            self.remove_wildcard_record(domain).await;
        }
        self.system_config
            .set_private_dns_enabled(false)
            .await
            .map_err(AppError::Internal)?;
        self.publish_changed(false);
        Ok(PrivateDnsStatus {
            enabled: false,
            domain,
        })
    }

    async fn grant_device(&self, device_id: Uuid) -> Result<PrivateDnsGrant, AppError> {
        auth_context::require_admin()?;
        self.require_entitled()?;

        if !self.enabled().await? {
            return Err(AppError::Conflict("Private DNS is disabled".to_owned()));
        }

        let device_key = device_id.to_string();
        if self.devices.get_device(&device_key).await?.is_none() {
            return Err(AppError::NotFound(format!("device {device_id} not found")));
        }
        // Friendly pre-check; the UNIQUE index still backstops the race below.
        if self
            .grants
            .find_by_device_id(&device_key)
            .await
            .map_err(AppError::Internal)?
            .is_some()
        {
            return Err(AppError::Conflict(
                "device already has a Private DNS grant".to_owned(),
            ));
        }

        let row = PrivateDnsGrantRow {
            id: Uuid::new_v4().to_string(),
            device_id: device_key,
            token: mint_token(),
            created_at: Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        };
        if let Err(e) = self.grants.insert(&row).await {
            if e.downcast_ref::<DeviceAlreadyGrantedPrivateDnsError>()
                .is_some()
            {
                return Err(AppError::Conflict(
                    "device already has a Private DNS grant".to_owned(),
                ));
            }
            return Err(AppError::Internal(e));
        }

        // A grant is an admin configuration act, so the device becomes managed
        // and thereby exempt from the retention prune (issue #1181). This is
        // the case that most needs it: the grant's secret hostname is what a
        // roaming phone resolves through, and that phone can easily be absent
        // from the LAN for well over 30 days. Under the old
        // `managed = name IS NOT NULL` inference, an unnamed device would have
        // had its row — and, by cascade, its live grant — deleted underneath a
        // working configuration.
        self.devices.mark_managed(&row.device_id).await?;

        // Grant mutations re-emit the change event (enabled is true here —
        // granting requires it) so listeners holding token state refresh.
        self.publish_changed(true);

        // Announce the grant itself so `AccessRequestListener` can resolve any
        // pending Private-DNS request for this device. It goes over the bus
        // rather than through a direct call because approving such a request
        // already depends on *this* service — calling back the other way would
        // close a cycle. Carries the acting admin so the resolved request
        // records a truthful `decided_by`.
        //
        // Enumerated rather than a bare `Some(User(..))`, because
        // `AuthContext::system()` *is* a `User` — the nil UUID carrying the
        // admin role. Matching it would stamp `00000000-…-0000` into
        // `decided_by`, an id ADR-0031 guarantees has no `users` row, so the
        // admin UI would show a decision made by nobody. A grant with no human
        // behind it records no decider at all.
        let granted_by = match auth_context::try_current() {
            Some(AuthContext::User(user)) if user.user_id() != Uuid::nil() => {
                Some(user.user_id().to_string())
            }
            Some(AuthContext::User(_) | AuthContext::Device { .. } | AuthContext::Anonymous)
            | None => None,
        };
        self.events.publish(WardnetEvent::PrivateDnsGrantCreated {
            device_id,
            granted_by,
            timestamp: Utc::now(),
        });

        PrivateDnsGrant::from_row(&row)
    }

    async fn revoke_grant(&self, grant_id: Uuid) -> Result<(), AppError> {
        auth_context::require_admin()?;

        let key = grant_id.to_string();
        let Some(row) = self
            .grants
            .find_by_id(&key)
            .await
            .map_err(AppError::Internal)?
        else {
            return Err(AppError::NotFound(format!("grant {grant_id} not found")));
        };
        self.grants.delete(&key).await.map_err(AppError::Internal)?;

        // Cut off the device's already-established DoT sessions: the token is
        // otherwise only checked at handshake, so without this a revoked
        // (lost/compromised) device keeps resolving until it reconnects.
        if let Ok(device_id) = Uuid::parse_str(&row.device_id) {
            self.events.publish(WardnetEvent::PrivateDnsGrantRevoked {
                device_id,
                timestamp: Utc::now(),
            });
        }
        self.publish_changed(self.enabled().await?);
        Ok(())
    }

    async fn list_grants(&self) -> Result<Vec<PrivateDnsGrant>, AppError> {
        auth_context::require_admin()?;
        let rows = self.grants.find_all().await.map_err(AppError::Internal)?;
        rows.iter().map(PrivateDnsGrant::from_row).collect()
    }

    async fn device_grant(&self, device_id: Uuid) -> Result<Option<PrivateDnsGrant>, AppError> {
        auth_context::require_admin()?;
        self.grant_for_device(device_id).await
    }

    async fn notify_device(&self, device_id: Uuid) -> Result<bool, AppError> {
        auth_context::require_admin()?;

        // A device with no grant is a 404 — the same guard `delete_grant`
        // applies (there is nothing to nudge a household member about yet).
        if self.grant_for_device(device_id).await?.is_none() {
            return Err(AppError::NotFound(format!(
                "device {device_id} has no Private DNS grant"
            )));
        }

        // Delegate targeting to the push service: it owns the device UUID -> MAC
        // resolution and the subscription fan-out, and reports whether any
        // subscription existed (the API's `delivered`). Runs under this admin
        // context, satisfying the push method's own `require_admin`.
        self.push.notify_private_dns_granted(device_id).await
    }

    async fn device_profile(&self, device_id: Uuid) -> Result<Option<Vec<u8>>, AppError> {
        auth_context::require_admin()?;

        // Every miss here is a legitimate 404, not an error: an ungranted or
        // disabled device simply has no profile, and a box without the wardnet
        // domain / issued cert can't mint one yet.
        if !self.enabled().await? {
            return Ok(None);
        }
        let Some(domain) = self.wardnet_domain().await? else {
            return Ok(None);
        };
        let Some(grant) = self.grant_for_device(device_id).await? else {
            return Ok(None);
        };
        let Some((chain_pem, key_pem)) = crate::tls::load_stored_cert(self.secrets.as_ref())
            .await
            .map_err(AppError::Internal)?
        else {
            return Ok(None);
        };

        let hostname = format!("{}.{}", grant.token, domain);
        let signed = profile::build_signed_profile(&hostname, &chain_pem, &key_pem)
            .map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;
        Ok(Some(signed))
    }

    async fn resolve_token(&self, token: &str) -> Result<Option<PrivateDnsGrant>, AppError> {
        auth_context::require_admin()?;
        let row = self
            .grants
            .find_by_token(token)
            .await
            .map_err(AppError::Internal)?;
        row.as_ref().map(PrivateDnsGrant::from_row).transpose()
    }

    async fn reconcile(&self) -> Result<(), AppError> {
        // No auth guard: runs at startup before any admin exists (startup /
        // restore exception), and deliberately touches only repositories —
        // the wildcard record is a stored custom record that survives
        // restarts on its own.
        if self.enabled().await? && !self.entitlement.is_entitled() {
            tracing::warn!(
                "Private DNS was enabled but the entitlement is inactive; persisting a disable"
            );
            self.system_config
                .set_private_dns_enabled(false)
                .await
                .map_err(AppError::Internal)?;
            self.publish_changed(false);
        }
        Ok(())
    }
}
