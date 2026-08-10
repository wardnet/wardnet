use async_trait::async_trait;

use crate::repository::user::UserRole;

/// Marker error returned by [`UserCredentialRepository::insert`] when the
/// insert fails specifically because the `(kind, subject)` uniqueness
/// constraint is violated — i.e. that provider account (or passkey credential
/// id, or username) is already linked to *some* user.
///
/// This constraint is the anti-hijack invariant (ADR-0031 §1), so the service
/// layer must be able to tell it apart from a generic failure and refuse the
/// link with a `409 Conflict`. It deliberately does **not** disclose which
/// user holds the existing link: that would turn an authenticated link attempt
/// into a directory-enumeration oracle.
#[derive(Debug)]
pub struct CredentialAlreadyLinkedError;

impl std::fmt::Display for CredentialAlreadyLinkedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("that credential is already linked to an account")
    }
}

impl std::error::Error for CredentialAlreadyLinkedError {}

/// Marker error returned when the insert violates the partial
/// `UNIQUE(user_id) WHERE kind='password'` index — the user already has a
/// password. Callers that mean "replace it" must call
/// [`UserCredentialRepository::set_password`] instead.
#[derive(Debug)]
pub struct UserAlreadyHasPasswordError;

impl std::fmt::Display for UserAlreadyHasPasswordError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("that user already has a password credential")
    }
}

impl std::error::Error for UserAlreadyHasPasswordError {}

/// The kind of credential a `user_credentials` row holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredentialKind {
    /// `Argon2id` password. The floor — works with no WAN, no certificate and
    /// no provider, and is never removable.
    Password,
    /// Google, verified against the provider's userinfo endpoint.
    Google,
    /// `GitHub`, verified against the provider's userinfo endpoint.
    Github,
    /// `WebAuthn` passkey.
    Passkey,
}

impl CredentialKind {
    /// The `kind` column value. Must match the migration's `CHECK` constraint.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Password => "password",
            Self::Google => "google",
            Self::Github => "github",
            Self::Passkey => "passkey",
        }
    }

    /// Parse a `kind` column value. `None` for anything else — an unknown
    /// credential kind must never be treated as a usable one.
    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "password" => Some(Self::Password),
            "google" => Some(Self::Google),
            "github" => Some(Self::Github),
            "passkey" => Some(Self::Passkey),
            _ => None,
        }
    }
}

/// A full credential row, **including the secret**.
///
/// Deliberately not `Serialize`, and only produced by the lookup methods that
/// genuinely need to verify something. Everything user-facing goes through
/// [`CredentialSummary`] instead.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CredentialRow {
    /// Credential UUID (primary key).
    pub id: String,
    /// Owning user.
    pub user_id: String,
    /// What sort of credential this is.
    pub kind: CredentialKind,
    /// The login identifier: username (backfilled local admin), email (new
    /// local user), Google `sub`, `GitHub` **numeric id**, or the base64url
    /// passkey credential id.
    pub subject: String,
    /// `Argon2id` PHC string, or the passkey COSE public key.
    ///
    /// **Must never leave the repository layer.** Nothing that returns this
    /// type may be handed to a serializer.
    pub secret: Option<String>,
    /// Human label shown in the credential list ("Pixel 8 passkey").
    pub label: Option<String>,
    /// Kind-specific JSON: passkey sign count, backup state and transports;
    /// the provider account's login/handle for the OAuth kinds.
    pub metadata: String,
    /// RFC 3339 creation timestamp.
    pub created_at: String,
    /// RFC 3339 timestamp of the last successful authentication, or `None`.
    pub last_used_at: Option<String>,
}

/// A credential as shown to a user or an admin — **structurally without the
/// secret**.
///
/// This is the enforcement mechanism for "the secret never leaves the
/// repository layer" (ADR-0031 §1): listing methods return this type, which
/// has no field to leak. "Remember not to serialise `secret`" would be a
/// review comment; this is a type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CredentialSummary {
    /// Credential UUID.
    pub id: String,
    /// Owning user.
    pub user_id: String,
    /// What sort of credential this is.
    pub kind: CredentialKind,
    /// The login identifier. Safe to show: for a passkey it is an opaque
    /// credential id, and for OAuth it is the provider's own subject.
    pub subject: String,
    /// Human label shown in the credential list.
    pub label: Option<String>,
    /// Kind-specific JSON (never contains the secret).
    pub metadata: String,
    /// RFC 3339 creation timestamp.
    pub created_at: String,
    /// RFC 3339 timestamp of the last successful authentication.
    pub last_used_at: Option<String>,
}

/// The result of a login lookup: a credential joined to its user.
///
/// Produced by [`UserCredentialRepository::find_for_login`], which filters out
/// disabled users in SQL — so a caller cannot forget to check `enabled`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CredentialLogin {
    /// The credential to verify against.
    pub credential: CredentialRow,
    /// The authenticated user's role, needed to build the `AuthContext`.
    pub role: UserRole,
    /// The user's display name, for the session response.
    pub display_name: String,
}

/// Data access for `user_credentials`.
#[async_trait]
pub trait UserCredentialRepository: Send + Sync {
    /// Insert a credential row.
    ///
    /// A `(kind, subject)` violation MUST downcast to
    /// [`CredentialAlreadyLinkedError`]; a second-password violation MUST
    /// downcast to [`UserAlreadyHasPasswordError`]. Any other failure is
    /// returned as an ordinary error.
    async fn insert(&self, row: &CredentialRow) -> anyhow::Result<()>;

    /// Look up a credential for login, joined to its user.
    ///
    /// Returns `None` when no such credential exists **or** when its user is
    /// disabled — the two are deliberately indistinguishable to the caller, so
    /// a disabled account cannot be probed. Callers run the decoy-hash
    /// constant-work path on `None` regardless.
    async fn find_for_login(
        &self,
        kind: CredentialKind,
        subject: &str,
    ) -> anyhow::Result<Option<CredentialLogin>>;

    /// The user's password credential, if any. Used to verify a password
    /// change and to answer "does this user have the floor credential?".
    async fn find_password(&self, user_id: &str) -> anyhow::Result<Option<CredentialRow>>;

    /// Every credential a user holds, without secrets, newest first.
    async fn list_for_user(&self, user_id: &str) -> anyhow::Result<Vec<CredentialSummary>>;

    /// Create or replace the user's password credential in one statement.
    ///
    /// Upsert rather than delete-then-insert: the partial unique index makes
    /// the two-statement form a window in which the user has no password at
    /// all, and a crash inside that window locks them out permanently.
    async fn set_password(
        &self,
        id: &str,
        user_id: &str,
        subject: &str,
        secret: &str,
        now: &str,
    ) -> anyhow::Result<()>;

    /// Record a successful authentication.
    async fn touch_last_used(&self, id: &str, now: &str) -> anyhow::Result<()>;

    /// Replace a credential's `metadata` JSON — the passkey sign count,
    /// backup state and transports write-back path.
    async fn update_metadata(&self, id: &str, metadata: &str) -> anyhow::Result<u64>;

    /// Delete one credential, scoped to its owner so a caller cannot delete
    /// somebody else's by guessing an id.
    async fn delete(&self, id: &str, user_id: &str) -> anyhow::Result<u64>;

    /// Delete every credential of one kind for a user. Backs "reset passkeys"
    /// after an RP-ID divergence, and unlinking a provider.
    async fn delete_by_kind(&self, user_id: &str, kind: CredentialKind) -> anyhow::Result<u64>;

    /// Count a user's credentials of one kind. Backs the "you cannot remove
    /// your last password" rule — the password is the floor.
    async fn count_by_kind(&self, user_id: &str, kind: CredentialKind) -> anyhow::Result<i64>;
}
