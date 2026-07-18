//! Tests for the SQLite-backed update-history repository.

use wardnet_common::update::{UpdateHistoryEntry, UpdateHistoryStatus};

use super::test_pool;
use crate::repository::SqliteUpdateRepository;
use crate::repository::update::{UpdateHistoryRow, UpdateRepository};

fn row(from: &str, to: &str, status: UpdateHistoryStatus) -> UpdateHistoryRow {
    UpdateHistoryRow {
        from_version: from.to_owned(),
        to_version: to.to_owned(),
        phase: "download".to_owned(),
        status,
        error: None,
    }
}

#[tokio::test]
async fn insert_and_list_round_trip() {
    let repo = SqliteUpdateRepository::new(test_pool().await);
    let id = repo
        .insert(&row(
            "2026.06.00",
            "2026.07.00",
            UpdateHistoryStatus::Started,
        ))
        .await
        .unwrap();

    let entries = repo.list(10).await.unwrap();
    assert_eq!(entries.len(), 1);
    let entry = &entries[0];
    assert_eq!(entry.id, id);
    assert_eq!(entry.from_version, "2026.06.00");
    assert_eq!(entry.to_version, "2026.07.00");
    assert_eq!(entry.phase, "download");
    assert_eq!(entry.status, UpdateHistoryStatus::Started);
    assert_eq!(entry.error, None);
    assert_eq!(entry.finished_at, None, "not finalized yet");
}

#[tokio::test]
async fn finalize_updates_status_phase_error_and_finished_at() {
    let repo = SqliteUpdateRepository::new(test_pool().await);
    let id = repo
        .insert(&row(
            "2026.06.00",
            "2026.07.00",
            UpdateHistoryStatus::Started,
        ))
        .await
        .unwrap();

    repo.finalize(id, UpdateHistoryStatus::Failed, "swap", Some("boom"))
        .await
        .unwrap();

    let entry = repo.list(10).await.unwrap().remove(0);
    assert_eq!(entry.status, UpdateHistoryStatus::Failed);
    assert_eq!(entry.phase, "swap");
    assert_eq!(entry.error.as_deref(), Some("boom"));
    assert!(entry.finished_at.is_some(), "finalize stamps finished_at");
}

#[tokio::test]
async fn list_respects_limit() {
    let repo = SqliteUpdateRepository::new(test_pool().await);
    for i in 0..3 {
        repo.insert(&row(
            "2026.06.00",
            &format!("2026.07.0{i}"),
            UpdateHistoryStatus::Started,
        ))
        .await
        .unwrap();
    }
    assert_eq!(repo.list(2).await.unwrap().len(), 2);
    assert_eq!(repo.list(10).await.unwrap().len(), 3);
}

#[tokio::test]
async fn last_succeeded_ignores_failures_and_rollbacks() {
    let repo = SqliteUpdateRepository::new(test_pool().await);
    assert!(
        repo.last_succeeded().await.unwrap().is_none(),
        "empty table"
    );

    let failed = repo
        .insert(&row(
            "2026.05.00",
            "2026.06.00",
            UpdateHistoryStatus::Started,
        ))
        .await
        .unwrap();
    repo.finalize(failed, UpdateHistoryStatus::Failed, "swap", Some("boom"))
        .await
        .unwrap();
    assert!(
        repo.last_succeeded().await.unwrap().is_none(),
        "failures don't count"
    );

    let ok = repo
        .insert(&row(
            "2026.05.00",
            "2026.06.00",
            UpdateHistoryStatus::Started,
        ))
        .await
        .unwrap();
    repo.finalize(ok, UpdateHistoryStatus::Succeeded, "restart", None)
        .await
        .unwrap();

    let entry: UpdateHistoryEntry = repo.last_succeeded().await.unwrap().expect("one success");
    assert_eq!(entry.id, ok);
    assert_eq!(entry.status, UpdateHistoryStatus::Succeeded);
}

#[tokio::test]
async fn empty_finished_at_is_treated_as_unfinished() {
    let pool = test_pool().await;
    sqlx::query(
        "INSERT INTO update_history (from_version, to_version, phase, status, finished_at) \
         VALUES ('a', 'b', 'download', 'started', '')",
    )
    .execute(&pool)
    .await
    .unwrap();

    let repo = SqliteUpdateRepository::new(pool);
    let entry = repo.list(10).await.unwrap().remove(0);
    assert_eq!(entry.finished_at, None);
}
