use super::test_pool;
use crate::repository::notification::NOTIFICATION_RETENTION_CAP;
use crate::repository::{NewNotification, NotificationRepository, SqliteNotificationRepository};

fn notif<'a>(id: &'a str, kind: &'a str, created_at: &'a str) -> NewNotification<'a> {
    NewNotification {
        id,
        kind,
        title: "Title",
        body: "Body",
        url: Some("/devices"),
        subject_id: Some("subject-1"),
        created_at,
    }
}

#[tokio::test]
async fn list_recent_returns_newest_first_with_limit() {
    let pool = test_pool().await;
    let repo = SqliteNotificationRepository::new(pool);

    repo.insert(notif("n1", "tunnel_offline", "2026-07-01T00:00:00Z"))
        .await
        .unwrap();
    repo.insert(notif(
        "n2",
        "new_device_quarantined",
        "2026-07-02T00:00:00Z",
    ))
    .await
    .unwrap();
    repo.insert(notif("n3", "routing_changed", "2026-07-03T00:00:00Z"))
        .await
        .unwrap();

    let recent = repo.list_recent(2).await.unwrap();
    assert_eq!(recent.len(), 2);
    assert_eq!(recent[0].id, "n3");
    assert_eq!(recent[1].id, "n2");
    assert_eq!(recent[0].kind, "routing_changed");
    assert_eq!(recent[0].url.as_deref(), Some("/devices"));
    assert_eq!(recent[0].subject_id.as_deref(), Some("subject-1"));
}

#[tokio::test]
async fn optional_fields_round_trip_as_none() {
    let pool = test_pool().await;
    let repo = SqliteNotificationRepository::new(pool);

    repo.insert(NewNotification {
        id: "n1",
        kind: "routing_locked",
        title: "Routing locked",
        body: "Body",
        url: None,
        subject_id: None,
        created_at: "2026-07-01T00:00:00Z",
    })
    .await
    .unwrap();

    let recent = repo.list_recent(10).await.unwrap();
    assert_eq!(recent.len(), 1);
    assert_eq!(recent[0].url, None);
    assert_eq!(recent[0].subject_id, None);
}

#[tokio::test]
async fn insert_prunes_oldest_rows_beyond_the_retention_cap() {
    let pool = test_pool().await;
    let repo = SqliteNotificationRepository::new(pool);

    // Insert one row more than the cap; ids/timestamps increase monotonically.
    for i in 0..=NOTIFICATION_RETENTION_CAP {
        let id = format!("n{i:03}");
        let created_at = format!("2026-07-01T00:00:{:02}.{:03}Z", i / 1000, i % 1000);
        repo.insert(notif(&id, "tunnel_offline", &created_at))
            .await
            .unwrap();
    }

    let recent = repo
        .list_recent(NOTIFICATION_RETENTION_CAP + 10)
        .await
        .unwrap();
    assert_eq!(recent.len(), NOTIFICATION_RETENTION_CAP as usize);
    // The oldest row was pruned; the newest survives.
    assert_eq!(recent[0].id, format!("n{NOTIFICATION_RETENTION_CAP:03}"));
    assert!(recent.iter().all(|n| n.id != "n000"));
}

#[tokio::test]
async fn clear_removes_everything_and_reports_the_count() {
    let pool = test_pool().await;
    let repo = SqliteNotificationRepository::new(pool);

    repo.insert(notif("n1", "tunnel_offline", "2026-07-01T00:00:00Z"))
        .await
        .unwrap();
    repo.insert(notif("n2", "tunnel_offline", "2026-07-02T00:00:00Z"))
        .await
        .unwrap();

    assert_eq!(repo.clear().await.unwrap(), 2);
    assert!(repo.list_recent(10).await.unwrap().is_empty());
    assert_eq!(repo.clear().await.unwrap(), 0);
}
