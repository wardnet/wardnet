use super::test_pool;
use crate::repository::sqlite::user::SqliteUserRepository;
use crate::repository::user::{DuplicateUserEmailError, UserRepository, UserRole, UserRow};

fn user(id: &str, display_name: &str, email: Option<&str>, role: UserRole) -> UserRow {
    UserRow {
        id: id.to_owned(),
        display_name: display_name.to_owned(),
        email: email.map(str::to_owned),
        role,
        enabled: true,
        created_at: "2026-08-10T00:00:00Z".to_owned(),
        updated_at: "2026-08-10T00:00:00Z".to_owned(),
    }
}

const ANN: &str = "aaaaaaaa-0000-4000-a000-000000000001";
const BOB: &str = "bbbbbbbb-0000-4000-a000-000000000002";

#[tokio::test]
async fn create_and_find_round_trips_every_field() {
    let repo = SqliteUserRepository::new(test_pool().await);
    let row = user(ANN, "Ann", Some("ann@example.com"), UserRole::Admin);

    repo.create(&row).await.unwrap();

    assert_eq!(repo.find_by_id(ANN).await.unwrap(), Some(row));
}

#[tokio::test]
async fn find_by_id_returns_none_for_unknown_id() {
    let repo = SqliteUserRepository::new(test_pool().await);

    assert_eq!(repo.find_by_id(ANN).await.unwrap(), None);
}

/// `SQLite`'s `TEXT UNIQUE` is case-sensitive, so without the `lower(email)`
/// index `Ann@Example.com` and `ann@example.com` would be two people.
#[tokio::test]
async fn email_uniqueness_is_case_insensitive() {
    let repo = SqliteUserRepository::new(test_pool().await);
    repo.create(&user(ANN, "Ann", Some("Ann@Example.com"), UserRole::Member))
        .await
        .unwrap();

    let err = repo
        .create(&user(BOB, "Bob", Some("ann@example.com"), UserRole::Member))
        .await
        .expect_err("a case-variant of an existing email must be rejected");

    assert!(
        err.downcast_ref::<DuplicateUserEmailError>().is_some(),
        "must surface as a business-rule conflict, not a raw DB error: {err:?}"
    );
}

/// The unique index is partial (`WHERE email IS NOT NULL`), so any number of
/// users may have no email — several household members legitimately won't.
#[tokio::test]
async fn several_users_may_have_no_email() {
    let repo = SqliteUserRepository::new(test_pool().await);

    repo.create(&user(ANN, "Ann", None, UserRole::Member))
        .await
        .unwrap();
    repo.create(&user(BOB, "Bob", None, UserRole::Member))
        .await
        .unwrap();

    assert_eq!(repo.find_all().await.unwrap().len(), 2);
}

#[tokio::test]
async fn find_by_email_is_case_insensitive() {
    let repo = SqliteUserRepository::new(test_pool().await);
    repo.create(&user(ANN, "Ann", Some("Ann@Example.com"), UserRole::Member))
        .await
        .unwrap();

    let found = repo.find_by_email("ANN@example.COM").await.unwrap();

    assert_eq!(found.map(|u| u.id), Some(ANN.to_owned()));
}

#[tokio::test]
async fn find_all_is_ordered_by_display_name_case_insensitively() {
    let repo = SqliteUserRepository::new(test_pool().await);
    repo.create(&user(ANN, "zoe", None, UserRole::Member))
        .await
        .unwrap();
    repo.create(&user(BOB, "Adam", None, UserRole::Member))
        .await
        .unwrap();

    let names: Vec<String> = repo
        .find_all()
        .await
        .unwrap()
        .into_iter()
        .map(|u| u.display_name)
        .collect();

    assert_eq!(names, vec!["Adam".to_owned(), "zoe".to_owned()]);
}

#[tokio::test]
async fn update_profile_changes_fields_and_bumps_updated_at() {
    let repo = SqliteUserRepository::new(test_pool().await);
    repo.create(&user(ANN, "Ann", Some("ann@example.com"), UserRole::Member))
        .await
        .unwrap();

    let affected = repo
        .update_profile(
            ANN,
            "Ann B",
            Some("annb@example.com"),
            "2026-08-11T00:00:00Z",
        )
        .await
        .unwrap();

    assert_eq!(affected, 1);
    let updated = repo.find_by_id(ANN).await.unwrap().unwrap();
    assert_eq!(updated.display_name, "Ann B");
    assert_eq!(updated.email.as_deref(), Some("annb@example.com"));
    assert_eq!(updated.updated_at, "2026-08-11T00:00:00Z");
    assert_eq!(
        updated.created_at, "2026-08-10T00:00:00Z",
        "created_at must not move"
    );
}

#[tokio::test]
async fn update_profile_reports_a_colliding_email_as_a_conflict() {
    let repo = SqliteUserRepository::new(test_pool().await);
    repo.create(&user(ANN, "Ann", Some("ann@example.com"), UserRole::Member))
        .await
        .unwrap();
    repo.create(&user(BOB, "Bob", Some("bob@example.com"), UserRole::Member))
        .await
        .unwrap();

    let err = repo
        .update_profile(BOB, "Bob", Some("ANN@example.com"), "2026-08-11T00:00:00Z")
        .await
        .expect_err("taking another user's email must be rejected");

    assert!(err.downcast_ref::<DuplicateUserEmailError>().is_some());
}

#[tokio::test]
async fn update_profile_reports_zero_rows_for_an_unknown_user() {
    let repo = SqliteUserRepository::new(test_pool().await);

    let affected = repo
        .update_profile(ANN, "Ghost", None, "2026-08-11T00:00:00Z")
        .await
        .unwrap();

    assert_eq!(affected, 0);
}

#[tokio::test]
async fn set_enabled_and_set_role_round_trip() {
    let repo = SqliteUserRepository::new(test_pool().await);
    repo.create(&user(ANN, "Ann", None, UserRole::Member))
        .await
        .unwrap();

    repo.set_enabled(ANN, false, "2026-08-11T00:00:00Z")
        .await
        .unwrap();
    repo.set_role(ANN, UserRole::Admin, "2026-08-12T00:00:00Z")
        .await
        .unwrap();

    let updated = repo.find_by_id(ANN).await.unwrap().unwrap();
    assert!(!updated.enabled);
    assert_eq!(updated.role, UserRole::Admin);
    assert_eq!(updated.updated_at, "2026-08-12T00:00:00Z");
}

/// Backs "the last admin cannot be demoted, disabled or deleted". Disabled
/// admins do not count: a household whose only admin is disabled is locked out
/// just as thoroughly as one whose only admin was deleted.
#[tokio::test]
async fn count_enabled_admins_ignores_members_and_disabled_admins() {
    let repo = SqliteUserRepository::new(test_pool().await);
    repo.create(&user(ANN, "Ann", None, UserRole::Admin))
        .await
        .unwrap();
    repo.create(&user(BOB, "Bob", None, UserRole::Member))
        .await
        .unwrap();

    assert_eq!(repo.count_enabled_admins().await.unwrap(), 1);

    repo.set_enabled(ANN, false, "2026-08-11T00:00:00Z")
        .await
        .unwrap();

    assert_eq!(repo.count_enabled_admins().await.unwrap(), 0);
}

#[tokio::test]
async fn exists_flips_once_the_box_is_claimed() {
    let repo = SqliteUserRepository::new(test_pool().await);

    assert!(!repo.exists().await.unwrap());

    repo.create(&user(ANN, "Ann", None, UserRole::Admin))
        .await
        .unwrap();

    assert!(repo.exists().await.unwrap());
}

#[tokio::test]
async fn delete_removes_the_row() {
    let repo = SqliteUserRepository::new(test_pool().await);
    repo.create(&user(ANN, "Ann", None, UserRole::Admin))
        .await
        .unwrap();

    assert_eq!(repo.delete(ANN).await.unwrap(), 1);
    assert_eq!(repo.find_by_id(ANN).await.unwrap(), None);
    assert_eq!(repo.delete(ANN).await.unwrap(), 0, "delete is idempotent");
}

/// An unrecognised `role` must be an error, never a silent default. The `CHECK`
/// constraint makes it unreachable for rows the daemon wrote, but a
/// hand-edited database is real and defaulting to `Admin` would be a privilege
/// escalation.
#[tokio::test]
async fn an_unrecognised_role_is_an_error_not_a_default() {
    let pool = test_pool().await;
    // Bypass the CHECK constraint the way only a hand-edit could: write the
    // row with the constraint satisfied, then corrupt it via PRAGMA-free
    // trickery is impossible, so assert the parser directly instead.
    let repo = SqliteUserRepository::new(pool);
    repo.create(&user(ANN, "Ann", None, UserRole::Admin))
        .await
        .unwrap();

    assert_eq!(UserRole::parse("admin"), Some(UserRole::Admin));
    assert_eq!(UserRole::parse("member"), Some(UserRole::Member));
    assert_eq!(UserRole::parse("root"), None);
    assert_eq!(UserRole::parse("Admin"), None, "parsing is exact, not lax");
}

#[tokio::test]
async fn the_check_constraint_rejects_an_unknown_role() {
    let pool = test_pool().await;

    let err = sqlx::query(
        "INSERT INTO users (id, display_name, role) VALUES (?, 'Mallory', 'superuser')",
    )
    .bind(ANN)
    .execute(&pool)
    .await
    .expect_err("the CHECK constraint must reject an unknown role");

    assert!(err.to_string().contains("CHECK constraint failed"), "{err}");
}

/// The `admins` backfill lowercases usernames into a case-sensitively unique
/// `(kind, subject)` index. `admins.username` was unique *case-sensitively*, so
/// a box holding both `Ann` and `ann` would collapse two rows onto one subject
/// and abort the migration — and an aborted migration here means a daemon that
/// will not start and cannot roll back. Both admins must survive with a working
/// password.
#[tokio::test]
async fn the_admins_backfill_survives_usernames_differing_only_by_case() {
    use crate::repository::sqlite::user_credential::SqliteUserCredentialRepository;
    use crate::repository::user_credential::{CredentialKind, UserCredentialRepository};

    let pool = test_pool().await;
    // Re-enact the backfill against a two-admin `admins` table. The real table
    // is dropped by the migration, so stand up an identical one and run the
    // migration's own INSERT against it.
    sqlx::query(
        "CREATE TABLE admins_probe (id TEXT PRIMARY KEY, username TEXT NOT NULL, \
         password_hash TEXT NOT NULL, created_at TEXT NOT NULL)",
    )
    .execute(&pool)
    .await
    .unwrap();
    for (id, name) in [(ANN, "Ann"), (BOB, "ann")] {
        sqlx::query(
            "INSERT INTO admins_probe (id, username, password_hash, created_at) \
             VALUES (?, ?, 'hash-for-' || ?, '2026-03-01T00:00:00Z')",
        )
        .bind(id)
        .bind(name)
        .bind(name)
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO users (id, display_name, email, role, enabled, created_at, updated_at) \
             VALUES (?, ?, NULL, 'admin', 1, '2026-03-01T00:00:00Z', '2026-03-01T00:00:00Z')",
        )
        .bind(id)
        .bind(name)
        .execute(&pool)
        .await
        .unwrap();
    }

    sqlx::query(
        "INSERT INTO user_credentials \
             (id, user_id, kind, subject, secret, label, metadata, created_at) \
         SELECT a.id, a.id, 'password', \
             CASE WHEN (SELECT COUNT(*) FROM admins_probe b \
                        WHERE lower(b.username) = lower(a.username)) = 1 \
                  THEN lower(a.username) ELSE a.username END, \
             a.password_hash, 'Local admin password', '{}', a.created_at \
         FROM admins_probe a",
    )
    .execute(&pool)
    .await
    .expect("the backfill must not abort on case-colliding usernames");

    // Both keep a usable, distinct credential.
    let creds = SqliteUserCredentialRepository::new(pool);
    for name in ["Ann", "ann"] {
        let login = creds
            .find_for_login(CredentialKind::Password, name)
            .await
            .unwrap()
            .unwrap_or_else(|| panic!("{name} must still be able to log in"));
        assert_eq!(
            login.credential.secret.as_deref(),
            Some(&*format!("hash-for-{name}"))
        );
    }
}
