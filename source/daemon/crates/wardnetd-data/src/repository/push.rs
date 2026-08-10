use async_trait::async_trait;

/// The owner discriminator kinds for [`StoredPushSubscription`]. Kept as
/// `&str` constants (rather than an enum) because the value crosses the
/// `SQLite` text column verbatim and the service layer maps auth contexts onto
/// them.
pub const OWNER_KIND_DEVICE: &str = "device";
/// A household user, keyed by `users.id`.
///
/// Replaced the former `"admin"` value in the household-identity migration,
/// which rewrote existing rows in place. The ids were preserved by the
/// `admins` → `users` backfill, so upgraded boxes keep their subscriptions.
pub const OWNER_KIND_USER: &str = "user";

/// A Web Push subscription as persisted in `push_subscriptions`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredPushSubscription {
    pub id: String,
    /// [`OWNER_KIND_DEVICE`] or [`OWNER_KIND_USER`].
    pub owner_kind: String,
    /// Device MAC (device kind) or `users.id` (user kind).
    pub owner_key: String,
    pub endpoint: String,
    pub p256dh: String,
    pub auth: String,
    pub created_at: String,
}

/// Fields needed to persist a new subscription. Grouped in a struct so
/// [`PushRepository::upsert`] stays under clippy's argument-count limit.
#[derive(Debug, Clone)]
pub struct NewPushSubscription<'a> {
    pub id: &'a str,
    pub owner_kind: &'a str,
    pub owner_key: &'a str,
    pub endpoint: &'a str,
    pub p256dh: &'a str,
    pub auth: &'a str,
    pub created_at: &'a str,
}

#[async_trait]
pub trait PushRepository: Send + Sync {
    /// Insert a subscription, or update the owner + keys of an existing row
    /// with the same `endpoint` (a browser re-subscribing). Idempotent per
    /// endpoint.
    async fn upsert(&self, sub: NewPushSubscription<'_>) -> anyhow::Result<()>;

    /// All subscriptions for one owner (kind + key).
    async fn list_by_owner(
        &self,
        owner_kind: &str,
        owner_key: &str,
    ) -> anyhow::Result<Vec<StoredPushSubscription>>;

    /// Subscriptions owned by **enabled `admin`-role household users** — the
    /// fan-out target for admin-PWA notifications.
    ///
    /// Determined by joining `users` and reading `role`, not by the
    /// `owner_kind` discriminator alone: a `member` is a legitimate
    /// subscription owner ([`OWNER_KIND_USER`]) who must not receive
    /// administrative notifications.
    async fn list_admins(&self) -> anyhow::Result<Vec<StoredPushSubscription>>;

    /// Remove every subscription for one owner. Returns the number removed.
    async fn delete_by_owner(&self, owner_kind: &str, owner_key: &str) -> anyhow::Result<u64>;

    /// Remove a single subscription owned by `(owner_kind, owner_key)` with the
    /// given endpoint. Owner-scoped so a caller can only delete its own
    /// subscription — used for caller-requested unsubscribe. Returns the number
    /// removed (0 or 1).
    async fn delete_by_owner_and_endpoint(
        &self,
        owner_kind: &str,
        owner_key: &str,
        endpoint: &str,
    ) -> anyhow::Result<u64>;

    /// Remove a subscription by endpoint regardless of owner. **Internal use
    /// only** — pruning after a 404/410 from the push service. Not reachable
    /// from the caller-facing API (that path is owner-scoped via
    /// [`Self::delete_by_owner_and_endpoint`]). Returns the number removed.
    async fn delete_by_endpoint(&self, endpoint: &str) -> anyhow::Result<u64>;
}
