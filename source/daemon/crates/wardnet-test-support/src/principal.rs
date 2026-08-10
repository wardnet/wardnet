//! Principal builders for tests.
//!
//! `AuthenticatedUser`'s fields are private and its constructor is guarded by
//! `build-support/check-auth-constructors.sh`, which exists to stop production
//! code minting a principal it has not verified. Tests legitimately need to
//! fabricate every principal — that is what a guard × variant truth table *is* —
//! so the helpers live here, in one of the script's allowlisted locations,
//! rather than being sprinkled across thirty test stubs.
//!
//! Keeping them here has a second benefit: a test that wants an admin says
//! [`admin`], and one that wants a member says [`member`]. When the next
//! principal is added, the diff shows which tests deliberately chose a role.

use uuid::Uuid;
use wardnet_common::auth::{AuthContext, AuthenticatedUser, UserRole};

/// An `admin`-role principal with the given id.
#[must_use]
pub fn admin(user_id: Uuid) -> AuthenticatedUser {
    AuthenticatedUser::from_validated_session(user_id, UserRole::Admin)
}

/// A `member`-role principal with the given id.
#[must_use]
pub fn member(user_id: Uuid) -> AuthenticatedUser {
    AuthenticatedUser::from_validated_session(user_id, UserRole::Member)
}

/// An `admin`-role principal with a fresh random id.
#[must_use]
pub fn some_admin() -> AuthenticatedUser {
    admin(Uuid::new_v4())
}

/// A `member`-role principal with a fresh random id.
#[must_use]
pub fn some_member() -> AuthenticatedUser {
    member(Uuid::new_v4())
}

/// An admin [`AuthContext`], for `with_context` in service tests.
#[must_use]
pub fn admin_context(user_id: Uuid) -> AuthContext {
    AuthContext::user(admin(user_id))
}

/// A member [`AuthContext`], for `with_context` in service tests.
#[must_use]
pub fn member_context(user_id: Uuid) -> AuthContext {
    AuthContext::user(member(user_id))
}

/// A device [`AuthContext`] keyed by MAC.
#[must_use]
pub fn device_context(mac: &str) -> AuthContext {
    AuthContext::Device {
        mac: mac.to_owned(),
    }
}
