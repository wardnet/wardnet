use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Access role for API requests.
///
/// This is the **OpenAPI security scope** vocabulary, not the household-user
/// role — see [`UserRole`] for that. The two read confusingly side by side;
/// ADR-0031 flags this deliberately rather than renaming, because renaming
/// churns every `#[utoipa::path]` annotation in the tree for a cosmetic gain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Admin,
    Public,
}

/// A household user's role (ADR-0031 §11).
///
/// `Admin` is *exactly* equal to the legacy local admin — no deny-list, no
/// second tier. Deliberately only two values: a household is 2–6 people, and
/// an allow-list is honest at that size.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum UserRole {
    /// May do everything.
    Admin,
    /// An ordinary household member.
    Member,
}

impl UserRole {
    /// The wire/column value. Must match the `users.role` `CHECK` constraint.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Admin => "admin",
            Self::Member => "member",
        }
    }

    /// Parse a role value. `None` for anything unrecognised — silently
    /// defaulting an unknown role to `Admin` would be a privilege escalation.
    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "admin" => Some(Self::Admin),
            "member" => Some(Self::Member),
            _ => None,
        }
    }
}

/// Proof that a household user authenticated.
///
/// The fields are **private**, so an `AuthContext::User` cannot be assembled
/// from a `user_id` somebody happened to have lying around. In particular
/// `devices.owner_user_id` — which says who a device belongs to and grants
/// nothing — is a plain `Option<Uuid>` on a device row with no path into this
/// type (ADR-0031 §4).
///
/// # The one rule
///
/// [`from_validated_session`](Self::from_validated_session) must be called
/// **only** from the code that has just verified a credential: session
/// validation, API-key validation, and [`system`](AuthContext::system) for
/// background work. Calling it anywhere else — above all from anything holding
/// a `Device` — is a privilege escalation, and is a defect regardless of how
/// the surrounding code reads.
///
/// This is enforced three ways: the constructor's name makes every call site
/// self-describing, `build-support/check-auth-constructors.sh` fails CI if it
/// appears outside the sanctioned files, and a regression test asserts that a
/// device owned by an `admin`-role user still resolves to `Device`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthenticatedUser {
    user_id: Uuid,
    role: UserRole,
}

impl AuthenticatedUser {
    /// Build the proof. See the type docs: only credential-verifying code may
    /// call this.
    #[must_use]
    pub const fn from_validated_session(user_id: Uuid, role: UserRole) -> Self {
        Self { user_id, role }
    }

    /// The authenticated user's id.
    #[must_use]
    pub const fn user_id(&self) -> Uuid {
        self.user_id
    }

    /// The authenticated user's role.
    #[must_use]
    pub const fn role(&self) -> UserRole {
        self.role
    }
}

/// Identity and authorization context for the current request.
///
/// Set by API middleware and made available to services via
/// `tokio::task_local!`.
///
/// # Never a wildcard arm
///
/// Every `match` over an `AuthContext` must list every variant explicitly.
/// `_ =>` means the next principal somebody adds lands in whichever branch
/// happens to be last, silently — and several call sites branch on `Device`
/// and let everything else fall through to the *admin* path. For the same
/// reason, an `Anonymous` arm that "cannot happen" returns
/// `Forbidden` rather than `unreachable!()`: an authorization bug must not
/// become a remotely-triggerable panic. See `.agents/auth.md`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthContext {
    /// An authenticated household user. Replaces the former `Admin` variant:
    /// one principal for "a person who signed in", carrying their role, so
    /// `require_admin()` is a single honest predicate at every call site.
    User(AuthenticatedUser),
    /// Self-service caller identified by their device MAC address. Holds no
    /// credential — its identity is its presence on the network.
    Device {
        /// The MAC address of the caller's device.
        mac: String,
    },
    /// No identity resolved (e.g. unknown IP, public info endpoints).
    Anonymous,
}

impl AuthContext {
    /// Build a user context from a validated session or API key.
    #[must_use]
    pub const fn user(user: AuthenticatedUser) -> Self {
        Self::User(user)
    }

    /// The context background tasks and runners use.
    ///
    /// `Uuid::nil()` is the reserved **system actor**: it is what keeps
    /// background work distinguishable from a real person in audit logs, and
    /// the household-identity migration deliberately refuses to create a
    /// `users` row bearing it. Use this rather than hand-rolling the variant.
    #[must_use]
    pub const fn system() -> Self {
        Self::User(AuthenticatedUser::from_validated_session(
            Uuid::nil(),
            UserRole::Admin,
        ))
    }

    /// Returns `true` if the caller is a household user with `role = Admin`.
    #[must_use]
    pub fn is_admin(&self) -> bool {
        match self {
            Self::User(user) => user.role() == UserRole::Admin,
            Self::Device { .. } | Self::Anonymous => false,
        }
    }

    /// Returns `true` if the caller holds any authenticated identity — a
    /// household user of either role, or a device.
    ///
    /// A **positive** predicate on purpose. `!matches!(self, Anonymous)` would
    /// admit every principal added in the future with no compile error.
    #[must_use]
    pub fn is_authenticated(&self) -> bool {
        match self {
            Self::User(_) | Self::Device { .. } => true,
            Self::Anonymous => false,
        }
    }

    /// The authenticated user's id, if this is a [`User`](Self::User) context.
    #[must_use]
    pub fn user_id(&self) -> Option<Uuid> {
        match self {
            Self::User(user) => Some(user.user_id()),
            Self::Device { .. } | Self::Anonymous => None,
        }
    }

    /// The authenticated user's role, if this is a [`User`](Self::User)
    /// context.
    #[must_use]
    pub fn role(&self) -> Option<UserRole> {
        match self {
            Self::User(user) => Some(user.role()),
            Self::Device { .. } | Self::Anonymous => None,
        }
    }

    /// Returns the device MAC if this is a [`Device`](Self::Device) context.
    ///
    /// Every variant is listed explicitly and no wildcard arm is permitted:
    /// adding a principal must fail to compile here rather than silently
    /// resolve to `None`.
    #[must_use]
    pub fn device_mac(&self) -> Option<&str> {
        match self {
            Self::Device { mac } => Some(mac),
            Self::User(_) | Self::Anonymous => None,
        }
    }
}

/// An authenticated household-user session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: Uuid,
    pub user_id: Uuid,
    pub token_hash: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

/// A stored API key record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeyRecord {
    pub id: Uuid,
    pub label: String,
    pub key_hash: String,
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
}
