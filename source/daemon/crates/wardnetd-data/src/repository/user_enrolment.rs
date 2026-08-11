use async_trait::async_trait;

/// One enrolment token as persisted in `user_enrolment_tokens`.
///
/// The token itself is **never** stored — only its hash. A readable enrolment
/// token sitting in a database backup is a standing invitation to become a
/// household member, and backups leave the box (ADR-0031 §9).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnrolmentTokenRow {
    /// Token UUID (primary key).
    pub id: String,
    /// The user this token enrols.
    pub user_id: String,
    /// Hash of the token the admin handed over. Unique.
    pub token_hash: String,
    /// RFC 3339 creation timestamp.
    pub created_at: String,
    /// RFC 3339 expiry.
    pub expires_at: String,
    /// RFC 3339 redemption timestamp, or `None` while unredeemed. Single-use
    /// is enforced by this column, not by deleting the row: keeping the spent
    /// row lets the admin UI say "redeemed on the 3rd" rather than "unknown
    /// token", and lets a replayed token be recognised rather than merely
    /// missed.
    pub used_at: Option<String>,
}

/// Data access for admin-issued one-time enrolment tokens.
#[async_trait]
pub trait UserEnrolmentRepository: Send + Sync {
    /// Insert a new token row.
    async fn create(&self, row: &EnrolmentTokenRow) -> anyhow::Result<()>;

    /// Find a **redeemable** token by its hash: one that exists, has not
    /// expired at `now`, and has not been used.
    ///
    /// All three conditions are applied in SQL so no caller can redeem a spent
    /// or expired token by forgetting a check.
    async fn find_redeemable(
        &self,
        token_hash: &str,
        now: &str,
    ) -> anyhow::Result<Option<EnrolmentTokenRow>>;

    /// Mark a token redeemed, but only if it is still redeemable.
    ///
    /// Returns the number of rows affected: `1` on success, `0` when another
    /// request redeemed it first. The `used_at IS NULL` predicate lives in the
    /// `UPDATE`'s `WHERE` clause precisely so that two concurrent redemptions
    /// cannot both succeed — a check-then-write in the service would race.
    async fn mark_used(&self, id: &str, now: &str) -> anyhow::Result<u64>;

    /// Every token issued for a user, newest first, so the admin UI can show
    /// outstanding invitations.
    async fn list_for_user(&self, user_id: &str) -> anyhow::Result<Vec<EnrolmentTokenRow>>;

    /// Revoke an outstanding token before it is redeemed.
    async fn delete(&self, id: &str) -> anyhow::Result<u64>;

    /// Delete every token that expired before `now`. Returns rows removed.
    /// Called by `session_cleanup_runner` alongside expired sessions.
    async fn delete_expired(&self, now: &str) -> anyhow::Result<u64>;
}
