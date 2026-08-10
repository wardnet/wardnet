//! Upgrade tests for `20260810000000_household_identity.sql`.
//!
//! The plan calls this migration the highest-risk item in the epic, and it is:
//! `SQLite` cannot alter a foreign key in place, so `sessions` is rebuilt, and a
//! mistake leaves a daemon that will not start and cannot roll back.
//!
//! These tests seed a **pre-upgrade** database — the schema as it stood before
//! this migration, with `admins` rows and live sessions — then run the migration
//! and assert the properties an operator depends on.

use sqlx::SqlitePool;
use sqlx::sqlite::SqlitePoolOptions;

/// Apply every migration **up to but excluding** the household-identity one, so
/// the test can seed a genuine pre-upgrade database.
///
/// `sqlx::migrate!` has no "run up to" control, so the pre-upgrade schema is
/// spelled out here for exactly the tables this migration reads. It is
/// deliberately minimal: `admins`, `sessions` as it was, and the `network_zones`
/// + `devices` rows the FK work needs.
async fn pre_upgrade_pool() -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .connect("sqlite::memory:")
        .await
        .unwrap();

    sqlx::query("PRAGMA foreign_keys = ON")
        .execute(&pool)
        .await
        .unwrap();

    for statement in [
        "CREATE TABLE admins (
             id TEXT PRIMARY KEY,
             username TEXT NOT NULL UNIQUE,
             password_hash TEXT NOT NULL,
             created_at TEXT NOT NULL
         )",
        "CREATE TABLE sessions (
             id TEXT PRIMARY KEY,
             admin_id TEXT NOT NULL REFERENCES admins(id) ON DELETE CASCADE,
             token_hash TEXT NOT NULL,
             created_at TEXT NOT NULL,
             expires_at TEXT NOT NULL,
             remember_me INTEGER NOT NULL DEFAULT 0
         )",
        "CREATE INDEX idx_sessions_token_hash ON sessions(token_hash)",
        "CREATE INDEX idx_sessions_expires_at ON sessions(expires_at)",
        "CREATE TABLE network_zones (
             id TEXT PRIMARY KEY,
             name TEXT NOT NULL,
             is_default_for_new INTEGER NOT NULL DEFAULT 0
         )",
        "CREATE TABLE devices (
             id TEXT PRIMARY KEY,
             mac TEXT NOT NULL UNIQUE,
             zone_id TEXT NOT NULL REFERENCES network_zones(id)
         )",
        "CREATE TABLE push_subscriptions (
             id TEXT PRIMARY KEY,
             owner_kind TEXT NOT NULL,
             owner_key TEXT NOT NULL,
             endpoint TEXT NOT NULL UNIQUE,
             p256dh TEXT NOT NULL,
             auth TEXT NOT NULL,
             created_at TEXT NOT NULL
         )",
        "INSERT INTO network_zones (id, name) VALUES ('zone-1', 'Trusted')",
    ] {
        sqlx::query(statement).execute(&pool).await.unwrap();
    }

    pool
}

/// Seed one admin.
async fn seed_admin(pool: &SqlitePool, id: &str, username: &str, created_at: &str) {
    sqlx::query("INSERT INTO admins (id, username, password_hash, created_at) VALUES (?, ?, ?, ?)")
        .bind(id)
        .bind(username)
        .bind(format!("$argon2-hash-for-{username}"))
        .bind(created_at)
        .execute(pool)
        .await
        .unwrap();
}

/// Seed a session for an admin.
async fn seed_session(
    pool: &SqlitePool,
    id: &str,
    admin_id: &str,
    created_at: &str,
    expires_at: &str,
    remember_me: bool,
) {
    sqlx::query(
        "INSERT INTO sessions (id, admin_id, token_hash, created_at, expires_at, remember_me) \
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(id)
    .bind(admin_id)
    .bind(format!("hash-{id}"))
    .bind(created_at)
    .bind(expires_at)
    .bind(remember_me)
    .execute(pool)
    .await
    .unwrap();
}

/// Run just the household-identity migration against a seeded pre-upgrade pool.
async fn run_migration(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    let sql = include_str!("../../migrations/20260810000000_household_identity.sql");
    // The real migration runs inside sqlx's per-migration transaction, which is
    // what makes `defer_foreign_keys` the correct pragma; reproduce that here.
    let mut tx = pool.begin().await?;
    sqlx::raw_sql(sql).execute(&mut *tx).await?;
    tx.commit().await
}

#[tokio::test]
async fn a_single_admin_is_backfilled_with_a_lowercased_login_subject() {
    let pool = pre_upgrade_pool().await;
    seed_admin(&pool, "admin-1", "Operator", "2026-01-01T00:00:00+00:00").await;

    run_migration(&pool)
        .await
        .expect("the migration must apply");

    // The id is preserved — that is what makes the sessions rebuild a rename.
    let (id, role, enabled): (String, String, bool) =
        sqlx::query_as("SELECT id, role, enabled FROM users")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(id, "admin-1");
    assert_eq!(role, "admin");
    assert!(enabled);

    // The subject is lowercased, because the login path lowercases what the
    // operator types and matches the column exactly.
    let subject: String = sqlx::query_scalar(
        "SELECT subject FROM user_credentials WHERE kind = 'password' AND user_id = 'admin-1'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(subject, "operator");
}

#[tokio::test]
async fn usernames_differing_only_by_case_do_not_abort_the_migration() {
    // A mistake here is an unbootable box, so the collision must never abort.
    let pool = pre_upgrade_pool().await;
    seed_admin(&pool, "admin-old", "Ann", "2026-01-01T00:00:00+00:00").await;
    seed_admin(&pool, "admin-new", "ann", "2026-02-01T00:00:00+00:00").await;

    run_migration(&pool)
        .await
        .expect("a case collision must not abort the migration");

    // Both people survive, with their ids and their admin role.
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE role = 'admin'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 2, "nobody may be dropped by the upgrade");
}

#[tokio::test]
async fn a_case_collision_gives_the_login_to_the_oldest_admin_and_leaves_no_dead_subject() {
    // Under the new scheme `Ann` and `ann` are one login. The oldest keeps it;
    // the other arrives credential-less and is re-enrolled. Critically, no
    // credential row may carry a mixed-case subject: the login path always
    // lowercases, so such a row could never be matched and that admin would be
    // locked out with no recovery.
    let pool = pre_upgrade_pool().await;
    seed_admin(&pool, "admin-old", "Ann", "2026-01-01T00:00:00+00:00").await;
    seed_admin(&pool, "admin-new", "ann", "2026-02-01T00:00:00+00:00").await;

    run_migration(&pool).await.unwrap();

    let rows: Vec<(String, String)> =
        sqlx::query_as("SELECT user_id, subject FROM user_credentials WHERE kind = 'password'")
            .fetch_all(&pool)
            .await
            .unwrap();

    assert_eq!(
        rows.len(),
        1,
        "exactly one of the colliding admins may own the case-insensitive login"
    );
    assert_eq!(rows[0].0, "admin-old", "the oldest admin keeps the login");
    assert_eq!(rows[0].1, "ann");

    for (_, subject) in &rows {
        assert_eq!(
            *subject,
            subject.to_lowercase(),
            "a mixed-case subject is unreachable by the login path"
        );
    }

    // The other admin has no password, which is exactly the state
    // `issue_enrolment` accepts — so they have a recovery path.
    let has_credential: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM user_credentials WHERE user_id = 'admin-new')",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(!has_credential);
}

#[tokio::test]
async fn live_sessions_survive_the_rebuild() {
    let pool = pre_upgrade_pool().await;
    seed_admin(&pool, "admin-1", "operator", "2026-01-01T00:00:00+00:00").await;
    seed_session(
        &pool,
        "session-1",
        "admin-1",
        "2026-08-01T00:00:00+00:00",
        "2026-08-31T00:00:00+00:00",
        true,
    )
    .await;

    run_migration(&pool).await.unwrap();

    let (user_id, token_hash, remember_me): (String, String, bool) =
        sqlx::query_as("SELECT user_id, token_hash, remember_me FROM sessions")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(user_id, "admin-1", "admin_id becomes user_id unchanged");
    assert_eq!(token_hash, "hash-session-1");
    assert!(remember_me, "remember_me must carry across");
}

#[tokio::test]
async fn a_migrated_session_keeps_a_sliding_window() {
    // The ceiling must be `created_at + 90 days` — the rule these sessions were
    // already living under, since the old `refresh_session` recomputed it every
    // call. Backfilling `expires_at` instead pins the ceiling at the current
    // expiry, and `min(slid_expiry, ceiling)` then freezes the session: refresh
    // can never move it and the user is logged out at the original expiry.
    let pool = pre_upgrade_pool().await;
    seed_admin(&pool, "admin-1", "operator", "2026-01-01T00:00:00+00:00").await;
    seed_session(
        &pool,
        "session-1",
        "admin-1",
        "2026-08-01T00:00:00+00:00",
        "2026-08-31T00:00:00+00:00",
        true,
    )
    .await;

    run_migration(&pool).await.unwrap();

    let (expires_at, absolute): (String, String) =
        sqlx::query_as("SELECT expires_at, absolute_expires_at FROM sessions")
            .fetch_one(&pool)
            .await
            .unwrap();

    assert!(
        absolute > expires_at,
        "the ceiling must sit beyond the current expiry, or refresh can never \
         move it (expires_at={expires_at}, absolute={absolute})"
    );
    // 2026-08-01 + 90 days.
    assert_eq!(absolute, "2026-10-30T00:00:00+00:00");
    // RFC 3339 shaped, because every comparison against it is a lexicographic
    // string compare against `chrono`'s `to_rfc3339()` output.
    assert!(
        absolute.contains('T') && absolute.ends_with("+00:00"),
        "the ceiling must be RFC 3339, got {absolute}"
    );
}

#[tokio::test]
async fn a_session_already_past_the_ceiling_is_not_shortened() {
    // `max(...)`: nobody gets logged out *by the upgrade itself*.
    let pool = pre_upgrade_pool().await;
    seed_admin(&pool, "admin-1", "operator", "2026-01-01T00:00:00+00:00").await;
    seed_session(
        &pool,
        "session-1",
        "admin-1",
        "2026-01-01T00:00:00+00:00",
        // Well beyond created_at + 90 days.
        "2027-06-01T00:00:00+00:00",
        true,
    )
    .await;

    run_migration(&pool).await.unwrap();

    let absolute: String = sqlx::query_scalar("SELECT absolute_expires_at FROM sessions")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(absolute, "2027-06-01T00:00:00+00:00");
}

#[tokio::test]
async fn the_nil_uuid_admin_is_excluded_and_its_sessions_dropped() {
    // A nil-UUID `users` row would make system-initiated actions
    // indistinguishable from a real person in audit logs.
    let pool = pre_upgrade_pool().await;
    seed_admin(&pool, "admin-1", "operator", "2026-01-01T00:00:00+00:00").await;
    seed_admin(
        &pool,
        "00000000-0000-0000-0000-000000000000",
        "system",
        "2026-01-01T00:00:00+00:00",
    )
    .await;
    seed_session(
        &pool,
        "session-nil",
        "00000000-0000-0000-0000-000000000000",
        "2026-08-01T00:00:00+00:00",
        "2026-08-31T00:00:00+00:00",
        false,
    )
    .await;

    run_migration(&pool).await.unwrap();

    let users: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(users, 1);
    let sessions: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sessions")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(sessions, 0, "a session with no users row must not survive");
}

#[tokio::test]
async fn admin_push_subscriptions_are_renamed_not_orphaned() {
    // `caller_owner()` derives the pair from the live `AuthContext`, which no
    // longer has an `Admin` variant. Leaving the old discriminator would make
    // every upgraded box's admin subscriptions present but unreachable.
    let pool = pre_upgrade_pool().await;
    seed_admin(&pool, "admin-1", "operator", "2026-01-01T00:00:00+00:00").await;
    sqlx::query(
        "INSERT INTO push_subscriptions \
         (id, owner_kind, owner_key, endpoint, p256dh, auth, created_at) \
         VALUES ('sub-1', 'admin', 'admin-1', 'https://push/a', 'pk', 'au', \
                 '2026-08-01T00:00:00+00:00')",
    )
    .execute(&pool)
    .await
    .unwrap();

    run_migration(&pool).await.unwrap();

    let owner_kind: String =
        sqlx::query_scalar("SELECT owner_kind FROM push_subscriptions WHERE id = 'sub-1'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(owner_kind, "user");
}

#[tokio::test]
async fn the_admins_table_is_gone_and_foreign_keys_hold() {
    let pool = pre_upgrade_pool().await;
    seed_admin(&pool, "admin-1", "operator", "2026-01-01T00:00:00+00:00").await;
    seed_session(
        &pool,
        "session-1",
        "admin-1",
        "2026-08-01T00:00:00+00:00",
        "2026-08-31T00:00:00+00:00",
        true,
    )
    .await;

    run_migration(&pool).await.unwrap();

    let admins_exist: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='admins')",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(!admins_exist, "`admins` must be dropped");

    // No dangling references after the rebuild.
    let violations: Vec<(String,)> = sqlx::query_as("PRAGMA foreign_key_check")
        .fetch_all(&pool)
        .await
        .unwrap_or_default();
    assert!(
        violations.is_empty(),
        "the rebuild left foreign-key violations: {violations:?}"
    );
}
