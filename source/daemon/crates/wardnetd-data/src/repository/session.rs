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
        remember_me: bool,
    ) -> anyhow::Result<()>;

    /// Find the `admin_id` for a session whose token hash matches and has not expired.
    async fn find_admin_id_by_token_hash(
        &self,
        token_hash: &str,
        now: &str,
    ) -> anyhow::Result<Option<String>>;

    /// Delete all sessions whose `expires_at` is in the past. Returns the number of rows removed.
    async fn delete_expired(&self, now: &str) -> anyhow::Result<u64>;

    /// Delete the session with the given token hash (used by the logout
    /// endpoint). Returns the number of rows removed (0 when the session was
    /// already gone, 1 when it existed).
    async fn delete_by_token_hash(&self, token_hash: &str) -> anyhow::Result<u64>;

    /// Slide the expiry forward for an existing session (used by the refresh endpoint).
    async fn extend_expiry(&self, token_hash: &str, new_expires_at: &str) -> anyhow::Result<()>;

    /// Atomically replace the token hash and extend expiry (token rotation on refresh).
    async fn rotate_token(
        &self,
        old_token_hash: &str,
        new_token_hash: &str,
        new_expires_at: &str,
    ) -> anyhow::Result<()>;

    /// Atomically look up a session for the refresh endpoint.
    ///
    /// Returns `Some((admin_id, remember_me, created_at))` when the session exists and has
    /// not expired; `None` otherwise. Using a single query eliminates the
    /// race window between the two-call pattern
    /// (`find_admin_id_by_token_hash` + separate `remember_me` lookup) where
    /// `delete_expired` could remove the row between the two reads.
    async fn find_session_for_refresh(
        &self,
        token_hash: &str,
        now: &str,
    ) -> anyhow::Result<Option<(String, bool, String)>>;
}
