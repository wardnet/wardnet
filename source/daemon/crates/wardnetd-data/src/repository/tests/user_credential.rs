use sqlx::SqlitePool;

use super::test_pool;
use crate::repository::sqlite::user::SqliteUserRepository;
use crate::repository::sqlite::user_credential::SqliteUserCredentialRepository;
use crate::repository::user::{UserRepository, UserRole, UserRow};
use crate::repository::user_credential::{
    CredentialAlreadyLinkedError, CredentialKind, CredentialRow, UserAlreadyHasPasswordError,
    UserCredentialRepository,
};

const ANN: &str = "aaaaaaaa-0000-4000-a000-000000000001";
const BOB: &str = "bbbbbbbb-0000-4000-a000-000000000002";

async fn seed_users(pool: &SqlitePool) {
    let users = SqliteUserRepository::new(pool.clone());
    for (id, name, role) in [
        (ANN, "Ann", UserRole::Admin),
        (BOB, "Bob", UserRole::Member),
    ] {
        users
            .create(&UserRow {
                id: id.to_owned(),
                display_name: name.to_owned(),
                email: None,
                role,
                enabled: true,
                created_at: "2026-08-10T00:00:00Z".to_owned(),
                updated_at: "2026-08-10T00:00:00Z".to_owned(),
            })
            .await
            .unwrap();
    }
}

fn credential(id: &str, user_id: &str, kind: CredentialKind, subject: &str) -> CredentialRow {
    CredentialRow {
        id: id.to_owned(),
        user_id: user_id.to_owned(),
        kind,
        subject: subject.to_owned(),
        secret: Some("secret-material".to_owned()),
        label: Some("a label".to_owned()),
        metadata: "{}".to_owned(),
        created_at: "2026-08-10T00:00:00Z".to_owned(),
        last_used_at: None,
    }
}

async fn fixture() -> (SqlitePool, SqliteUserCredentialRepository) {
    let pool = test_pool().await;
    seed_users(&pool).await;
    let repo = SqliteUserCredentialRepository::new(pool.clone());
    (pool, repo)
}

#[tokio::test]
async fn insert_and_find_for_login_round_trips() {
    let (_pool, repo) = fixture().await;
    let row = credential("c1", ANN, CredentialKind::Google, "google-sub-1");

    repo.insert(&row).await.unwrap();

    let login = repo
        .find_for_login(CredentialKind::Google, "google-sub-1")
        .await
        .unwrap()
        .expect("the credential should resolve");
    assert_eq!(login.credential, row);
    assert_eq!(login.role, UserRole::Admin);
    assert_eq!(login.display_name, "Ann");
}

/// The anti-hijack invariant (ADR-0031 §1): two users must never be able to
/// link the same provider account, or a login resolves to whichever row the
/// query happened to return.
#[tokio::test]
async fn the_same_google_account_cannot_be_linked_twice() {
    let (_pool, repo) = fixture().await;
    repo.insert(&credential(
        "c1",
        ANN,
        CredentialKind::Google,
        "google-sub-1",
    ))
    .await
    .unwrap();

    let err = repo
        .insert(&credential(
            "c2",
            BOB,
            CredentialKind::Google,
            "google-sub-1",
        ))
        .await
        .expect_err("a second link of the same provider account must be rejected");

    assert!(
        err.downcast_ref::<CredentialAlreadyLinkedError>().is_some(),
        "must be a business-rule conflict, not a raw DB error: {err:?}"
    );
}

/// `(kind, subject)` is scoped by kind, so a Google `sub` and a GitHub id that
/// happen to be the same string are unrelated credentials.
#[tokio::test]
async fn the_same_subject_under_a_different_kind_is_allowed() {
    let (_pool, repo) = fixture().await;
    repo.insert(&credential("c1", ANN, CredentialKind::Google, "12345"))
        .await
        .unwrap();

    repo.insert(&credential("c2", BOB, CredentialKind::Github, "12345"))
        .await
        .unwrap();
}

#[tokio::test]
async fn a_user_cannot_hold_two_passwords() {
    let (_pool, repo) = fixture().await;
    repo.insert(&credential("c1", ANN, CredentialKind::Password, "ann"))
        .await
        .unwrap();

    let err = repo
        .insert(&credential("c2", ANN, CredentialKind::Password, "ann-alt"))
        .await
        .expect_err("a second password is a parallel credential, not a lesser one");

    assert!(
        err.downcast_ref::<UserAlreadyHasPasswordError>().is_some(),
        "must be distinguishable from an already-linked conflict: {err:?}"
    );
}

/// A disabled user must be indistinguishable from a nonexistent one, so a
/// disabled account cannot be probed and no caller can forget the check.
#[tokio::test]
async fn find_for_login_hides_a_disabled_users_credential() {
    let (pool, repo) = fixture().await;
    repo.insert(&credential("c1", ANN, CredentialKind::Password, "ann"))
        .await
        .unwrap();
    SqliteUserRepository::new(pool)
        .set_enabled(ANN, false, "2026-08-11T00:00:00Z")
        .await
        .unwrap();

    let found = repo
        .find_for_login(CredentialKind::Password, "ann")
        .await
        .unwrap();

    assert!(found.is_none());
}

#[tokio::test]
async fn set_password_creates_then_replaces_in_one_statement() {
    let (_pool, repo) = fixture().await;

    repo.set_password("c1", ANN, "ann", "hash-one", "2026-08-10T00:00:00Z")
        .await
        .unwrap();
    repo.set_password("c2", ANN, "ann", "hash-two", "2026-08-11T00:00:00Z")
        .await
        .unwrap();

    let password = repo.find_password(ANN).await.unwrap().unwrap();
    assert_eq!(password.secret.as_deref(), Some("hash-two"));
    assert_eq!(
        repo.count_by_kind(ANN, CredentialKind::Password)
            .await
            .unwrap(),
        1,
        "the upsert must replace, never accumulate"
    );
}

/// A user changing their email changes their password credential's `subject`.
/// The upsert targets the partial `user_id` index for exactly this reason — a
/// `(kind, subject)` target would miss the row and trip the
/// one-password-per-user index as a hard error instead.
#[tokio::test]
async fn set_password_follows_a_changed_subject() {
    let (_pool, repo) = fixture().await;
    repo.set_password(
        "c1",
        ANN,
        "ann@old.example",
        "hash-one",
        "2026-08-10T00:00:00Z",
    )
    .await
    .unwrap();

    repo.set_password(
        "c2",
        ANN,
        "ann@new.example",
        "hash-two",
        "2026-08-11T00:00:00Z",
    )
    .await
    .unwrap();

    let password = repo.find_password(ANN).await.unwrap().unwrap();
    assert_eq!(password.subject, "ann@new.example");
    assert_eq!(password.secret.as_deref(), Some("hash-two"));
    assert!(
        repo.find_for_login(CredentialKind::Password, "ann@old.example")
            .await
            .unwrap()
            .is_none(),
        "the old subject must no longer authenticate"
    );
}

#[tokio::test]
async fn set_password_reports_a_subject_owned_by_another_user_as_a_conflict() {
    let (_pool, repo) = fixture().await;
    repo.set_password(
        "c1",
        ANN,
        "shared@example.com",
        "hash",
        "2026-08-10T00:00:00Z",
    )
    .await
    .unwrap();

    let err = repo
        .set_password(
            "c2",
            BOB,
            "shared@example.com",
            "hash",
            "2026-08-10T00:00:00Z",
        )
        .await
        .expect_err("two users cannot share a password subject");

    assert!(err.downcast_ref::<CredentialAlreadyLinkedError>().is_some());
}

/// The listing type has no `secret` field at all — that is the enforcement
/// mechanism, not a review comment.
#[tokio::test]
async fn list_for_user_is_newest_first_and_carries_no_secret() {
    let (_pool, repo) = fixture().await;
    let mut older = credential("c1", ANN, CredentialKind::Password, "ann");
    older.created_at = "2026-08-01T00:00:00Z".to_owned();
    let mut newer = credential("c2", ANN, CredentialKind::Passkey, "passkey-id");
    newer.created_at = "2026-08-09T00:00:00Z".to_owned();
    repo.insert(&older).await.unwrap();
    repo.insert(&newer).await.unwrap();

    let listed = repo.list_for_user(ANN).await.unwrap();

    assert_eq!(listed.len(), 2);
    assert_eq!(listed[0].id, "c2");
    assert_eq!(listed[1].id, "c1");
    assert_eq!(listed[0].kind, CredentialKind::Passkey);
}

#[tokio::test]
async fn touch_last_used_and_update_metadata_persist() {
    let (_pool, repo) = fixture().await;
    repo.insert(&credential(
        "c1",
        ANN,
        CredentialKind::Passkey,
        "passkey-id",
    ))
    .await
    .unwrap();

    repo.touch_last_used("c1", "2026-08-11T09:00:00Z")
        .await
        .unwrap();
    assert_eq!(
        repo.update_metadata("c1", r#"{"sign_count":7}"#)
            .await
            .unwrap(),
        1
    );

    let listed = repo.list_for_user(ANN).await.unwrap();
    assert_eq!(
        listed[0].last_used_at.as_deref(),
        Some("2026-08-11T09:00:00Z")
    );
    assert_eq!(listed[0].metadata, r#"{"sign_count":7}"#);
}

/// Deletion is scoped to the owner, so a caller cannot delete somebody else's
/// credential by guessing an id.
#[tokio::test]
async fn delete_is_scoped_to_the_owning_user() {
    let (_pool, repo) = fixture().await;
    repo.insert(&credential("c1", ANN, CredentialKind::Google, "sub"))
        .await
        .unwrap();

    assert_eq!(repo.delete("c1", BOB).await.unwrap(), 0);
    assert_eq!(repo.delete("c1", ANN).await.unwrap(), 1);
}

#[tokio::test]
async fn delete_by_kind_removes_only_that_kind() {
    let (_pool, repo) = fixture().await;
    repo.insert(&credential("c1", ANN, CredentialKind::Password, "ann"))
        .await
        .unwrap();
    repo.insert(&credential("c2", ANN, CredentialKind::Passkey, "pk-1"))
        .await
        .unwrap();
    repo.insert(&credential("c3", ANN, CredentialKind::Passkey, "pk-2"))
        .await
        .unwrap();

    assert_eq!(
        repo.delete_by_kind(ANN, CredentialKind::Passkey)
            .await
            .unwrap(),
        2
    );
    assert!(
        repo.find_password(ANN).await.unwrap().is_some(),
        "resetting passkeys must never remove the password floor"
    );
}

/// Deleting a person removes their credentials with them.
#[tokio::test]
async fn credentials_cascade_when_the_user_is_deleted() {
    let (pool, repo) = fixture().await;
    repo.insert(&credential("c1", ANN, CredentialKind::Password, "ann"))
        .await
        .unwrap();

    SqliteUserRepository::new(pool).delete(ANN).await.unwrap();

    assert!(repo.list_for_user(ANN).await.unwrap().is_empty());
}
