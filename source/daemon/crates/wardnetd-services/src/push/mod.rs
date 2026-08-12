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

pub mod listener;
pub mod sender;

#[cfg(test)]
mod tests;

use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::OnceCell;
use wardnet_common::anomaly::{Anomaly, AnomalyType};
use wardnet_common::api::WebPushSubscription;
use wardnet_common::auth::AuthContext;
use wardnet_common::event::WardnetEvent;
use wardnet_common::routing::{RoutingTarget, RuleCreator};
use wardnet_common::rule_request::RuleRequestKind;
use wardnetd_data::repository::push::{OWNER_KIND_DEVICE, OWNER_KIND_USER};
use wardnetd_data::repository::{
    DeviceRepository, NewNotification, NewPushSubscription, NotificationRepository, PushRepository,
    StoredNotification, StoredPushSubscription, SystemConfigRepository, TunnelRepository,
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

/// The stable machine tags carried in [`NotificationData::kind`]. An enum —
/// not bare literals at the call sites — so the wire payload, the feed rows,
/// and the frontend consumers cannot drift apart via a typo'd string.
/// The serialized names are consumed by the PWAs (notification tag, feed
/// pill); treat them as a wire contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NotificationKind {
    /// An anomaly opened or resolved. Carries the type so the wire `kind` is
    /// the anomaly's own slug (`blocklist_refresh_failing`, ...) rather than a
    /// generic "anomaly" that clients would have to unpack the body to read.
    Anomaly(AnomalyType),
    RoutingLocked,
    RoutingUnlocked,
    RoutingChanged,
    TunnelOffline,
    NewDeviceQuarantined,
    RuleRequestCreated,
    PrivateDnsGranted,
}

impl NotificationKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Anomaly(anomaly_type) => anomaly_type.as_str(),
            Self::RoutingLocked => "routing_locked",
            Self::RoutingUnlocked => "routing_unlocked",
            Self::RoutingChanged => "routing_changed",
            Self::TunnelOffline => "tunnel_offline",
            Self::NewDeviceQuarantined => "new_device_quarantined",
            Self::RuleRequestCreated => "rule_request_created",
            Self::PrivateDnsGranted => "private_dns_granted",
        }
    }
}

/// Structured, machine-readable companion to the human title/body. The PWA
/// service worker collapses notifications by `kind` + `subject_id`,
/// deep-links via `url`, and identifies the subject entity via `subject_id`.
#[derive(Debug, Clone, PartialEq, Eq)]
struct NotificationData {
    /// Stable machine tag, e.g. `new_device_quarantined`, `tunnel_offline`.
    kind: NotificationKind,
    /// App-relative deep link (no PWA base path, e.g. `/devices`); the service
    /// worker resolves it against its own registration scope.
    url: Option<&'static str>,
    /// Identifier of the subject entity; what it identifies is driven by
    /// `kind` (device UUID for device kinds, tunnel UUID for tunnel kinds).
    subject_id: Option<String>,
    /// For anomaly kinds, which edge this is: `"opened"` or `"resolved"`.
    /// Both edges share one `kind` so the service worker collapses them onto
    /// the same notification, replacing "X is broken" with "X recovered"
    /// instead of stacking a second entry.
    state: Option<&'static str>,
}

/// A rendered notification: the title + body shown by the service worker,
/// plus the structured [`NotificationData`] it acts on.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Notification {
    title: &'static str,
    body: String,
    data: NotificationData,
}

impl Notification {
    fn to_json_bytes(&self) -> Vec<u8> {
        // Minimal, stable JSON the PWA service worker reads. Kept hand-rolled
        // (rather than serde) so the exact wire shape is obvious in review.
        // `url`/`subject_id` are omitted (not null) when absent.
        let mut data = serde_json::Map::new();
        data.insert("kind".to_owned(), self.data.kind.as_str().into());
        if let Some(url) = self.data.url {
            data.insert("url".to_owned(), url.into());
        }
        if let Some(subject_id) = &self.data.subject_id {
            data.insert("subject_id".to_owned(), subject_id.as_str().into());
        }
        if let Some(state) = self.data.state {
            data.insert("state".to_owned(), state.into());
        }
        serde_json::json!({ "title": self.title, "body": self.body, "data": data })
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

    /// The most recent admin-feed notifications, newest first. `limit` is
    /// clamped to 1..=100. Admin only.
    async fn recent_notifications(&self, limit: u32) -> Result<Vec<StoredNotification>, AppError>;

    /// Remove every notification from the admin feed (the Clear action).
    /// Admin only. The feed is shared across admin accounts, so this clears
    /// it for everyone.
    async fn clear_notifications(&self) -> Result<(), AppError>;

    /// Send the device-keyed "Private DNS is ready" nudge to a granted device,
    /// deep-linking the user PWA to `/settings#private-dns`, where the Private
    /// DNS card carries the per-platform setup steps (#916). Resolves the
    /// device UUID to
    /// its MAC (the subscription owner key) internally. Returns whether any
    /// subscription for the device was targeted, so the admin API can report
    /// `delivered`. Admin only. A default `Ok(false)` keeps test doubles that
    /// predate it compiling.
    async fn notify_private_dns_granted(&self, device_id: uuid::Uuid) -> Result<bool, AppError> {
        let _ = device_id;
        Ok(false)
    }

    /// Tell the admins an anomaly just opened. Called by the anomaly service
    /// on the open edge only — never on a repeat observation, which is what
    /// keeps a long-running problem to a single alert.
    async fn notify_anomaly_opened(&self, anomaly: &Anomaly) -> Result<(), AppError> {
        let _ = anomaly;
        Ok(())
    }

    /// Tell the admins a previously-alerted anomaly resolved. Gated on the
    /// open having been notified, so "it is working again" can never arrive
    /// without its "it is broken".
    async fn notify_anomaly_resolved(&self, anomaly: &Anomaly) -> Result<(), AppError> {
        let _ = anomaly;
        Ok(())
    }
}

pub struct PushServiceImpl {
    push_repo: Arc<dyn PushRepository>,
    notification_repo: Arc<dyn NotificationRepository>,
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
        notification_repo: Arc<dyn NotificationRepository>,
        device_repo: Arc<dyn DeviceRepository>,
        tunnel_repo: Arc<dyn TunnelRepository>,
        system_config: Arc<dyn SystemConfigRepository>,
        secrets: Arc<dyn SecretStore>,
        sender: Arc<dyn WebPushSender>,
    ) -> Self {
        Self {
            push_repo,
            notification_repo,
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
            AuthContext::User(user) => Ok((OWNER_KIND_USER, user.user_id().to_string())),
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

    /// Like [`Self::deliver_to_device`] but surfaces whether the device had any
    /// subscription to target — the admin resend endpoint reports that as
    /// `delivered`. A load error propagates (the admin caller wants to know),
    /// unlike the best-effort event-driven path.
    async fn deliver_to_device_reporting(
        &self,
        mac: &str,
        notif: Notification,
    ) -> Result<bool, AppError> {
        let subs = self
            .push_repo
            .list_by_owner(OWNER_KIND_DEVICE, mac)
            .await
            .map_err(AppError::Internal)?;
        let delivered = !subs.is_empty();
        self.deliver(subs, &notif).await;
        Ok(delivered)
    }

    async fn deliver_to_admins(&self, notif: Notification) {
        // The admin feed records "what happened", not "what was delivered":
        // persist before fan-out, regardless of subscriptions or send outcomes.
        // Best-effort — a failed insert must not block delivery.
        if let Err(error) = self
            .notification_repo
            .insert(NewNotification {
                id: &uuid::Uuid::new_v4().to_string(),
                kind: notif.data.kind.as_str(),
                title: notif.title,
                body: &notif.body,
                url: notif.data.url,
                subject_id: notif.data.subject_id.as_deref(),
                created_at: &chrono::Utc::now().to_rfc3339(),
            })
            .await
        {
            tracing::warn!(%error, "push: failed to persist notification to the admin feed");
        }
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

    // A flat event -> notification mapping table; splitting it would only
    // scatter the per-event copy.
    #[allow(clippy::too_many_lines)]
    async fn handle_event(&self, event: &WardnetEvent) -> Result<(), AppError> {
        // Invoked by the daemon event listener under `AuthContext::system()`.
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
                            data: NotificationData {
                                kind: NotificationKind::RoutingLocked,
                                url: None,
                                subject_id: Some(device_id.to_string()),
                                state: None,
                            },
                        }
                    } else {
                        Notification {
                            title: "Routing unlocked",
                            body: "You can now change your routing configuration.".to_owned(),
                            data: NotificationData {
                                kind: NotificationKind::RoutingUnlocked,
                                url: None,
                                subject_id: Some(device_id.to_string()),
                                state: None,
                            },
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
                                    data: NotificationData {
                                        kind: NotificationKind::RoutingChanged,
                                        url: None,
                                        subject_id: Some(device_id.to_string()),
                                        state: None,
                                    },
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
                            data: NotificationData {
                                kind: NotificationKind::RoutingChanged,
                                url: Some("/devices"),
                                subject_id: Some(device_id.to_string()),
                                state: None,
                            },
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
                    data: NotificationData {
                        kind: NotificationKind::TunnelOffline,
                        url: Some("/tunnels"),
                        subject_id: Some(tunnel_id.to_string()),
                        state: None,
                    },
                })
                .await;
            }

            // A running tunnel became unreachable. `TunnelReconnecting` is a
            // stale-handshake signal; `TunnelDown` with the
            // `TUNNEL_DOWN_INTERFACE_ABSENT` reason is the kernel interface
            // vanishing. Deliberate tear-downs (every other `TunnelDown`
            // reason) are intentionally NOT notified.
            WardnetEvent::TunnelReconnecting { tunnel_id, .. } => {
                let label = self.tunnel_label(&tunnel_id.to_string()).await;
                self.deliver_to_admins(Notification {
                    title: "Tunnel offline",
                    body: format!("{label} went offline."),
                    data: NotificationData {
                        kind: NotificationKind::TunnelOffline,
                        url: Some("/tunnels"),
                        subject_id: Some(tunnel_id.to_string()),
                        state: None,
                    },
                })
                .await;
            }
            WardnetEvent::TunnelDown {
                tunnel_id, reason, ..
            } if reason == wardnet_common::event::TUNNEL_DOWN_INTERFACE_ABSENT => {
                let label = self.tunnel_label(&tunnel_id.to_string()).await;
                self.deliver_to_admins(Notification {
                    title: "Tunnel offline",
                    body: format!("{label} went offline."),
                    data: NotificationData {
                        kind: NotificationKind::TunnelOffline,
                        url: Some("/tunnels"),
                        subject_id: Some(tunnel_id.to_string()),
                        state: None,
                    },
                })
                .await;
            }

            // A previously-unseen device landed in the quarantine (default-for-new)
            // zone while new-device quarantine is on (#738). Nudge the admins to
            // approve it; approving = reassigning its zone via
            // `PUT /api/devices/{id}/zone`.
            WardnetEvent::NewDeviceQuarantined {
                device_id,
                zone_name,
                ..
            } => {
                let name = self.device_name(&device_id.to_string()).await;
                self.deliver_to_admins(Notification {
                    title: "New device",
                    body: format!("New device {name} joined, in {zone_name}. Approve in the app."),
                    data: NotificationData {
                        kind: NotificationKind::NewDeviceQuarantined,
                        url: Some("/devices"),
                        subject_id: Some(device_id.to_string()),
                        state: None,
                    },
                })
                .await;
            }

            // A device asked the admin to allow/block a domain (the rule-request
            // inbox). Decisions live on the desktop admin site — the admin PWA
            // has no rule-request surface yet — so the notification carries no
            // deep link and a tap opens the app root.
            WardnetEvent::RuleRequestCreated {
                request_id,
                device_id,
                kind,
                domain,
                ..
            } => {
                let name = self.device_name(device_id).await;
                let verb = match kind {
                    RuleRequestKind::Allow => "allow",
                    RuleRequestKind::Block => "block",
                };
                self.deliver_to_admins(Notification {
                    title: "Rule request",
                    body: format!("{name} asked to {verb} {domain}."),
                    data: NotificationData {
                        kind: NotificationKind::RuleRequestCreated,
                        url: None,
                        subject_id: Some(request_id.clone()),
                        state: None,
                    },
                })
                .await;
            }

            _ => {}
        }
        Ok(())
    }

    async fn recent_notifications(&self, limit: u32) -> Result<Vec<StoredNotification>, AppError> {
        auth_context::require_admin()?;
        let limit = limit.clamp(1, 100);
        self.notification_repo
            .list_recent(limit)
            .await
            .map_err(AppError::Internal)
    }

    async fn clear_notifications(&self) -> Result<(), AppError> {
        auth_context::require_admin()?;
        self.notification_repo
            .clear()
            .await
            .map_err(AppError::Internal)?;
        Ok(())
    }

    async fn notify_private_dns_granted(&self, device_id: uuid::Uuid) -> Result<bool, AppError> {
        auth_context::require_admin()?;

        // Subscriptions are keyed by device MAC (`owner_key`), but the grant —
        // and this endpoint — speak device UUID, so resolve UUID -> MAC first.
        // An unknown device simply has no subscription: report `false`, not an
        // error (the grant check upstream already guards the real 404).
        let Some(device) = self
            .device_repo
            .find_by_id(&device_id.to_string())
            .await
            .map_err(AppError::Internal)?
        else {
            return Ok(false);
        };

        let notif = Notification {
            title: "Private DNS is ready",
            body: "Tap to set up encrypted DNS on this device.".to_owned(),
            data: NotificationData {
                kind: NotificationKind::PrivateDnsGranted,
                url: Some("/settings#private-dns"),
                subject_id: Some(device_id.to_string()),
                state: None,
            },
        };
        self.deliver_to_device_reporting(&device.mac, notif).await
    }

    async fn notify_anomaly_opened(&self, anomaly: &Anomaly) -> Result<(), AppError> {
        auth_context::require_admin()?;
        self.deliver_to_admins(Notification {
            title: "Wardnet found a problem",
            // The anomaly's own message already names the subject and the
            // specifics; the hint belongs on the dashboard, not in a push.
            body: anomaly.message.clone(),
            data: NotificationData {
                kind: NotificationKind::Anomaly(anomaly.anomaly_type),
                url: Some(anomaly.anomaly_type.url()),
                subject_id: Some(anomaly.id.to_string()),
                state: Some("opened"),
            },
        })
        .await;
        Ok(())
    }

    async fn notify_anomaly_resolved(&self, anomaly: &Anomaly) -> Result<(), AppError> {
        auth_context::require_admin()?;
        self.deliver_to_admins(Notification {
            title: "Problem resolved",
            body: anomaly.message.clone(),
            data: NotificationData {
                kind: NotificationKind::Anomaly(anomaly.anomaly_type),
                url: Some(anomaly.anomaly_type.url()),
                subject_id: Some(anomaly.id.to_string()),
                state: Some("resolved"),
            },
        })
        .await;
        Ok(())
    }
}
