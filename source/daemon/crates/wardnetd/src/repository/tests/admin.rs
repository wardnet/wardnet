use super::test_pool;
use crate::repository::{AdminRepository, SqliteAdminRepository};

#[tokio::test]
async fn create_and_find_by_username() {
    let pool = test_pool().await;
    let repo = SqliteAdminRepository::new(pool);

    repo.create("id-1", "alice", "hash123").await.unwrap();

    let result = repo.find_by_username("alice").await.unwrap();
    assert_eq!(result, Some(("id-1".to_owned(), "hash123".to_owned())));
}

#[tokio::test]
async fn find_by_username_not_found() {
    let pool = test_pool().await;
    let repo = SqliteAdminRepository::new(pool);

    let result = repo.find_by_username("nobody").await.unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn find_first_id_returns_an_admin() {
    let pool = test_pool().await;
    let repo = SqliteAdminRepository::new(pool);

    repo.create("id-a", "alice", "h1").await.unwrap();
    repo.create("id-b", "bob", "h2").await.unwrap();

    let first = repo.find_first_id().await.unwrap();
    assert!(first.is_some());
}

#[tokio::test]
async fn find_first_id_empty() {
    let pool = test_pool().await;
    let repo = SqliteAdminRepository::new(pool);

    let result = repo.find_first_id().await.unwrap();
    assert!(result.is_none());
}
