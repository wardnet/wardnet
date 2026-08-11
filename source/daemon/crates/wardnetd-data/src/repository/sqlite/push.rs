use async_trait::async_trait;
use sqlx::SqlitePool;

use crate::db::DbPools;
use crate::repository::push::{NewPushSubscription, PushRepository, StoredPushSubscription};

#[derive(sqlx::FromRow)]
struct DbPushSubscriptionRow {
    id: String,
    owner_kind: String,
    owner_key: String,
    endpoint: String,
    p256dh: String,
    auth: String,
    created_at: String,
}

impl DbPushSubscriptionRow {
    fn into_domain(self) -> StoredPushSubscription {
        StoredPushSubscription {
            id: self.id,
            owner_kind: self.owner_kind,
            owner_key: self.owner_key,
            endpoint: self.endpoint,
            p256dh: self.p256dh,
            auth: self.auth,
            created_at: self.created_at,
        }
    }
}

pub struct SqlitePushRepository {
    pools: DbPools,
}

impl SqlitePushRepository {
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
impl PushRepository for SqlitePushRepository {
    async fn upsert(&self, sub: NewPushSubscription<'_>) -> anyhow::Result<()> {
        // Conflict on the UNIQUE endpoint updates ownership + keys, so a
        // browser re-subscribing (same endpoint) never duplicates and can move
        // between owners (e.g. anonymous device -> admin after login).
        sqlx::query(
            "INSERT INTO push_subscriptions \
             (id, owner_kind, owner_key, endpoint, p256dh, auth, created_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?) \
             ON CONFLICT(endpoint) DO UPDATE SET \
             owner_kind = excluded.owner_kind, \
             owner_key = excluded.owner_key, \
             p256dh = excluded.p256dh, \
             auth = excluded.auth",
        )
        .bind(sub.id)
        .bind(sub.owner_kind)
        .bind(sub.owner_key)
        .bind(sub.endpoint)
        .bind(sub.p256dh)
        .bind(sub.auth)
        .bind(sub.created_at)
        .execute(&self.pools.write)
        .await?;
        Ok(())
    }

    async fn list_by_owner(
        &self,
        owner_kind: &str,
        owner_key: &str,
    ) -> anyhow::Result<Vec<StoredPushSubscription>> {
        let rows = sqlx::query_as::<_, DbPushSubscriptionRow>(
            "SELECT id, owner_kind, owner_key, endpoint, p256dh, auth, created_at \
             FROM push_subscriptions \
             WHERE owner_kind = ? AND owner_key = ? ORDER BY created_at ASC",
        )
        .bind(owner_kind)
        .bind(owner_key)
        .fetch_all(&self.pools.read)
        .await?;
        Ok(rows
            .into_iter()
            .map(DbPushSubscriptionRow::into_domain)
            .collect())
    }

    async fn list_admins(&self) -> anyhow::Result<Vec<StoredPushSubscription>> {
        // Joined to `users` and filtered on `role`, not merely on the owner
        // discriminator. Before household identity every user-owned
        // subscription belonged to the single admin, so `owner_kind = 'admin'`
        // was the whole test; now a `member` owns subscriptions too, and
        // fanning admin notifications out to them would leak the household's
        // administrative activity to whoever holds the fewest privileges.
        //
        // `enabled = 1` for the same reason the login join filters it: a
        // disabled account must stop receiving as well as stop authenticating.
        let rows = sqlx::query_as::<_, DbPushSubscriptionRow>(
            "SELECT p.id, p.owner_kind, p.owner_key, p.endpoint, p.p256dh, p.auth, p.created_at \
             FROM push_subscriptions p \
             JOIN users u ON u.id = p.owner_key \
             WHERE p.owner_kind = 'user' AND u.role = 'admin' AND u.enabled = 1 \
             ORDER BY p.created_at ASC",
        )
        .fetch_all(&self.pools.read)
        .await?;
        Ok(rows
            .into_iter()
            .map(DbPushSubscriptionRow::into_domain)
            .collect())
    }

    async fn delete_by_owner(&self, owner_kind: &str, owner_key: &str) -> anyhow::Result<u64> {
        let affected =
            sqlx::query("DELETE FROM push_subscriptions WHERE owner_kind = ? AND owner_key = ?")
                .bind(owner_kind)
                .bind(owner_key)
                .execute(&self.pools.write)
                .await?
                .rows_affected();
        Ok(affected)
    }

    async fn delete_by_owner_and_endpoint(
        &self,
        owner_kind: &str,
        owner_key: &str,
        endpoint: &str,
    ) -> anyhow::Result<u64> {
        let affected = sqlx::query(
            "DELETE FROM push_subscriptions \
             WHERE owner_kind = ? AND owner_key = ? AND endpoint = ?",
        )
        .bind(owner_kind)
        .bind(owner_key)
        .bind(endpoint)
        .execute(&self.pools.write)
        .await?
        .rows_affected();
        Ok(affected)
    }

    async fn delete_by_endpoint(&self, endpoint: &str) -> anyhow::Result<u64> {
        let affected = sqlx::query("DELETE FROM push_subscriptions WHERE endpoint = ?")
            .bind(endpoint)
            .execute(&self.pools.write)
            .await?
            .rows_affected();
        Ok(affected)
    }
}
