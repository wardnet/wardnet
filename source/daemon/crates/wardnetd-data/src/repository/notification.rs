use async_trait::async_trait;

/// How many notifications the feed retains. Enforced inside
/// [`NotificationRepository::insert`] (delete-oldest-beyond-cap), so no
/// background cleanup job is needed — admin-notification volume is a handful
/// per day.
pub const NOTIFICATION_RETENTION_CAP: u32 = 100;

/// An admin-feed notification as persisted in `notifications`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredNotification {
    pub id: String,
    /// Stable machine tag, e.g. `new_device_quarantined`, `tunnel_offline`.
    pub kind: String,
    pub title: String,
    pub body: String,
    /// App-relative deep link (no PWA base path), e.g. `/devices`.
    pub url: Option<String>,
    /// Kind-driven subject entity id (device UUID, tunnel UUID, ...).
    pub subject_id: Option<String>,
    pub created_at: String,
}

/// Fields needed to persist a new notification.
#[derive(Debug, Clone)]
pub struct NewNotification<'a> {
    pub id: &'a str,
    pub kind: &'a str,
    pub title: &'a str,
    pub body: &'a str,
    pub url: Option<&'a str>,
    pub subject_id: Option<&'a str>,
    pub created_at: &'a str,
}

/// Persistence for the admin notification feed (issue #482): the recent,
/// count-capped record of admin-audience push notifications shown on the
/// admin-PWA System screen.
#[async_trait]
pub trait NotificationRepository: Send + Sync {
    /// Insert a notification, then prune the oldest rows beyond
    /// [`NOTIFICATION_RETENTION_CAP`] in the same call.
    async fn insert(&self, notification: NewNotification<'_>) -> anyhow::Result<()>;

    /// The most recent notifications, newest first, at most `limit`.
    async fn list_recent(&self, limit: u32) -> anyhow::Result<Vec<StoredNotification>>;

    /// Remove every notification (the feed's Clear action). Returns the number
    /// removed.
    async fn clear(&self) -> anyhow::Result<u64>;
}
