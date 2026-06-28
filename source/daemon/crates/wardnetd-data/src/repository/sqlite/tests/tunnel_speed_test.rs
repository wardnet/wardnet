use sqlx::SqlitePool;
use sqlx::sqlite::SqlitePoolOptions;
use uuid::Uuid;

use crate::repository::TunnelSpeedTestRepository;
use crate::repository::sqlite::tunnel_speed_test::SqliteTunnelSpeedTestRepository;
use crate::repository::tunnel_speed_test::SpeedTestRow;

/// In-memory pool with the full schema applied. A single connection is used so
/// the in-memory database persists across the repository's read/write pools.
async fn make_pool() -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("open in-memory db");
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("run migrations");
    pool
}

/// Insert a minimal parent tunnel so the speed-test FK is satisfied. The
/// interface name is derived from the id (the column is UNIQUE).
async fn seed_tunnel(pool: &SqlitePool, id: &str) {
    sqlx::query(
        "INSERT INTO tunnels (id, label, country_code, interface_name, endpoint) \
         VALUES (?, 'test', 'NL', ?, '198.51.100.1:51820')",
    )
    .bind(id)
    .bind(format!("wg_{}", &id[..8]))
    .execute(pool)
    .await
    .expect("seed tunnel");
}

fn row(tunnel_id: &str, tested_at: &str, tunnel_mbps: f64) -> SpeedTestRow {
    SpeedTestRow {
        id: Uuid::new_v4().to_string(),
        tunnel_id: tunnel_id.to_owned(),
        direct_throughput_mbps: 94.0,
        tunnel_throughput_mbps: tunnel_mbps,
        direct_latency_ms: 11.0,
        tunnel_latency_ms: 23.0,
        direct_jitter_ms: 2.0,
        tunnel_jitter_ms: 4.0,
        tested_at: tested_at.to_owned(),
    }
}

#[tokio::test]
async fn insert_round_trips_all_fields() {
    let pool = make_pool().await;
    let tunnel = Uuid::new_v4().to_string();
    seed_tunnel(&pool, &tunnel).await;
    let repo = SqliteTunnelSpeedTestRepository::new(pool);
    repo.insert(&row(&tunnel, "2026-06-28T10:00:00Z", 85.0))
        .await
        .unwrap();

    let results = repo.find_recent(&tunnel, 5).await.unwrap();
    assert_eq!(results.len(), 1);
    let r = &results[0];
    assert_eq!(r.tunnel_id.to_string(), tunnel);
    assert!((r.direct_throughput_mbps - 94.0).abs() < f64::EPSILON);
    assert!((r.tunnel_throughput_mbps - 85.0).abs() < f64::EPSILON);
    assert!((r.direct_latency_ms - 11.0).abs() < f64::EPSILON);
    assert!((r.tunnel_latency_ms - 23.0).abs() < f64::EPSILON);
    assert!((r.direct_jitter_ms - 2.0).abs() < f64::EPSILON);
    assert!((r.tunnel_jitter_ms - 4.0).abs() < f64::EPSILON);
}

#[tokio::test]
async fn find_recent_orders_newest_first_and_applies_limit() {
    let pool = make_pool().await;
    let tunnel = Uuid::new_v4().to_string();
    seed_tunnel(&pool, &tunnel).await;
    let repo = SqliteTunnelSpeedTestRepository::new(pool);
    // Six runs at increasing timestamps; tunnel throughput encodes the order.
    for i in 0..6 {
        repo.insert(&row(
            &tunnel,
            &format!("2026-06-28T10:0{i}:00Z"),
            80.0 + f64::from(i),
        ))
        .await
        .unwrap();
    }

    let results = repo.find_recent(&tunnel, 5).await.unwrap();
    assert_eq!(results.len(), 5, "limit of 5 is applied");
    // Newest first: the i=5 row (85.0) leads, the i=0 row (80.0) is dropped.
    assert!((results[0].tunnel_throughput_mbps - 85.0).abs() < f64::EPSILON);
    for pair in results.windows(2) {
        assert!(
            pair[0].tested_at >= pair[1].tested_at,
            "results must be ordered newest-first",
        );
    }
}

#[tokio::test]
async fn find_recent_filters_by_tunnel() {
    let pool = make_pool().await;
    let a = Uuid::new_v4().to_string();
    let b = Uuid::new_v4().to_string();
    seed_tunnel(&pool, &a).await;
    seed_tunnel(&pool, &b).await;
    let repo = SqliteTunnelSpeedTestRepository::new(pool);
    repo.insert(&row(&a, "2026-06-28T10:00:00Z", 85.0))
        .await
        .unwrap();
    repo.insert(&row(&b, "2026-06-28T10:01:00Z", 70.0))
        .await
        .unwrap();

    let results = repo.find_recent(&a, 5).await.unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].tunnel_id.to_string(), a);
}

#[tokio::test]
async fn find_recent_empty_for_unknown_tunnel() {
    let repo = SqliteTunnelSpeedTestRepository::new(make_pool().await);
    let results = repo
        .find_recent(&Uuid::new_v4().to_string(), 5)
        .await
        .unwrap();
    assert!(results.is_empty());
}
