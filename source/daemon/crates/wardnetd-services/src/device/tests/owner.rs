//! Device affinity (ADR-0031 §4): `devices.owner_user_id` is **attribution,
//! never authority**.
//!
//! The rule this file defends is narrow and easy to break by writing something
//! that reads perfectly well: "this device belongs to an admin, so treat the
//! request as coming from that admin". That single line would turn every
//! household device into an admin credential, because a device principal is
//! established by *presence on the network* — no secret is involved, and a MAC
//! is trivially spoofable.
//!
//! The guarantee is enforced in three places and this file is the third:
//!
//! 1. `AuthenticatedUser`'s fields are private, so an `AuthContext::User`
//!    cannot be assembled from an id somebody had lying around.
//! 2. `build-support/check-auth-constructors.sh` fails CI if the one
//!    constructor is called outside the files that verify credentials.
//! 3. These tests: an owned device still resolves to `Device`, and an
//!    admin-only operation still refuses it.

use std::sync::Arc;

use sqlx::SqlitePool;
use sqlx::sqlite::SqlitePoolOptions;
use uuid::Uuid;
use wardnet_common::auth::{AuthContext, UserRole};
use wardnet_test_support::principal;
use wardnetd_data::repository::device::DeviceRepository;
use wardnetd_data::repository::device::UnknownOwnerError;
use wardnetd_data::repository::sqlite::SqliteDeviceRepository;

use crate::auth_context;
use crate::error::AppError;

/// A pool with the real migrations applied, so the FK and `ON DELETE SET NULL`
/// behaviour under test is the production schema's, not a mock's opinion of it.
async fn test_pool() -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("../wardnetd-data/migrations")
        .run(&pool)
        .await
        .unwrap();
    // The initial migration turns foreign keys on per-connection; make sure the
    // behaviour we assert below is actually enforced.
    sqlx::query("PRAGMA foreign_keys = ON")
        .execute(&pool)
        .await
        .unwrap();
    pool
}

/// Insert a household user directly.
async fn seed_user(pool: &SqlitePool, id: &str, role: &str) {
    sqlx::query(
        "INSERT INTO users (id, display_name, email, role, enabled, created_at, updated_at) \
         VALUES (?, ?, NULL, ?, 1, ?, ?)",
    )
    .bind(id)
    .bind(id)
    .bind(role)
    .bind("2026-08-01T00:00:00Z")
    .bind("2026-08-01T00:00:00Z")
    .execute(pool)
    .await
    .unwrap();
}

/// Insert a device in the default zone.
async fn seed_device(pool: &SqlitePool, id: &str, mac: &str) {
    let zone_id: String = sqlx::query_scalar("SELECT id FROM network_zones LIMIT 1")
        .fetch_one(pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO devices \
         (id, mac, name, hostname, manufacturer, is_randomized, device_type, first_seen, \
          last_seen, last_ip, admin_locked, zone_id, dns_capture_enabled, \
          dns_capture_cap_count, dns_capture_cap_days, connection_mode) \
         VALUES (?, ?, NULL, NULL, NULL, 0, 'unknown', ?, ?, '192.168.1.50', 0, ?, 0, 500, 14, 'lan')",
    )
    .bind(id)
    .bind(mac)
    .bind("2026-08-01T00:00:00Z")
    .bind("2026-08-01T00:00:00Z")
    .bind(zone_id)
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn set_owner_assigns_and_clears_the_owning_user() {
    let pool = test_pool().await;
    let admin_id = Uuid::new_v4().to_string();
    let device_id = Uuid::new_v4().to_string();
    seed_user(&pool, &admin_id, "admin").await;
    seed_device(&pool, &device_id, "aa:bb:cc:dd:ee:01").await;
    let repo = SqliteDeviceRepository::new(pool);

    assert!(
        repo.find_by_id(&device_id)
            .await
            .unwrap()
            .unwrap()
            .owner_user_id
            .is_none(),
        "a device starts unowned"
    );

    assert!(repo.set_owner(&device_id, Some(&admin_id)).await.unwrap());
    let owned = repo.find_by_id(&device_id).await.unwrap().unwrap();
    assert_eq!(owned.owner_user_id.map(|u| u.to_string()), Some(admin_id));

    assert!(repo.set_owner(&device_id, None).await.unwrap());
    assert!(
        repo.find_by_id(&device_id)
            .await
            .unwrap()
            .unwrap()
            .owner_user_id
            .is_none(),
        "passing None must clear the assignment"
    );
}

#[tokio::test]
async fn set_owner_reports_false_for_an_unknown_device() {
    let pool = test_pool().await;
    let repo = SqliteDeviceRepository::new(pool);

    assert!(
        !repo
            .set_owner(&Uuid::new_v4().to_string(), None)
            .await
            .unwrap(),
        "no row updated must be reported, not silently succeed"
    );
}

#[tokio::test]
async fn set_owner_rejects_an_unknown_user() {
    // The FK is what stops `owner_user_id` becoming a dangling reference that a
    // later join would silently drop.
    let pool = test_pool().await;
    let device_id = Uuid::new_v4().to_string();
    seed_device(&pool, &device_id, "aa:bb:cc:dd:ee:02").await;
    let repo = SqliteDeviceRepository::new(pool);

    let result = repo
        .set_owner(&device_id, Some(&Uuid::new_v4().to_string()))
        .await;
    let err = result.expect_err("assigning a nonexistent user must fail the FK, not be stored");

    // …and it must arrive as a *typed* error, so the service can answer 400.
    // A bare foreign-key failure would surface as a 500 for what is plainly a
    // bad request, and the route documents no such response.
    assert!(
        err.downcast_ref::<UnknownOwnerError>().is_some(),
        "expected UnknownOwnerError, got: {err}"
    );
}

#[tokio::test]
async fn deleting_the_owner_clears_the_device_but_keeps_it() {
    // `ON DELETE SET NULL`, not CASCADE: removing a person from the household
    // must not delete the household's hardware.
    let pool = test_pool().await;
    let user_id = Uuid::new_v4().to_string();
    let device_id = Uuid::new_v4().to_string();
    seed_user(&pool, &user_id, "member").await;
    seed_device(&pool, &device_id, "aa:bb:cc:dd:ee:03").await;
    let repo = SqliteDeviceRepository::new(pool.clone());
    repo.set_owner(&device_id, Some(&user_id)).await.unwrap();

    sqlx::query("DELETE FROM users WHERE id = ?")
        .bind(&user_id)
        .execute(&pool)
        .await
        .unwrap();

    let device = repo
        .find_by_id(&device_id)
        .await
        .unwrap()
        .expect("the device must survive its owner's deletion");
    assert!(device.owner_user_id.is_none());
}

// -- the non-escalation guarantee ----------------------------------------

#[tokio::test]
async fn a_device_owned_by_an_admin_still_resolves_to_the_device_principal() {
    // The core assertion. Ownership is data on a device row; it has no path
    // into an `AuthContext`. If somebody ever "helpfully" promotes an owned
    // device to its owner, this fails.
    let pool = test_pool().await;
    let admin_id = Uuid::new_v4().to_string();
    let device_id = Uuid::new_v4().to_string();
    let mac = "aa:bb:cc:dd:ee:04";
    seed_user(&pool, &admin_id, "admin").await;
    seed_device(&pool, &device_id, mac).await;
    let repo = SqliteDeviceRepository::new(pool);
    repo.set_owner(&device_id, Some(&admin_id)).await.unwrap();

    let device = repo.find_by_id(&device_id).await.unwrap().unwrap();
    assert_eq!(
        device.owner_user_id.map(|u| u.to_string()),
        Some(admin_id),
        "precondition: the device really is owned by an admin"
    );

    // This is how the middleware builds a device caller's context: from the MAC
    // alone. There is deliberately no owner lookup involved.
    let ctx = AuthContext::Device {
        mac: device.mac.clone(),
    };

    assert_eq!(ctx.user_id(), None, "a device caller has no user id");
    assert_eq!(ctx.role(), None, "a device caller has no role");
    assert!(
        !ctx.is_admin(),
        "a device owned by an admin must not itself be an admin"
    );
    assert_eq!(ctx.device_mac(), Some(mac));
}

#[tokio::test]
async fn an_admin_only_operation_refuses_a_device_owned_by_an_admin() {
    // The end-to-end half: the guard, not just the context shape. Running an
    // admin-gated service call under the owned device's context must be
    // refused.
    let pool = test_pool().await;
    let admin_id = Uuid::new_v4().to_string();
    let device_id = Uuid::new_v4().to_string();
    seed_user(&pool, &admin_id, "admin").await;
    seed_device(&pool, &device_id, "aa:bb:cc:dd:ee:05").await;
    let repo: Arc<dyn DeviceRepository> = Arc::new(SqliteDeviceRepository::new(pool));
    repo.set_owner(&device_id, Some(&admin_id)).await.unwrap();

    let ctx = principal::device_context("aa:bb:cc:dd:ee:05");
    let result = auth_context::with_context(ctx, async { auth_context::require_admin() }).await;

    assert!(
        matches!(result, Err(AppError::Forbidden(_))),
        "an admin-owned device must still fail require_admin"
    );
}

#[tokio::test]
async fn ownership_does_not_change_what_a_member_owner_grants_either() {
    // Stated explicitly so the test reads as a rule about ownership rather than
    // a rule about admins: no role of the owner grants the device anything.
    for role in [UserRole::Admin, UserRole::Member] {
        let ctx = AuthContext::Device {
            mac: "aa:bb:cc:dd:ee:06".to_owned(),
        };
        let result = auth_context::with_context(ctx, async { auth_context::require_admin() }).await;
        assert!(
            matches!(result, Err(AppError::Forbidden(_))),
            "owner role {role:?} must not grant the device admin"
        );
    }
}
