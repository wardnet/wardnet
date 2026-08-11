use sqlx::SqlitePool;

use super::test_pool;
use crate::repository::session::SessionRepository;
use crate::repository::sqlite::session::SqliteSessionRepository;
use crate::repository::sqlite::user::SqliteUserRepository;
use crate::repository::user::{UserRepository, UserRole, UserRow};

const ANN: &str = "aaaaaaaa-0000-4000-a000-000000000001";
const BOB: &str = "bbbbbbbb-0000-4000-a000-000000000002";

/// Far enough out that it never bounds a test by accident — every test that
/// cares about the ceiling sets its own.
const FAR_FUTURE: &str = "2099-01-01T00:00:00Z";

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

async fn fixture() -> (SqlitePool, SqliteSessionRepository) {
    let pool = test_pool().await;
    seed_users(&pool).await;
    let repo = SqliteSessionRepository::new(pool.clone());
    (pool, repo)
}

/// Create a session with the common defaults, overriding only what a test
/// cares about.
async fn create_session(
    repo: &SqliteSessionRepository,
    id: &str,
    user_id: &str,
    token_hash: &str,
    expires_at: &str,
    absolute_expires_at: &str,
) {
    repo.create(
        id,
        user_id,
        token_hash,
        "2026-08-10T00:00:00Z",
        expires_at,
        false,
        None,
        None,
        absolute_expires_at,
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn a_live_session_resolves_to_its_principal_with_the_users_role() {
    let (_pool, repo) = fixture().await;
    create_session(
        &repo,
        "s1",
        ANN,
        "hash-1",
        "2026-09-01T00:00:00Z",
        FAR_FUTURE,
    )
    .await;

    let principal = repo
        .find_principal_by_token_hash("hash-1", "2026-08-11T00:00:00Z")
        .await
        .unwrap()
        .expect("a live session should resolve");

    assert_eq!(principal.user_id, ANN);
    assert_eq!(principal.role, UserRole::Admin);
    assert_eq!(principal.display_name, "Ann");
}

/// The role is read live from `users`, never cached in the session row, so a
/// demotion takes effect on the very next request rather than at next login.
#[tokio::test]
async fn a_demotion_takes_effect_on_the_next_request() {
    let (pool, repo) = fixture().await;
    create_session(
        &repo,
        "s1",
        ANN,
        "hash-1",
        "2026-09-01T00:00:00Z",
        FAR_FUTURE,
    )
    .await;

    SqliteUserRepository::new(pool)
        .set_role(ANN, UserRole::Member, "2026-08-11T00:00:00Z")
        .await
        .unwrap();

    let principal = repo
        .find_principal_by_token_hash("hash-1", "2026-08-11T00:00:00Z")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(principal.role, UserRole::Member);
}

#[tokio::test]
async fn an_expired_session_does_not_resolve() {
    let (_pool, repo) = fixture().await;
    create_session(
        &repo,
        "s1",
        ANN,
        "hash-1",
        "2026-08-01T00:00:00Z",
        FAR_FUTURE,
    )
    .await;

    assert!(
        repo.find_principal_by_token_hash("hash-1", "2026-08-11T00:00:00Z")
            .await
            .unwrap()
            .is_none()
    );
}

/// The absolute ceiling bounds a session independently of its sliding expiry:
/// a session refreshed forever must still die at the ceiling.
#[tokio::test]
async fn a_session_past_its_absolute_ceiling_does_not_resolve() {
    let (_pool, repo) = fixture().await;
    create_session(
        &repo,
        "s1",
        ANN,
        "hash-1",
        // Sliding expiry is still in the future...
        "2026-12-01T00:00:00Z",
        // ...but the ceiling has passed.
        "2026-08-05T00:00:00Z",
    )
    .await;

    assert!(
        repo.find_principal_by_token_hash("hash-1", "2026-08-11T00:00:00Z")
            .await
            .unwrap()
            .is_none()
    );
}

/// Disabling a user takes effect immediately, without hunting down their
/// sessions first.
#[tokio::test]
async fn a_disabled_users_session_does_not_resolve() {
    let (pool, repo) = fixture().await;
    create_session(
        &repo,
        "s1",
        ANN,
        "hash-1",
        "2026-09-01T00:00:00Z",
        FAR_FUTURE,
    )
    .await;

    SqliteUserRepository::new(pool)
        .set_enabled(ANN, false, "2026-08-11T00:00:00Z")
        .await
        .unwrap();

    assert!(
        repo.find_principal_by_token_hash("hash-1", "2026-08-11T00:00:00Z")
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn delete_expired_removes_only_expired_rows() {
    let (_pool, repo) = fixture().await;
    create_session(
        &repo,
        "s1",
        ANN,
        "hash-old",
        "2026-08-01T00:00:00Z",
        FAR_FUTURE,
    )
    .await;
    create_session(
        &repo,
        "s2",
        ANN,
        "hash-new",
        "2026-09-01T00:00:00Z",
        FAR_FUTURE,
    )
    .await;

    assert_eq!(
        repo.delete_expired("2026-08-11T00:00:00Z").await.unwrap(),
        1
    );
    assert!(
        repo.find_principal_by_token_hash("hash-new", "2026-08-11T00:00:00Z")
            .await
            .unwrap()
            .is_some()
    );
}

#[tokio::test]
async fn delete_by_token_hash_reports_whether_the_session_existed() {
    let (_pool, repo) = fixture().await;
    create_session(
        &repo,
        "s1",
        ANN,
        "hash-1",
        "2026-09-01T00:00:00Z",
        FAR_FUTURE,
    )
    .await;

    assert_eq!(repo.delete_by_token_hash("hash-1").await.unwrap(), 1);
    assert_eq!(repo.delete_by_token_hash("hash-1").await.unwrap(), 0);
}

/// Scoped to the owner, so a caller cannot revoke somebody else's session by
/// guessing an id.
#[tokio::test]
async fn delete_by_id_is_scoped_to_the_owning_user() {
    let (_pool, repo) = fixture().await;
    create_session(
        &repo,
        "s1",
        ANN,
        "hash-1",
        "2026-09-01T00:00:00Z",
        FAR_FUTURE,
    )
    .await;

    assert_eq!(repo.delete_by_id("s1", BOB).await.unwrap(), 0);
    assert_eq!(repo.delete_by_id("s1", ANN).await.unwrap(), 1);
}

#[tokio::test]
async fn delete_all_for_user_leaves_other_users_alone() {
    let (_pool, repo) = fixture().await;
    create_session(
        &repo,
        "s1",
        ANN,
        "hash-1",
        "2026-09-01T00:00:00Z",
        FAR_FUTURE,
    )
    .await;
    create_session(
        &repo,
        "s2",
        ANN,
        "hash-2",
        "2026-09-01T00:00:00Z",
        FAR_FUTURE,
    )
    .await;
    create_session(
        &repo,
        "s3",
        BOB,
        "hash-3",
        "2026-09-01T00:00:00Z",
        FAR_FUTURE,
    )
    .await;

    assert_eq!(repo.delete_all_for_user(ANN).await.unwrap(), 2);
    assert_eq!(
        repo.list_for_user(BOB, "2026-08-11T00:00:00Z")
            .await
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn list_for_user_shows_only_live_sessions_newest_first() {
    let (_pool, repo) = fixture().await;
    repo.create(
        "s-old",
        ANN,
        "hash-old",
        "2026-08-01T00:00:00Z",
        "2026-09-01T00:00:00Z",
        false,
        None,
        Some("curl/8"),
        FAR_FUTURE,
    )
    .await
    .unwrap();
    repo.create(
        "s-new",
        ANN,
        "hash-new",
        "2026-08-09T00:00:00Z",
        "2026-09-01T00:00:00Z",
        true,
        None,
        Some("Firefox"),
        FAR_FUTURE,
    )
    .await
    .unwrap();
    create_session(
        &repo,
        "s-dead",
        ANN,
        "hash-dead",
        "2026-08-02T00:00:00Z",
        FAR_FUTURE,
    )
    .await;

    let listed = repo
        .list_for_user(ANN, "2026-08-11T00:00:00Z")
        .await
        .unwrap();

    assert_eq!(listed.len(), 2, "the expired session must not be listed");
    assert_eq!(listed[0].id, "s-new");
    assert_eq!(listed[0].user_agent.as_deref(), Some("Firefox"));
    assert_eq!(listed[1].id, "s-old");
}

#[tokio::test]
async fn rotate_token_replaces_the_hash_and_slides_expiry() {
    let (_pool, repo) = fixture().await;
    create_session(
        &repo,
        "s1",
        ANN,
        "hash-old",
        "2026-08-20T00:00:00Z",
        FAR_FUTURE,
    )
    .await;

    repo.rotate_token("hash-old", "hash-new", "2026-09-20T00:00:00Z")
        .await
        .unwrap();

    assert!(
        repo.find_principal_by_token_hash("hash-old", "2026-08-11T00:00:00Z")
            .await
            .unwrap()
            .is_none(),
        "the old token must stop working immediately"
    );
    let refreshed = repo
        .find_session_for_refresh("hash-new", "2026-08-11T00:00:00Z")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(refreshed.user_id, ANN);
}

/// The ceiling is set once, at issue time. A refresh that could raise it would
/// not be a ceiling.
#[tokio::test]
async fn rotate_token_never_moves_the_absolute_ceiling() {
    let (_pool, repo) = fixture().await;
    create_session(
        &repo,
        "s1",
        ANN,
        "hash-old",
        "2026-08-20T00:00:00Z",
        "2026-09-01T00:00:00Z",
    )
    .await;

    repo.rotate_token("hash-old", "hash-new", "2027-01-01T00:00:00Z")
        .await
        .unwrap();

    let refreshed = repo
        .find_session_for_refresh("hash-new", "2026-08-11T00:00:00Z")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(refreshed.absolute_expires_at, "2026-09-01T00:00:00Z");
}

#[tokio::test]
async fn find_session_for_refresh_returns_everything_the_endpoint_needs() {
    let (_pool, repo) = fixture().await;
    repo.create(
        "s1",
        ANN,
        "hash-1",
        "2026-08-10T00:00:00Z",
        "2026-09-01T00:00:00Z",
        true,
        None,
        None,
        "2026-11-01T00:00:00Z",
    )
    .await
    .unwrap();

    let found = repo
        .find_session_for_refresh("hash-1", "2026-08-11T00:00:00Z")
        .await
        .unwrap()
        .unwrap();

    assert_eq!(found.user_id, ANN);
    assert!(found.remember_me);
    assert_eq!(found.created_at, "2026-08-10T00:00:00Z");
    assert_eq!(found.absolute_expires_at, "2026-11-01T00:00:00Z");
}

/// Deleting a person signs them out everywhere, by the FK rather than by a
/// service remembering to do it.
#[tokio::test]
async fn sessions_cascade_when_the_user_is_deleted() {
    let (pool, repo) = fixture().await;
    create_session(
        &repo,
        "s1",
        ANN,
        "hash-1",
        "2026-09-01T00:00:00Z",
        FAR_FUTURE,
    )
    .await;

    SqliteUserRepository::new(pool).delete(ANN).await.unwrap();

    assert!(
        repo.list_for_user(ANN, "2026-08-11T00:00:00Z")
            .await
            .unwrap()
            .is_empty()
    );
}

/// Refresh must apply the same liveness conditions as authentication.
/// Otherwise a disabled user's client keeps rotating itself a fresh token —
/// 200 on refresh, 403 on everything else — which is a revocation that does
/// not revoke.
#[tokio::test]
async fn a_disabled_user_cannot_refresh() {
    let (pool, repo) = fixture().await;
    create_session(
        &repo,
        "s1",
        ANN,
        "hash-1",
        "2026-09-01T00:00:00Z",
        FAR_FUTURE,
    )
    .await;

    SqliteUserRepository::new(pool)
        .set_enabled(ANN, false, "2026-08-11T00:00:00Z")
        .await
        .unwrap();

    assert_eq!(
        repo.find_session_for_refresh("hash-1", "2026-08-11T00:00:00Z")
            .await
            .unwrap(),
        None
    );
}

/// A session past its hard ceiling must not be refreshable — being able to
/// extend past the ceiling is exactly what a ceiling forbids.
#[tokio::test]
async fn a_session_past_its_ceiling_cannot_refresh() {
    let (_pool, repo) = fixture().await;
    create_session(
        &repo,
        "s1",
        ANN,
        "hash-1",
        // Sliding expiry still in the future...
        "2026-12-01T00:00:00Z",
        // ...ceiling passed.
        "2026-08-05T00:00:00Z",
    )
    .await;

    assert_eq!(
        repo.find_session_for_refresh("hash-1", "2026-08-11T00:00:00Z")
            .await
            .unwrap(),
        None
    );
}

/// A session that cannot authenticate must not be listed as live, or revoking
/// it looks like a no-op the user cannot distinguish from a bug.
#[tokio::test]
async fn list_for_user_hides_sessions_past_their_ceiling() {
    let (_pool, repo) = fixture().await;
    create_session(
        &repo,
        "s1",
        ANN,
        "hash-1",
        "2026-12-01T00:00:00Z",
        "2026-08-05T00:00:00Z",
    )
    .await;

    assert!(
        repo.list_for_user(ANN, "2026-08-11T00:00:00Z")
            .await
            .unwrap()
            .is_empty()
    );
}
