use async_trait::async_trait;
use sqlx::SqlitePool;

use crate::db::DbPools;
use crate::repository::notification::{
    NOTIFICATION_RETENTION_CAP, NewNotification, NotificationRepository, StoredNotification,
};

#[derive(sqlx::FromRow)]
struct DbNotificationRow {
    id: String,
    kind: String,
    title: String,
    body: String,
    url: Option<String>,
    subject_id: Option<String>,
    created_at: String,
}

impl DbNotificationRow {
    fn into_domain(self) -> StoredNotification {
        StoredNotification {
            id: self.id,
            kind: self.kind,
            title: self.title,
            body: self.body,
            url: self.url,
            subject_id: self.subject_id,
            created_at: self.created_at,
        }
    }
}

pub struct SqliteNotificationRepository {
    pools: DbPools,
}

impl SqliteNotificationRepository {
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self::new_pools(DbPools::single(pool))
    }

    #[must_use]
    pub fn new_pools(pools: DbPools) -> Self {
        Self { pools }
    }
}

#[async_trait]
impl NotificationRepository for SqliteNotificationRepository {
    async fn insert(&self, notification: NewNotification<'_>) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO notifications \
             (id, kind, title, body, url, subject_id, created_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(notification.id)
        .bind(notification.kind)
        .bind(notification.title)
        .bind(notification.body)
        .bind(notification.url)
        .bind(notification.subject_id)
        .bind(notification.created_at)
        .execute(&self.pools.write)
        .await?;
        // Retention: keep only the newest rows. The `id` tiebreak makes the
        // cut deterministic when timestamps collide.
        sqlx::query(
            "DELETE FROM notifications WHERE id NOT IN \
             (SELECT id FROM notifications ORDER BY created_at DESC, id DESC LIMIT ?)",
        )
        .bind(NOTIFICATION_RETENTION_CAP)
        .execute(&self.pools.write)
        .await?;
        Ok(())
    }

    async fn list_recent(&self, limit: u32) -> anyhow::Result<Vec<StoredNotification>> {
        let rows = sqlx::query_as::<_, DbNotificationRow>(
            "SELECT id, kind, title, body, url, subject_id, created_at \
             FROM notifications ORDER BY created_at DESC, id DESC LIMIT ?",
        )
        .bind(limit)
        .fetch_all(&self.pools.read)
        .await?;
        Ok(rows
            .into_iter()
            .map(DbNotificationRow::into_domain)
            .collect())
    }

    async fn clear(&self) -> anyhow::Result<u64> {
        let affected = sqlx::query("DELETE FROM notifications")
            .execute(&self.pools.write)
            .await?
            .rows_affected();
        Ok(affected)
    }
}
