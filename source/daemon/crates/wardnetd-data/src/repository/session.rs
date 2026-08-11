use async_trait::async_trait;

use crate::repository::user::UserRole;

/// A session resolved for authentication, joined to its user.
///
/// Carries the **role**, not just the id. `resolve_auth_context` used to
/// promote any valid session to admin; with two roles in play that would be a
/// privilege escalation, so the role travels with the session by construction
/// (ADR-0031 §2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionPrincipal {
    /// The authenticated household user.
    pub user_id: String,
    /// Their role at the time the session was resolved — read live from
    /// `users`, never cached in the session row, so a demotion takes effect on
    /// the next request rather than at the next login.
    pub role: UserRole,
    /// Display name, for the session response.
    pub display_name: String,
}

/// Everything the refresh endpoint needs, in one read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionForRefresh {
    /// Owning user.
    pub user_id: String,
    /// Whether this was a "remember me" session, which decides the new expiry.
    pub remember_me: bool,
    /// RFC 3339 creation timestamp.
    pub created_at: String,
    /// The hard ceiling a refresh may never push past.
    pub absolute_expires_at: String,
}

/// One session as shown in the "your sessions" list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionSummary {
    /// Session UUID.
    pub id: String,
    /// Owning user.
    pub user_id: String,
    /// Device the session was issued to, when known.
    pub device_id: Option<String>,
    /// Raw `User-Agent`, so a person can recognise their own session.
    pub user_agent: Option<String>,
    /// RFC 3339 creation timestamp.
    pub created_at: String,
    /// RFC 3339 sliding expiry.
    pub expires_at: String,
}

/// Data access for household-user sessions.
///
/// Token generation and hashing live in the service layer; this trait only
/// reads and writes rows.
#[async_trait]
pub trait SessionRepository: Send + Sync {
    /// Insert a new session row.
    ///
    /// `absolute_expires_at` is the ceiling no refresh may push past. It is
    /// stored rather than re-derived from `created_at` on every refresh, so
    /// the policy that issued the session is the policy that bounds it.
    #[allow(clippy::too_many_arguments)]
    async fn create(
        &self,
        id: &str,
        user_id: &str,
        token_hash: &str,
        created_at: &str,
        expires_at: &str,
        remember_me: bool,
        device_id: Option<&str>,
        user_agent: Option<&str>,
        absolute_expires_at: &str,
    ) -> anyhow::Result<()>;

    /// Resolve a non-expired session to its principal.
    ///
    /// Returns `None` when the token is unknown, the session has expired, has
    /// passed its absolute ceiling, **or** the owning user is disabled — all
    /// four filtered in SQL, so no caller can forget one.
    async fn find_principal_by_token_hash(
        &self,
        token_hash: &str,
        now: &str,
    ) -> anyhow::Result<Option<SessionPrincipal>>;

    /// Delete all sessions whose `expires_at` is in the past. Returns the
    /// number of rows removed.
    async fn delete_expired(&self, now: &str) -> anyhow::Result<u64>;

    /// Delete the session with the given token hash (the logout path).
    /// Returns rows removed: 0 when it was already gone, 1 when it existed.
    async fn delete_by_token_hash(&self, token_hash: &str) -> anyhow::Result<u64>;

    /// Delete one session by id, scoped to its owner so a caller cannot
    /// revoke somebody else's session by guessing an id.
    async fn delete_by_id(&self, id: &str, user_id: &str) -> anyhow::Result<u64>;

    /// Delete **every** session belonging to a user. Backs "sign out
    /// everywhere", and is called whenever a user is disabled, deleted, or has
    /// their password changed — a credential change that leaves live sessions
    /// standing has not actually revoked anything.
    async fn delete_all_for_user(&self, user_id: &str) -> anyhow::Result<u64>;

    /// List a user's live sessions, newest first.
    async fn list_for_user(&self, user_id: &str, now: &str) -> anyhow::Result<Vec<SessionSummary>>;

    /// Atomically replace the token hash and extend expiry (token rotation on
    /// refresh).
    async fn rotate_token(
        &self,
        old_token_hash: &str,
        new_token_hash: &str,
        new_expires_at: &str,
    ) -> anyhow::Result<()>;

    /// Atomically look up a session for the refresh endpoint.
    ///
    /// One query rather than several, which closes the race in which
    /// `delete_expired` removes the row between two sequential reads.
    async fn find_session_for_refresh(
        &self,
        token_hash: &str,
        now: &str,
    ) -> anyhow::Result<Option<SessionForRefresh>>;
}
