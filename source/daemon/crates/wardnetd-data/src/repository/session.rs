use async_trait::async_trait;

/// Data access for admin sessions.
///
/// Handles creation, lookup, and expiry of session rows. The actual token
/// generation and hashing logic lives in [`AuthService`](crate::service::AuthService).
#[async_trait]
pub trait SessionRepository: Send + Sync {
    /// Insert a new session row.
    async fn create(
        &self,
        id: &str,
        admin_id: &str,
        token_hash: &str,
        created_at: &str,
        expires_at: &str,
    ) -> anyhow::Result<()>;

    /// Find the `admin_id` for a session whose token hash matches and has not expired.
    async fn find_admin_id_by_token_hash(
        &self,
        token_hash: &str,
        now: &str,
    ) -> anyhow::Result<Option<String>>;

    /// Delete all sessions whose `expires_at` is in the past. Returns the number of rows removed.
    async fn delete_expired(&self, now: &str) -> anyhow::Result<u64>;
}
