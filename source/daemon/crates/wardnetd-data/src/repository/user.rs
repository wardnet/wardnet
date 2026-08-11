use async_trait::async_trait;

/// Marker error returned by [`UserRepository::create`] and
/// [`UserRepository::update_profile`] when the write fails specifically
/// because the case-insensitive email index (`idx_users_email_lower`) is
/// violated — i.e. another household user already has that email address.
///
/// This is a business-rule conflict, not a generic DB failure, so the service
/// layer downcasts on it and maps it to a `409 Conflict` rather than a raw
/// `500`. A uniqueness violation on `id` is deliberately NOT wrapped: ids are
/// minted by the daemon from a v4 UUID, and a collision indicates a genuine
/// internal fault.
#[derive(Debug)]
pub struct DuplicateUserEmailError;

impl std::fmt::Display for DuplicateUserEmailError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("a household user with that email already exists")
    }
}

impl std::error::Error for DuplicateUserEmailError {}

/// A household user's role — re-exported from `wardnet-common` rather than
/// redefined here.
///
/// One type on purpose. A second, structurally identical enum in the data layer
/// would need a conversion at the service boundary, and a role conversion is
/// precisely the kind of code where a mis-typed arm silently becomes a
/// privilege escalation. `as_str` is the `role` column value and must match the
/// migration's `CHECK` constraint; `parse` returns `None` for anything
/// unrecognised rather than defaulting to `Admin`.
pub use wardnet_common::auth::UserRole;

/// One household user as persisted in `users`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserRow {
    /// User UUID (primary key).
    pub id: String,
    /// Human name shown in the directory. For a backfilled local admin this
    /// is the old `admins.username`.
    pub display_name: String,
    /// Optional email. Unique case-insensitively when present; several users
    /// may legitimately have none.
    pub email: Option<String>,
    /// `admin` or `member`.
    pub role: UserRole,
    /// A disabled user keeps their rows but cannot authenticate: the login
    /// join filters on this, so disabling is immediate and does not depend on
    /// hunting down their sessions.
    pub enabled: bool,
    /// RFC 3339 creation timestamp.
    pub created_at: String,
    /// RFC 3339 last-modification timestamp.
    pub updated_at: String,
}

/// Data access for the household user directory (`users`).
///
/// No business logic — password hashing, the "last admin" rule, and every
/// authorization check belong in the service layer.
#[async_trait]
pub trait UserRepository: Send + Sync {
    /// Insert a new user row.
    ///
    /// An email uniqueness violation MUST be returned as an error that
    /// downcasts to [`DuplicateUserEmailError`], so the caller can surface a
    /// clean `409 Conflict` instead of a `500`.
    async fn create(&self, row: &UserRow) -> anyhow::Result<()>;

    /// Look up one user by id.
    async fn find_by_id(&self, id: &str) -> anyhow::Result<Option<UserRow>>;

    /// Look up one user by email, case-insensitively.
    async fn find_by_email(&self, email: &str) -> anyhow::Result<Option<UserRow>>;

    /// Every user, for the admin directory, ordered by `display_name`.
    async fn find_all(&self) -> anyhow::Result<Vec<UserRow>>;

    /// Update the mutable profile fields. Also bumps `updated_at`.
    ///
    /// Same [`DuplicateUserEmailError`] contract as [`create`](Self::create).
    /// Returns the number of rows affected (0 when the user is gone).
    async fn update_profile(
        &self,
        id: &str,
        display_name: &str,
        email: Option<&str>,
        now: &str,
    ) -> anyhow::Result<u64>;

    /// Enable or disable a user. Also bumps `updated_at`.
    async fn set_enabled(&self, id: &str, enabled: bool, now: &str) -> anyhow::Result<u64>;

    /// Change a user's role. Also bumps `updated_at`.
    async fn set_role(&self, id: &str, role: UserRole, now: &str) -> anyhow::Result<u64>;

    /// Delete a user. Credentials, enrolment tokens and sessions cascade;
    /// `devices.owner_user_id` is set to NULL (deleting a person must not
    /// delete the household's hardware).
    async fn delete(&self, id: &str) -> anyhow::Result<u64>;

    /// How many **enabled** users hold `role = 'admin'`.
    ///
    /// Backs the "the last admin cannot be demoted, disabled or deleted" rule.
    /// Counting only enabled admins is deliberate: a household whose sole
    /// admin is disabled is locked out just as thoroughly as one whose sole
    /// admin was deleted.
    async fn count_enabled_admins(&self) -> anyhow::Result<i64>;

    /// Return `true` if at least one user exists. Backs the setup wizard's
    /// "has this box been claimed?" check.
    async fn exists(&self) -> anyhow::Result<bool>;

    /// The oldest **enabled** `admin`-role user.
    ///
    /// Backs API-key validation: a key is a box-level credential with no person
    /// attached, so it has to act as *some* admin. Resolving to the oldest
    /// enabled admin — the backfilled local admin on any upgraded box, since
    /// the migration preserves its id and `created_at` — keeps that mapping
    /// stable across directory changes, which matters because it is the
    /// identity written into audit logs.
    ///
    /// Filtered on `enabled` so that disabling every admin also stops API
    /// keys, rather than leaving a credential that outlives the account it
    /// borrows authority from. Returns `None` when no enabled admin exists;
    /// callers must reject the key rather than fall back to any other user.
    async fn find_first_enabled_admin(&self) -> anyhow::Result<Option<UserRow>>;
}
