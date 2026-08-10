//! The guard × principal truth table (ADR-0031 §3).
//!
//! Every authorization decision in the daemon reduces to one of two guards
//! applied to one of three principals. This file writes that grid out in full
//! and asserts each cell, so **widening a guard cannot be a silent diff**:
//! adding a principal or changing what a guard admits forces an edit here, and
//! that edit shows up in review next to the code that motivated it.
//!
//! `.agents/auth.md` points at this file as the tripwire. It is deliberately
//! exhaustive rather than sampled — a table with a missing row is exactly the
//! shape of the bug it exists to catch, because the interesting cells are the
//! ones nobody thought about.

use uuid::Uuid;
use wardnet_common::auth::{AuthContext, UserRole};

use crate::auth_context;
use crate::error::AppError;

/// Which guard is under test.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Guard {
    /// `require_admin()` — the caller must be a `role = admin` household user.
    Admin,
    /// `require_authenticated()` — the caller must hold any identity.
    Authenticated,
}

impl Guard {
    /// Run the guard under `ctx` and report whether it admitted the caller.
    async fn admits(self, ctx: AuthContext) -> bool {
        let result = auth_context::with_context(ctx, async move {
            match self {
                Self::Admin => auth_context::require_admin(),
                Self::Authenticated => auth_context::require_authenticated(),
            }
        })
        .await;

        match result {
            Ok(()) => true,
            // A refusal must be `Forbidden`, never a panic and never `Internal`:
            // an authorization decision is a normal outcome, and turning one
            // into a 500 loses the distinction a caller needs.
            Err(AppError::Forbidden(_)) => false,
            Err(other) => panic!("a guard must refuse with Forbidden, got {other:?}"),
        }
    }
}

/// Every principal the daemon can present, with a name for failure messages.
fn principals() -> Vec<(&'static str, AuthContext)> {
    vec![
        (
            "user{role=admin}",
            AuthContext::user(wardnet_test_support::principal::admin(Uuid::new_v4())),
        ),
        (
            "user{role=member}",
            AuthContext::user(wardnet_test_support::principal::member(Uuid::new_v4())),
        ),
        ("system() (nil uuid, role=admin)", AuthContext::system()),
        (
            "device",
            AuthContext::Device {
                mac: "AA:BB:CC:DD:EE:01".to_owned(),
            },
        ),
        ("anonymous", AuthContext::Anonymous),
    ]
}

/// The table. Each row is `(guard, principal name, admitted?)`.
///
/// Read the `false` cells as the security properties they are, not as
/// omissions:
///
/// - `require_admin` × `user{role=member}` — a member is authenticated but not
///   an administrator. This is the single cell that made the whole epic
///   necessary; before household users, every session was admin by
///   construction.
/// - `require_admin` × `device` — a device holds no credential; its identity
///   is its presence on the network. `devices.owner_user_id` says who a device
///   belongs to and grants nothing (ADR-0031 §4), so a device owned by an admin
///   still fails here.
/// - `require_authenticated` × `device` — deliberately **true**: this is the
///   self-service surface (`/api/devices/me`), and a device is a real principal
///   there.
const TABLE: &[(Guard, &str, bool)] = &[
    (Guard::Admin, "user{role=admin}", true),
    (Guard::Admin, "user{role=member}", false),
    (Guard::Admin, "system() (nil uuid, role=admin)", true),
    (Guard::Admin, "device", false),
    (Guard::Admin, "anonymous", false),
    (Guard::Authenticated, "user{role=admin}", true),
    (Guard::Authenticated, "user{role=member}", true),
    (
        Guard::Authenticated,
        "system() (nil uuid, role=admin)",
        true,
    ),
    (Guard::Authenticated, "device", true),
    (Guard::Authenticated, "anonymous", false),
];

#[tokio::test]
async fn guard_by_principal_truth_table() {
    let principals = principals();

    for (guard, name, expected) in TABLE {
        let ctx = principals
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, c)| c.clone())
            .unwrap_or_else(|| panic!("the table names a principal that does not exist: {name}"));

        let actual = guard.admits(ctx).await;
        assert_eq!(
            actual, *expected,
            "{guard:?} × {name}: expected admitted={expected}, got {actual}"
        );
    }
}

#[tokio::test]
async fn the_table_covers_every_principal_for_every_guard() {
    // The table is only a tripwire if it is complete. Without this, adding a
    // principal would leave it silently under-tested — which is the failure
    // mode the table exists to prevent.
    let principals = principals();
    for guard in [Guard::Admin, Guard::Authenticated] {
        for (name, _) in &principals {
            assert!(
                TABLE.iter().any(|(g, n, _)| *g == guard && n == name),
                "the truth table is missing {guard:?} × {name}; add the cell and \
                 say in the PR why the answer is what it is"
            );
        }
    }
    assert_eq!(
        TABLE.len(),
        principals.len() * 2,
        "the truth table has a duplicate or stale row"
    );
}

#[tokio::test]
async fn a_missing_context_is_treated_as_anonymous() {
    // Background tasks and tests can call a guarded method with no task-local
    // set at all. That must deny, not panic and not pass: `try_current` returns
    // `None` and both guards fall back to `Anonymous`.
    assert!(matches!(
        auth_context::require_admin(),
        Err(AppError::Forbidden(_))
    ));
    assert!(matches!(
        auth_context::require_authenticated(),
        Err(AppError::Forbidden(_))
    ));
}

#[tokio::test]
async fn the_system_actor_is_the_nil_uuid_and_an_admin() {
    // Runners act as `AuthContext::system()`. The nil UUID is the reserved
    // system actor, which is what keeps background work distinguishable from a
    // real person in audit logs — and why the migration refuses to create a
    // `users` row bearing it.
    let ctx = AuthContext::system();
    assert_eq!(ctx.user_id(), Some(Uuid::nil()));
    assert_eq!(ctx.role(), Some(UserRole::Admin));
    assert!(ctx.is_admin());
}

#[tokio::test]
async fn device_affinity_never_produces_a_user_context() {
    // The non-escalation guarantee, at the level this file can assert it: a
    // `Device` context yields no user id and no role, so there is no value for
    // a plausible-looking line to promote into `AuthContext::User`. The
    // structural half of the guarantee is `AuthenticatedUser`'s private fields
    // plus `build-support/check-auth-constructors.sh`; the end-to-end half is
    // the API-layer regression test that a request from a device owned by an
    // admin-role user still resolves to `Device`.
    let device = AuthContext::Device {
        mac: "AA:BB:CC:DD:EE:01".to_owned(),
    };

    assert_eq!(device.user_id(), None);
    assert_eq!(device.role(), None);
    assert!(!device.is_admin());
    assert!(device.is_authenticated());
    assert_eq!(device.device_mac(), Some("AA:BB:CC:DD:EE:01"));
}

#[tokio::test]
async fn a_user_context_carries_no_device_mac() {
    // The converse direction: nothing lets a signed-in person be mistaken for
    // a device, which would hand them the self-service surface of whatever MAC
    // was inferred.
    let user = AuthContext::user(wardnet_test_support::principal::some_admin());
    assert_eq!(user.device_mac(), None);
}
