use super::test_pool;
use crate::repository::{SqliteSystemConfigRepository, SystemConfigRepository};
use wardnet_common::api::LastShutdownState;

#[tokio::test]
async fn get_seeded_value() {
    let pool = test_pool().await;
    let repo = SqliteSystemConfigRepository::new(pool);

    let version = repo.get("daemon_version").await.unwrap();
    assert_eq!(version, Some("0.1.0".to_owned()));
}

#[tokio::test]
async fn get_not_found() {
    let pool = test_pool().await;
    let repo = SqliteSystemConfigRepository::new(pool);

    let result = repo.get("nonexistent_key").await.unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn set_and_get() {
    let pool = test_pool().await;
    let repo = SqliteSystemConfigRepository::new(pool);

    repo.set("my_key", "my_value").await.unwrap();
    let result = repo.get("my_key").await.unwrap();
    assert_eq!(result, Some("my_value".to_owned()));
}

#[tokio::test]
async fn set_upsert_overwrites() {
    let pool = test_pool().await;
    let repo = SqliteSystemConfigRepository::new(pool);

    repo.set("key", "first").await.unwrap();
    repo.set("key", "second").await.unwrap();

    let result = repo.get("key").await.unwrap();
    assert_eq!(result, Some("second".to_owned()));
}

#[tokio::test]
async fn device_count_empty() {
    let pool = test_pool().await;
    let repo = SqliteSystemConfigRepository::new(pool);

    assert_eq!(repo.device_count().await.unwrap(), 0);
}

#[tokio::test]
async fn tunnel_count_empty() {
    let pool = test_pool().await;
    let repo = SqliteSystemConfigRepository::new(pool);

    assert_eq!(repo.tunnel_count().await.unwrap(), 0);
}

#[tokio::test]
async fn device_count_with_data() {
    let pool = test_pool().await;
    let now = "2026-03-07T00:00:00Z";
    for i in 0..3 {
        sqlx::query(
            "INSERT INTO devices (id, mac, last_ip, device_type, first_seen, last_seen, zone_id) \
             VALUES (?, ?, ?, 'unknown', ?, ?, '00000000-0000-0000-0000-000000000201')",
        )
        .bind(format!("00000000-0000-0000-0000-00000000000{i}"))
        .bind(format!("aa:bb:cc:dd:ee:{i:02x}"))
        .bind(format!("192.168.1.{}", 10 + i))
        .bind(now)
        .bind(now)
        .execute(&pool)
        .await
        .unwrap();
    }

    let repo = SqliteSystemConfigRepository::new(pool);
    assert_eq!(repo.device_count().await.unwrap(), 3);
}

#[tokio::test]
async fn is_setup_completed_returns_false_by_default() {
    let pool = test_pool().await;
    let repo = SqliteSystemConfigRepository::new(pool);

    // The migration seeds setup_completed = "false".
    assert!(!repo.is_setup_completed().await.unwrap());
}

#[tokio::test]
async fn set_setup_completed_updates_value() {
    let pool = test_pool().await;
    let repo = SqliteSystemConfigRepository::new(pool);

    repo.set_setup_completed(true).await.unwrap();
    assert!(repo.is_setup_completed().await.unwrap());
}

#[tokio::test]
async fn detect_first_ever_boot_returns_unknown() {
    let pool = test_pool().await;
    let repo = SqliteSystemConfigRepository::new(pool);

    let info = repo.detect_previous_shutdown().await.unwrap();
    assert_eq!(info.state, LastShutdownState::Unknown);
    assert!(info.at.is_none());
    assert!(info.acknowledged_at.is_none());

    // After detection, read_last_shutdown surfaces the persisted
    // classification of the prior run (still `Unknown`).
    let read = repo.read_last_shutdown().await.unwrap();
    assert_eq!(read.state, LastShutdownState::Unknown);
    assert!(read.at.is_none());
}

#[tokio::test]
async fn detect_after_graceful_returns_graceful() {
    let pool = test_pool().await;
    let repo = SqliteSystemConfigRepository::new(pool);

    // First boot — arms the running marker.
    repo.detect_previous_shutdown().await.unwrap();
    // Graceful exit recorded.
    repo.record_graceful_shutdown().await.unwrap();

    // Next boot — classifies the prior run.
    let info = repo.detect_previous_shutdown().await.unwrap();
    assert_eq!(info.state, LastShutdownState::Graceful);
    assert!(info.at.is_some());

    let read = repo.read_last_shutdown().await.unwrap();
    assert_eq!(read.state, LastShutdownState::Graceful);
    assert_eq!(read.at, info.at);
}

#[tokio::test]
async fn detect_after_running_marker_returns_unclean() {
    let pool = test_pool().await;
    let repo = SqliteSystemConfigRepository::new(pool);

    // First boot — arms running, never records graceful.
    repo.detect_previous_shutdown().await.unwrap();
    // Heartbeat fires while the daemon is alive.
    repo.record_heartbeat().await.unwrap();

    // Crash → next boot.
    let info = repo.detect_previous_shutdown().await.unwrap();
    assert_eq!(info.state, LastShutdownState::Unclean);
    // `at` resolves to the heartbeat timestamp.
    assert!(info.at.is_some());

    let read = repo.read_last_shutdown().await.unwrap();
    assert_eq!(read.state, LastShutdownState::Unclean);
    assert_eq!(read.at, info.at);
}

#[tokio::test]
async fn detect_unclean_falls_back_to_last_shutdown_at_when_heartbeat_missing() {
    let pool = test_pool().await;
    let repo = SqliteSystemConfigRepository::new(pool);

    // First boot — arms running. No heartbeat fires.
    repo.detect_previous_shutdown().await.unwrap();

    // Crash → next boot.
    let info = repo.detect_previous_shutdown().await.unwrap();
    assert_eq!(info.state, LastShutdownState::Unclean);
    // No heartbeat ever ran, so `at` falls back to the previous
    // run's `last_shutdown_at` (the boot time of the prior run).
    assert!(info.at.is_some());
}

#[tokio::test]
async fn detect_unclean_ignores_heartbeat_older_than_last_boot() {
    // Regression: a stale `last_seen_at` written by an *earlier* run
    // (before that run's graceful exit + a subsequent boot that
    // crashed before its first heartbeat) must not be returned as the
    // unclean event timestamp. The boot marker (`last_shutdown_at`)
    // is newer and is the correct fallback.
    let pool = test_pool().await;
    let repo = SqliteSystemConfigRepository::new(pool);

    // Boot 1 → heartbeat fires → graceful exit.
    repo.detect_previous_shutdown().await.unwrap();
    repo.record_heartbeat().await.unwrap();
    repo.record_graceful_shutdown().await.unwrap();

    // Sleep so the next boot's `last_shutdown_at` is strictly newer
    // than Boot 1's heartbeat.
    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;

    // Boot 2 → arms running. Crashes before its own heartbeat fires;
    // the stale `last_seen_at` is still Boot 1's value.
    let boot2 = repo.detect_previous_shutdown().await.unwrap();
    assert_eq!(boot2.state, LastShutdownState::Graceful);
    let boot2_at = boot2.at.expect("graceful at recorded");

    // Boot 3 → must classify Boot 2's crash with `at >= boot2_at`,
    // i.e. the boot marker, not the stale heartbeat from Boot 1.
    let boot3 = repo.detect_previous_shutdown().await.unwrap();
    assert_eq!(boot3.state, LastShutdownState::Unclean);
    let boot3_at = boot3.at.expect("unclean at recorded");
    assert!(
        boot3_at >= boot2_at,
        "unclean `at` ({boot3_at}) must not predate Boot 2's start ({boot2_at})",
    );
}

#[tokio::test]
async fn acknowledge_records_timestamp() {
    let pool = test_pool().await;
    let repo = SqliteSystemConfigRepository::new(pool);

    repo.detect_previous_shutdown().await.unwrap();
    repo.record_heartbeat().await.unwrap();
    repo.detect_previous_shutdown().await.unwrap();

    let before = repo.read_last_shutdown().await.unwrap();
    assert!(before.acknowledged_at.is_none());

    repo.acknowledge_last_shutdown().await.unwrap();

    let after = repo.read_last_shutdown().await.unwrap();
    let ack = after.acknowledged_at.expect("ack written");
    let at = after.at.expect("at present");
    // The banner predicate hides the banner when ack >= at.
    assert!(ack >= at);
}

#[tokio::test]
async fn new_unclean_event_makes_older_ack_stale() {
    let pool = test_pool().await;
    let repo = SqliteSystemConfigRepository::new(pool);

    // Boot 1 → unclean event 1.
    repo.detect_previous_shutdown().await.unwrap();
    repo.record_heartbeat().await.unwrap();
    repo.detect_previous_shutdown().await.unwrap();
    repo.acknowledge_last_shutdown().await.unwrap();
    let acked = repo.read_last_shutdown().await.unwrap();
    let ack_ts = acked.acknowledged_at.expect("ack present");
    let event1_at = acked.at.expect("event 1 at");
    assert!(ack_ts >= event1_at);

    // Sleep one second so timestamps (1s precision) advance.
    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;

    // Heartbeat continues in the new run.
    repo.record_heartbeat().await.unwrap();

    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;

    // Crash → unclean event 2 (no graceful in between).
    let event2 = repo.detect_previous_shutdown().await.unwrap();
    assert_eq!(event2.state, LastShutdownState::Unclean);

    let read = repo.read_last_shutdown().await.unwrap();
    let event2_at = read.at.expect("event 2 at");
    let still_acked = read.acknowledged_at.expect("ack persisted");
    // The old ack must now be older than the new event timestamp,
    // restoring the banner.
    assert!(still_acked < event2_at);
}

#[tokio::test]
async fn detect_arms_running_marker_for_next_boot() {
    let pool = test_pool().await;
    let repo = SqliteSystemConfigRepository::new(pool);

    repo.detect_previous_shutdown().await.unwrap();

    let state = repo.get("last_shutdown_state").await.unwrap();
    assert_eq!(state.as_deref(), Some("running"));
    let at = repo.get("last_shutdown_at").await.unwrap();
    assert!(at.is_some());
}

#[tokio::test]
async fn record_heartbeat_writes_last_seen_at() {
    let pool = test_pool().await;
    let repo = SqliteSystemConfigRepository::new(pool);

    assert!(repo.get("last_seen_at").await.unwrap().is_none());

    repo.record_heartbeat().await.unwrap();

    let seen = repo.get("last_seen_at").await.unwrap();
    assert!(seen.is_some());
}

#[tokio::test]
async fn is_setup_completed_returns_true_after_set() {
    let pool = test_pool().await;
    let repo = SqliteSystemConfigRepository::new(pool);

    assert!(!repo.is_setup_completed().await.unwrap());
    repo.set_setup_completed(true).await.unwrap();
    assert!(repo.is_setup_completed().await.unwrap());
}

#[tokio::test]
async fn default_policy_round_trip() {
    let pool = test_pool().await;
    let repo = SqliteSystemConfigRepository::new(pool);

    assert!(repo.get_default_policy().await.unwrap().is_none());

    repo.set_default_policy("direct").await.unwrap();
    assert_eq!(
        repo.get_default_policy().await.unwrap().as_deref(),
        Some("direct")
    );

    let tunnel_uuid = "10000000-0000-0000-0000-000000000001";
    repo.set_default_policy(tunnel_uuid).await.unwrap();
    assert_eq!(
        repo.get_default_policy().await.unwrap().as_deref(),
        Some(tunnel_uuid)
    );
}

#[tokio::test]
async fn wizard_step_round_trip() {
    let pool = test_pool().await;
    let repo = SqliteSystemConfigRepository::new(pool);

    assert!(repo.get_wizard_step().await.unwrap().is_none());

    repo.set_wizard_step("network").await.unwrap();
    assert_eq!(
        repo.get_wizard_step().await.unwrap().as_deref(),
        Some("network")
    );

    repo.set_wizard_step("completed").await.unwrap();
    assert_eq!(
        repo.get_wizard_step().await.unwrap().as_deref(),
        Some("completed")
    );
}

#[tokio::test]
async fn wizard_mode_round_trip() {
    let pool = test_pool().await;
    let repo = SqliteSystemConfigRepository::new(pool);

    assert!(repo.get_wizard_mode().await.unwrap().is_none());

    repo.set_wizard_mode("primary").await.unwrap();
    assert_eq!(
        repo.get_wizard_mode().await.unwrap().as_deref(),
        Some("primary")
    );
}

#[tokio::test]
async fn router_mac_round_trip() {
    let pool = test_pool().await;
    let repo = SqliteSystemConfigRepository::new(pool);

    assert!(repo.get_router_mac().await.unwrap().is_none());

    repo.set_router_mac("aa:bb:cc:dd:ee:ff").await.unwrap();
    assert_eq!(
        repo.get_router_mac().await.unwrap().as_deref(),
        Some("aa:bb:cc:dd:ee:ff")
    );
}
