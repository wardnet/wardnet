//! Push notifications — VAPID key management, subscription CRUD, and
//! event-driven Web Push delivery (issue #440).
//!
//! ## Shape
//!
//! ```text
//! PushNotificationListener ──(admin ctx)──▶ PushService::handle_event
//!  (event bus, daemon bin)                   ├─ resolve device/tunnel labels
//!                                             ├─ pick audience (device | admins)
//!                                             └─ WebPushSender::send ── prune on Gone
//!
//! HTTP handlers ──▶ PushService::{vapid_public_key, subscribe, unsubscribe}
//! ```
//!
//! The [`WebPushSender`] seam keeps all crypto + network I/O out of the
//! mapping logic, so the audience/label decisions are unit-tested with a
//! recording mock and the mock daemon no-ops delivery.
//!
//! ## Storage
//!
//! The VAPID **private** key lives in the [`SecretStore`] at
//! [`SECRET_VAPID_KEY`]; the browser-facing public key is cached (non-secret)
//! in `system_config` under [`KEY_VAPID_PUBLIC`]. The key pair is generated
//! once, lazily, on first use and **never rotated** — rotation invalidates
//! every existing subscription. Subscriptions live in `push_subscriptions`
//! (see [`PushRepository`]).

pub mod sender;

#[cfg(test)]
mod tests;

use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::OnceCell;
use wardnet_common::api::WebPushSubscription;
use wardnet_common::auth::AuthContext;
use wardnet_common::event::WardnetEvent;
use wardnet_common::routing::{RoutingTarget, RuleCreator};
use wardnetd_data::repository::push::{OWNER_KIND_ADMIN, OWNER_KIND_DEVICE};
use wardnetd_data::repository::{
    DeviceRepository, NewPushSubscription, PushRepository, StoredPushSubscription,
    SystemConfigRepository, TunnelRepository,
};

use crate::auth_context;
use crate::error::AppError;
use crate::secret_store::SecretStore;

use self::sender::{PushTarget, SendOutcome, VapidKey, WebPushSender};

/// [`SecretStore`] path holding the raw VAPID private key bytes.
pub const SECRET_VAPID_KEY: &str = "push/vapid/private_key";
/// `system_config` key caching the base64url VAPID public (application server)
/// key for the unauthenticated public-key endpoint.
pub const KEY_VAPID_PUBLIC: &str = "push_vapid_public_key";
/// VAPID `sub` contact advertised to push services (RFC 8292).
pub const VAPID_CONTACT: &str = "mailto:push@wardnet.network";

/// A rendered notification: the title + body shown by the service worker.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Notification {
    title: &'static str,
    body: String,
}

impl Notification {
    fn to_json_bytes(&self) -> Vec<u8> {
        // Minimal, stable JSON the PWA service worker reads. Kept hand-rolled
        // (rather than serde) so the exact wire shape is obvious in review.
        serde_json::json!({ "title": self.title, "body": self.body })
            .to_string()
            .into_bytes()
    }
}

#[async_trait]
pub trait PushService: Send + Sync {
    /// The base64url VAPID application server key. Unauthenticated: the browser
    /// needs it to build a subscription before any identity exists.
    async fn vapid_public_key(&self) -> Result<String, AppError>;

    /// Register (or refresh) the calling context's subscription. Admin callers
    /// are keyed to their account; device callers to their MAC.
    async fn subscribe(&self, sub: WebPushSubscription) -> Result<(), AppError>;

    /// Remove the caller's subscription(s). With `endpoint`, only that one;
    /// without, every subscription the caller owns.
    async fn unsubscribe(&self, endpoint: Option<String>) -> Result<(), AppError>;

    /// Translate a domain event into push notifications and deliver them.
    /// Invoked by the daemon's event listener under an admin context.
    async fn handle_event(&self, event: &WardnetEvent) -> Result<(), AppError>;
}

pub struct PushServiceImpl {
    push_repo: Arc<dyn PushRepository>,
    device_repo: Arc<dyn DeviceRepository>,
    tunnel_repo: Arc<dyn TunnelRepository>,
    system_config: Arc<dyn SystemConfigRepository>,
    secrets: Arc<dyn SecretStore>,
    sender: Arc<dyn WebPushSender>,
    /// Lazily generated once, then cached for the process lifetime.
    vapid: OnceCell<Arc<VapidKey>>,
}

impl PushServiceImpl {
    #[must_use]
    pub fn new(
        push_repo: Arc<dyn PushRepository>,
        device_repo: Arc<dyn DeviceRepository>,
        tunnel_repo: Arc<dyn TunnelRepository>,
        system_config: Arc<dyn SystemConfigRepository>,
        secrets: Arc<dyn SecretStore>,
        sender: Arc<dyn WebPushSender>,
    ) -> Self {
        Self {
            push_repo,
            device_repo,
            tunnel_repo,
            system_config,
            secrets,
            sender,
            vapid: OnceCell::new(),
        }
    }

    /// Load the VAPID key pair, generating + persisting it on first call.
    async fn ensure_vapid(&self) -> Result<Arc<VapidKey>, AppError> {
        let key = self
            .vapid
            .get_or_try_init(|| async {
                if let Some(bytes) = self.secrets.get(SECRET_VAPID_KEY).await? {
                    return Ok::<_, AppError>(Arc::new(VapidKey::from_bytes(&bytes)?));
                }
                // First run after setup: mint the key pair, store the private
                // bytes in the vault, and cache the public key for the
                // unauthenticated endpoint.
                let key = VapidKey::generate();
                self.secrets.put(SECRET_VAPID_KEY, &key.to_bytes()).await?;
                self.system_config
                    .set(KEY_VAPID_PUBLIC, &key.public_key_base64url())
                    .await?;
                tracing::info!("push: generated VAPID key pair");
                Ok(Arc::new(key))
            })
            .await?;
        Ok(key.clone())
    }

    /// The (`owner_kind`, `owner_key`) storage key for the current caller, or
    /// an error if the caller is anonymous.
    fn caller_owner() -> Result<(&'static str, String), AppError> {
        match auth_context::current() {
            AuthContext::Admin { admin_id } => Ok((OWNER_KIND_ADMIN, admin_id.to_string())),
            AuthContext::Device { mac } => Ok((OWNER_KIND_DEVICE, mac)),
            AuthContext::Anonymous => {
                Err(AppError::Forbidden("authentication required".to_owned()))
            }
        }
    }

    /// Human label for a routing target, used in "routing changed to X".
    async fn target_label(&self, target: &RoutingTarget) -> String {
        match target {
            RoutingTarget::Tunnel { tunnel_id } => self.tunnel_label(&tunnel_id.to_string()).await,
            RoutingTarget::Direct => "direct (no tunnel)".to_owned(),
            RoutingTarget::Default => "default routing".to_owned(),
        }
    }

    async fn tunnel_label(&self, tunnel_id: &str) -> String {
        match self.tunnel_repo.find_by_id(tunnel_id).await {
            Ok(Some(tunnel)) => tunnel.label,
            _ => "A tunnel".to_owned(),
        }
    }

    /// Best available human name for a device: user-set name, else hostname,
    /// else MAC.
    async fn device_name(&self, device_id: &str) -> String {
        match self.device_repo.find_by_id(device_id).await {
            Ok(Some(device)) => device
                .name
                .filter(|n| !n.is_empty())
                .or_else(|| device.hostname.filter(|h| !h.is_empty()))
                .unwrap_or(device.mac),
            _ => "A device".to_owned(),
        }
    }

    async fn deliver_to_device(&self, mac: &str, notif: Notification) {
        match self.push_repo.list_by_owner(OWNER_KIND_DEVICE, mac).await {
            Ok(subs) => self.deliver(subs, &notif).await,
            Err(error) => tracing::warn!(%error, "push: failed to load device subscriptions"),
        }
    }

    async fn deliver_to_admins(&self, notif: Notification) {
        match self.push_repo.list_admins().await {
            Ok(subs) => self.deliver(subs, &notif).await,
            Err(error) => tracing::warn!(%error, "push: failed to load admin subscriptions"),
        }
    }

    /// Fan a notification out to a set of subscriptions, pruning any the push
    /// service reports as gone. Best-effort: transient failures are dropped.
    async fn deliver(&self, subs: Vec<StoredPushSubscription>, notif: &Notification) {
        if subs.is_empty() {
            return;
        }
        let vapid = match self.ensure_vapid().await {
            Ok(vapid) => vapid,
            Err(error) => {
                tracing::warn!(%error, "push: no VAPID key, dropping notification");
                return;
            }
        };
        let payload = notif.to_json_bytes();
        for sub in subs {
            let outcome = self
                .sender
                .send(
                    &vapid,
                    PushTarget {
                        endpoint: &sub.endpoint,
                        p256dh: &sub.p256dh,
                        auth: &sub.auth,
                    },
                    payload.clone(),
                )
                .await;
            if outcome == SendOutcome::Gone
                && let Err(error) = self.push_repo.delete_by_endpoint(&sub.endpoint).await
            {
                tracing::warn!(%error, "push: failed to prune stale subscription");
            }
        }
    }
}

#[async_trait]
impl PushService for PushServiceImpl {
    async fn vapid_public_key(&self) -> Result<String, AppError> {
        // Intentionally unauthenticated: the VAPID public key is meant to be
        // public (browsers embed it in every subscription). No `require_*`
        // guard — mirrors the other public read endpoints (`GET /api/info`).
        Ok(self.ensure_vapid().await?.public_key_base64url())
    }

    async fn subscribe(&self, sub: WebPushSubscription) -> Result<(), AppError> {
        auth_context::require_authenticated()?;
        let (owner_kind, owner_key) = Self::caller_owner()?;

        if sub.endpoint.is_empty() || sub.keys.p256dh.is_empty() || sub.keys.auth.is_empty() {
            return Err(AppError::BadRequest(
                "endpoint, p256dh and auth are required".to_owned(),
            ));
        }
        // Web Push endpoints are always HTTPS. Rejecting anything else keeps the
        // daemon from ever POSTing to an attacker-chosen `http://`/loopback URL
        // (defence-in-depth against the endpoint becoming an SSRF vector).
        if !sub.endpoint.starts_with("https://") {
            return Err(AppError::BadRequest(
                "endpoint must be an https URL".to_owned(),
            ));
        }

        self.push_repo
            .upsert(NewPushSubscription {
                id: &uuid::Uuid::new_v4().to_string(),
                owner_kind,
                owner_key: &owner_key,
                endpoint: &sub.endpoint,
                p256dh: &sub.keys.p256dh,
                auth: &sub.keys.auth,
                created_at: &chrono::Utc::now().to_rfc3339(),
            })
            .await
            .map_err(AppError::Internal)?;
        Ok(())
    }

    async fn unsubscribe(&self, endpoint: Option<String>) -> Result<(), AppError> {
        auth_context::require_authenticated()?;
        let (owner_kind, owner_key) = Self::caller_owner()?;

        match endpoint {
            // Owner-scoped: a caller can only remove its own subscription, even
            // if it somehow learns another owner's endpoint URL.
            Some(endpoint) => {
                self.push_repo
                    .delete_by_owner_and_endpoint(owner_kind, &owner_key, &endpoint)
                    .await
                    .map_err(AppError::Internal)?;
            }
            None => {
                self.push_repo
                    .delete_by_owner(owner_kind, &owner_key)
                    .await
                    .map_err(AppError::Internal)?;
            }
        }
        Ok(())
    }

    async fn handle_event(&self, event: &WardnetEvent) -> Result<(), AppError> {
        // Invoked by the daemon event listener under `AuthContext::Admin`.
        auth_context::require_admin()?;

        match event {
            WardnetEvent::DeviceAdminLocked {
                device_id, locked, ..
            } => {
                if let Ok(Some(device)) = self.device_repo.find_by_id(&device_id.to_string()).await
                {
                    let notif = if *locked {
                        Notification {
                            title: "Routing locked",
                            body: "An administrator has locked your routing configuration."
                                .to_owned(),
                        }
                    } else {
                        Notification {
                            title: "Routing unlocked",
                            body: "You can now change your routing configuration.".to_owned(),
                        }
                    };
                    self.deliver_to_device(&device.mac, notif).await;
                }
            }

            WardnetEvent::RoutingRuleChanged {
                device_id,
                target,
                changed_by,
                ..
            } => {
                let label = self.target_label(target).await;
                match changed_by {
                    // Admin changed a device's rule -> tell that device.
                    RuleCreator::Admin => {
                        if let Ok(Some(device)) =
                            self.device_repo.find_by_id(&device_id.to_string()).await
                        {
                            self.deliver_to_device(
                                &device.mac,
                                Notification {
                                    title: "Routing changed",
                                    body: format!("Your routing was changed to {label}."),
                                },
                            )
                            .await;
                        }
                    }
                    // A device changed its own rule -> tell the admins.
                    RuleCreator::User => {
                        let name = self.device_name(&device_id.to_string()).await;
                        self.deliver_to_admins(Notification {
                            title: "Routing change",
                            body: format!("{name} changed routing to {label}."),
                        })
                        .await;
                    }
                }
            }

            WardnetEvent::TunnelStartFailed { tunnel_id, .. } => {
                let label = self.tunnel_label(&tunnel_id.to_string()).await;
                self.deliver_to_admins(Notification {
                    title: "Tunnel offline",
                    body: format!("{label} failed to start."),
                })
                .await;
            }

            // A running tunnel became unreachable. `TunnelReconnecting` is a
            // stale-handshake signal; `TunnelDown{reason:"interface absent"}`
            // is the kernel interface vanishing. Deliberate tear-downs (every
            // other `TunnelDown` reason) are intentionally NOT notified.
            WardnetEvent::TunnelReconnecting { tunnel_id, .. } => {
                let label = self.tunnel_label(&tunnel_id.to_string()).await;
                self.deliver_to_admins(Notification {
                    title: "Tunnel offline",
                    body: format!("{label} went offline."),
                })
                .await;
            }
            WardnetEvent::TunnelDown {
                tunnel_id, reason, ..
            } if reason == "interface absent" => {
                let label = self.tunnel_label(&tunnel_id.to_string()).await;
                self.deliver_to_admins(Notification {
                    title: "Tunnel offline",
                    body: format!("{label} went offline."),
                })
                .await;
            }

            _ => {}
        }
        Ok(())
    }
}
