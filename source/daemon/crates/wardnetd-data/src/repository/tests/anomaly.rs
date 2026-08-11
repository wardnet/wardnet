use uuid::Uuid;
use wardnet_common::anomaly::{AnomalyFilter, AnomalyQueryStatus, AnomalyType};

use super::test_pool;
use crate::repository::anomaly::{ANOMALY_RETENTION_CAP, AnomalyRepository, NewAnomaly};
use crate::repository::sqlite::SqliteAnomalyRepository;

const T1: &str = "2026-08-01T10:00:00Z";
const T2: &str = "2026-08-01T11:00:00Z";
const T3: &str = "2026-08-01T12:00:00Z";

fn blocklist_anomaly<'a>(id: Uuid, subject: Option<&'a str>, at: &'a str) -> NewAnomaly<'a> {
    NewAnomaly {
        id,
        anomaly_type: AnomalyType::BlocklistRefreshFailing.as_str(),
        subject_id: subject,
        message: "list is failing",
        details: None,
        observed_at: at,
    }
}

async fn repo() -> SqliteAnomalyRepository {
    SqliteAnomalyRepository::new(test_pool().await)
}

#[tokio::test]
async fn open_returns_the_id_of_a_newly_opened_anomaly() {
    let repo = repo().await;
    let id = Uuid::new_v4();

    let opened = repo
        .open(blocklist_anomaly(id, Some("bl-1"), T1))
        .await
        .unwrap();

    assert_eq!(opened, Some(id));
    let stored = repo.find_by_id(id).await.unwrap().unwrap();
    assert_eq!(stored.anomaly_type, AnomalyType::BlocklistRefreshFailing);
    assert_eq!(stored.subject_id.as_deref(), Some("bl-1"));
    assert_eq!(stored.occurrences, 1);
    assert!(stored.is_open());
}

/// The whole edge-trigger rests on this: a second open for the same
/// (type, subject) must report "already open" rather than inserting, so the
/// caller knows not to notify again.
#[tokio::test]
async fn open_is_deduplicated_per_type_and_subject() {
    let repo = repo().await;
    repo.open(blocklist_anomaly(Uuid::new_v4(), Some("bl-1"), T1))
        .await
        .unwrap();

    let second = repo
        .open(blocklist_anomaly(Uuid::new_v4(), Some("bl-1"), T2))
        .await
        .unwrap();

    assert_eq!(second, None);
    assert_eq!(repo.list_open().await.unwrap().len(), 1);
}

#[tokio::test]
async fn open_dedup_is_scoped_to_the_subject() {
    let repo = repo().await;
    repo.open(blocklist_anomaly(Uuid::new_v4(), Some("bl-1"), T1))
        .await
        .unwrap();

    let other = repo
        .open(blocklist_anomaly(Uuid::new_v4(), Some("bl-2"), T1))
        .await
        .unwrap();

    assert!(
        other.is_some(),
        "a different subject is a different anomaly"
    );
    assert_eq!(repo.list_open().await.unwrap().len(), 2);
}

/// NULLs do not compare equal in a unique index, which is why the index keys
/// on `COALESCE(subject_id, '')`. Without that, every box-wide anomaly of a
/// type would open a fresh row on every observation.
#[tokio::test]
async fn open_deduplicates_box_wide_anomalies_with_no_subject() {
    let repo = repo().await;
    let first = repo
        .open(blocklist_anomaly(Uuid::new_v4(), None, T1))
        .await
        .unwrap();
    let second = repo
        .open(blocklist_anomaly(Uuid::new_v4(), None, T2))
        .await
        .unwrap();

    assert!(first.is_some());
    assert_eq!(second, None);
}

/// Resolved rows sit outside the partial index, so the same condition
/// recurring later is a genuinely new anomaly and alerts again.
#[tokio::test]
async fn the_same_condition_can_reopen_after_resolving() {
    let repo = repo().await;
    let first = repo
        .open(blocklist_anomaly(Uuid::new_v4(), Some("bl-1"), T1))
        .await
        .unwrap()
        .unwrap();
    repo.resolve(first, T2).await.unwrap();

    let second = repo
        .open(blocklist_anomaly(Uuid::new_v4(), Some("bl-1"), T3))
        .await
        .unwrap();

    assert!(second.is_some());
    assert_eq!(repo.list_open().await.unwrap().len(), 1);
}

#[tokio::test]
async fn touch_advances_last_seen_and_occurrences() {
    let repo = repo().await;
    let id = repo
        .open(blocklist_anomaly(Uuid::new_v4(), Some("bl-1"), T1))
        .await
        .unwrap()
        .unwrap();

    repo.touch(
        AnomalyType::BlocklistRefreshFailing.as_str(),
        Some("bl-1"),
        T2,
        "list is failing (7 times)",
        Some(r#"{"consecutive_failures":7}"#),
    )
    .await
    .unwrap();

    let stored = repo.find_by_id(id).await.unwrap().unwrap();
    assert_eq!(stored.occurrences, 2);
    assert_eq!(stored.message, "list is failing (7 times)");
    assert_eq!(stored.details.unwrap()["consecutive_failures"], 7);
    assert!(stored.last_seen_at > stored.opened_at);
}

#[tokio::test]
async fn touch_does_not_revive_a_resolved_anomaly() {
    let repo = repo().await;
    let id = repo
        .open(blocklist_anomaly(Uuid::new_v4(), Some("bl-1"), T1))
        .await
        .unwrap()
        .unwrap();
    repo.resolve(id, T2).await.unwrap();

    repo.touch(
        AnomalyType::BlocklistRefreshFailing.as_str(),
        Some("bl-1"),
        T3,
        "still failing",
        None,
    )
    .await
    .unwrap();

    let stored = repo.find_by_id(id).await.unwrap().unwrap();
    assert!(!stored.is_open());
    assert_eq!(stored.occurrences, 1);
    assert_eq!(stored.message, "list is failing");
}

/// `resolve` reporting `false` the second time is what stops a duplicate
/// "it is working again" notice.
#[tokio::test]
async fn resolve_reports_whether_it_changed_anything() {
    let repo = repo().await;
    let id = repo
        .open(blocklist_anomaly(Uuid::new_v4(), Some("bl-1"), T1))
        .await
        .unwrap()
        .unwrap();

    assert!(repo.resolve(id, T2).await.unwrap());
    assert!(!repo.resolve(id, T3).await.unwrap());

    let stored = repo.find_by_id(id).await.unwrap().unwrap();
    assert!(!stored.is_open());
}

#[tokio::test]
async fn resolve_of_an_unknown_id_is_false_not_an_error() {
    let repo = repo().await;
    assert!(!repo.resolve(Uuid::new_v4(), T1).await.unwrap());
}

#[tokio::test]
async fn notified_state_is_recorded_and_readable() {
    let repo = repo().await;
    let id = repo
        .open(blocklist_anomaly(Uuid::new_v4(), Some("bl-1"), T1))
        .await
        .unwrap()
        .unwrap();

    assert!(!repo.was_notified(id).await.unwrap());
    repo.mark_notified(id, T2).await.unwrap();
    assert!(repo.was_notified(id).await.unwrap());
}

#[tokio::test]
async fn was_notified_is_false_for_an_unknown_id() {
    let repo = repo().await;
    assert!(!repo.was_notified(Uuid::new_v4()).await.unwrap());
}

#[tokio::test]
async fn list_open_returns_oldest_first() {
    let repo = repo().await;
    let first = repo
        .open(blocklist_anomaly(Uuid::new_v4(), Some("bl-1"), T1))
        .await
        .unwrap()
        .unwrap();
    let second = repo
        .open(blocklist_anomaly(Uuid::new_v4(), Some("bl-2"), T2))
        .await
        .unwrap()
        .unwrap();

    let open = repo.list_open().await.unwrap();
    assert_eq!(open.len(), 2);
    assert_eq!(open[0].id, first);
    assert_eq!(open[1].id, second);
}

#[tokio::test]
async fn list_filters_by_status() {
    let repo = repo().await;
    let resolved = repo
        .open(blocklist_anomaly(Uuid::new_v4(), Some("bl-1"), T1))
        .await
        .unwrap()
        .unwrap();
    repo.resolve(resolved, T2).await.unwrap();
    let still_open = repo
        .open(blocklist_anomaly(Uuid::new_v4(), Some("bl-2"), T2))
        .await
        .unwrap()
        .unwrap();

    let open_only = repo
        .list(
            &AnomalyFilter {
                status: AnomalyQueryStatus::Open,
                subject_id: None,
            },
            50,
        )
        .await
        .unwrap();
    assert_eq!(open_only.len(), 1);
    assert_eq!(open_only[0].id, still_open);

    let resolved_only = repo
        .list(
            &AnomalyFilter {
                status: AnomalyQueryStatus::Resolved,
                subject_id: None,
            },
            50,
        )
        .await
        .unwrap();
    assert_eq!(resolved_only.len(), 1);
    assert_eq!(resolved_only[0].id, resolved);

    let all = repo
        .list(
            &AnomalyFilter {
                status: AnomalyQueryStatus::All,
                subject_id: None,
            },
            50,
        )
        .await
        .unwrap();
    assert_eq!(all.len(), 2);
}

/// This is how a tunnel card asks "what is currently wrong with me".
#[tokio::test]
async fn list_filters_by_subject() {
    let repo = repo().await;
    repo.open(blocklist_anomaly(Uuid::new_v4(), Some("bl-1"), T1))
        .await
        .unwrap();
    repo.open(blocklist_anomaly(Uuid::new_v4(), Some("bl-2"), T2))
        .await
        .unwrap();

    let mine = repo
        .list(
            &AnomalyFilter {
                status: AnomalyQueryStatus::Open,
                subject_id: Some("bl-2".to_owned()),
            },
            50,
        )
        .await
        .unwrap();

    assert_eq!(mine.len(), 1);
    assert_eq!(mine[0].subject_id.as_deref(), Some("bl-2"));
}

#[tokio::test]
async fn list_returns_newest_first_and_honours_the_limit() {
    let repo = repo().await;
    for (i, at) in [T1, T2, T3].iter().enumerate() {
        repo.open(blocklist_anomaly(
            Uuid::new_v4(),
            Some(&format!("bl-{i}")),
            at,
        ))
        .await
        .unwrap();
    }

    let page = repo
        .list(
            &AnomalyFilter {
                status: AnomalyQueryStatus::All,
                subject_id: None,
            },
            2,
        )
        .await
        .unwrap();

    assert_eq!(page.len(), 2);
    assert!(page[0].opened_at > page[1].opened_at);
}

/// Retention must never touch an open anomaly: dropping one would lose the
/// "already told them" state and silently re-arm the alert.
#[tokio::test]
async fn retention_prunes_resolved_rows_but_never_open_ones() {
    let repo = repo().await;
    let cap = ANOMALY_RETENTION_CAP as usize;

    // One long-lived open anomaly, then enough resolved ones to blow the cap.
    repo.open(blocklist_anomaly(Uuid::new_v4(), Some("keep-me"), T1))
        .await
        .unwrap();
    for i in 0..=cap {
        let subject = format!("bl-{i}");
        let id = repo
            .open(blocklist_anomaly(Uuid::new_v4(), Some(&subject), T2))
            .await
            .unwrap()
            .unwrap();
        repo.resolve(id, T3).await.unwrap();
    }

    // Pruning happens inside `open`, so it is eventually consistent: the last
    // `resolve` above lands after the final prune. One more open forces the
    // sweep that trims it back to the cap.
    repo.open(blocklist_anomaly(Uuid::new_v4(), Some("trigger"), T3))
        .await
        .unwrap();

    let open = repo.list_open().await.unwrap();
    assert_eq!(open.len(), 2, "open anomalies must survive pruning");
    assert_eq!(open[0].subject_id.as_deref(), Some("keep-me"));

    let resolved = repo
        .list(
            &AnomalyFilter {
                status: AnomalyQueryStatus::Resolved,
                subject_id: None,
            },
            u32::MAX,
        )
        .await
        .unwrap();
    assert_eq!(
        resolved.len(),
        cap,
        "resolved history is trimmed to exactly the cap"
    );
}

/// A row written by a newer daemon and read back after a downgrade must not
/// take the whole listing down with it.
#[tokio::test]
async fn rows_with_an_unknown_type_are_skipped_not_fatal() {
    let pool = test_pool().await;
    sqlx::query(
        "INSERT INTO anomalies (id, anomaly_type, message, opened_at, last_seen_at) \
         VALUES (?, 'solar_flare', 'from the future', ?, ?)",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(T1)
    .bind(T1)
    .execute(&pool)
    .await
    .unwrap();
    let repo = SqliteAnomalyRepository::new(pool);

    repo.open(blocklist_anomaly(Uuid::new_v4(), Some("bl-1"), T2))
        .await
        .unwrap();

    let open = repo.list_open().await.unwrap();
    assert_eq!(open.len(), 1);
    assert_eq!(open[0].anomaly_type, AnomalyType::BlocklistRefreshFailing);
}

#[tokio::test]
async fn find_by_id_returns_none_for_an_unknown_id() {
    let repo = repo().await;
    assert!(repo.find_by_id(Uuid::new_v4()).await.unwrap().is_none());
}
