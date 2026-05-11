use super::test_pool;
use crate::repository::tunnel::TunnelRow;
use crate::repository::tunnel_metrics::{DailyMetricRow, IntradayMetricRow};
use crate::repository::{
    SqliteTunnelMetricsRepository, SqliteTunnelRepository, TunnelMetricsRepository,
    TunnelRepository,
};

const TUNNEL_A: &str = "00000000-0000-0000-0000-000000000001";
const TUNNEL_B: &str = "00000000-0000-0000-0000-000000000002";

fn sample_tunnel(id: &str, interface_name: &str) -> TunnelRow {
    TunnelRow {
        id: id.to_owned(),
        label: "Test".to_owned(),
        country_code: "SE".to_owned(),
        provider: Some("Mullvad".to_owned()),
        interface_name: interface_name.to_owned(),
        endpoint: "198.51.100.1:51820".to_owned(),
        status: "up".to_owned(),
        address: "[\"10.66.0.2/32\"]".to_owned(),
        dns: "[\"1.1.1.1\"]".to_owned(),
        peer_config: "{\"public_key\":\"abc\",\"endpoint\":\"198.51.100.1:51820\",\"allowed_ips\":[\"0.0.0.0/0\"],\"preshared_key\":null,\"persistent_keepalive\":25}".to_owned(),
        listen_port: None,
        override_default_dns: false,
    }
}

async fn seed_tunnel(pool: &sqlx::SqlitePool, id: &str, iface: &str) {
    let repo = SqliteTunnelRepository::new(pool.clone());
    repo.insert(&sample_tunnel(id, iface)).await.unwrap();
}

fn day_ts(day: &str, hour: u32) -> i64 {
    chrono::NaiveDate::parse_from_str(day, "%Y-%m-%d")
        .unwrap()
        .and_hms_opt(hour, 0, 0)
        .unwrap()
        .and_utc()
        .timestamp()
}

#[tokio::test]
async fn insert_and_query_intraday_window() {
    let pool = test_pool().await;
    seed_tunnel(&pool, TUNNEL_A, "wg_ward0").await;
    let repo = SqliteTunnelMetricsRepository::new(pool);

    let base = day_ts("2026-01-15", 10);
    for i in 0..6 {
        repo.insert_intraday(&IntradayMetricRow {
            tunnel_id: TUNNEL_A.to_owned(),
            ts: base + i64::from(i) * 300,
            bytes_tx_delta: i64::from(i) * 100,
            bytes_rx_delta: i64::from(i) * 200,
        })
        .await
        .unwrap();
    }

    let rows = repo
        .query_intraday(TUNNEL_A, base + 600, base + 1500)
        .await
        .unwrap();
    assert_eq!(rows.len(), 4);
    assert_eq!(rows[0].ts, base + 600);
    assert_eq!(rows[3].ts, base + 1500);
}

#[tokio::test]
async fn rollup_day_is_idempotent() {
    let pool = test_pool().await;
    seed_tunnel(&pool, TUNNEL_A, "wg_ward0").await;
    seed_tunnel(&pool, TUNNEL_B, "wg_ward1").await;
    let repo = SqliteTunnelMetricsRepository::new(pool);

    let base = day_ts("2026-01-15", 0);
    let rows = vec![
        IntradayMetricRow {
            tunnel_id: TUNNEL_A.to_owned(),
            ts: base + 300,
            bytes_tx_delta: 100,
            bytes_rx_delta: 200,
        },
        IntradayMetricRow {
            tunnel_id: TUNNEL_A.to_owned(),
            ts: base + 600,
            bytes_tx_delta: 50,
            bytes_rx_delta: 75,
        },
        IntradayMetricRow {
            tunnel_id: TUNNEL_B.to_owned(),
            ts: base + 300,
            bytes_tx_delta: 1000,
            bytes_rx_delta: 2000,
        },
    ];
    repo.insert_intraday_batch(&rows).await.unwrap();

    let pending = repo.days_pending_rollup("2026-01-16").await.unwrap();
    assert_eq!(pending, vec!["2026-01-15".to_owned()]);

    let inserted = repo.rollup_day("2026-01-15").await.unwrap();
    assert_eq!(inserted, 2);

    // Re-running rollup is a no-op (the daily rows already exist).
    let inserted_again = repo.rollup_day("2026-01-15").await.unwrap();
    assert_eq!(inserted_again, 0);

    // Daily rows reflect the SUM of intraday deltas.
    let daily_a = repo
        .query_daily(TUNNEL_A, "2026-01-15", "2026-01-15")
        .await
        .unwrap();
    assert_eq!(daily_a.len(), 1);
    assert_eq!(daily_a[0].bytes_tx_total, 150);
    assert_eq!(daily_a[0].bytes_rx_total, 275);

    // After rollup, days_pending_rollup excludes the rolled-up day.
    let pending = repo.days_pending_rollup("2026-01-16").await.unwrap();
    assert!(pending.is_empty());
}

#[tokio::test]
async fn trim_intraday_respects_boundary() {
    let pool = test_pool().await;
    seed_tunnel(&pool, TUNNEL_A, "wg_ward0").await;
    let repo = SqliteTunnelMetricsRepository::new(pool);

    let cutoff = 1_700_000_000_i64;
    repo.insert_intraday(&IntradayMetricRow {
        tunnel_id: TUNNEL_A.to_owned(),
        ts: cutoff - 1,
        bytes_tx_delta: 1,
        bytes_rx_delta: 1,
    })
    .await
    .unwrap();
    repo.insert_intraday(&IntradayMetricRow {
        tunnel_id: TUNNEL_A.to_owned(),
        ts: cutoff,
        bytes_tx_delta: 2,
        bytes_rx_delta: 2,
    })
    .await
    .unwrap();
    repo.insert_intraday(&IntradayMetricRow {
        tunnel_id: TUNNEL_A.to_owned(),
        ts: cutoff + 1,
        bytes_tx_delta: 3,
        bytes_rx_delta: 3,
    })
    .await
    .unwrap();

    let deleted = repo.trim_intraday(cutoff).await.unwrap();
    assert_eq!(deleted, 1);

    let surviving = repo.query_intraday(TUNNEL_A, 0, i64::MAX).await.unwrap();
    assert_eq!(surviving.len(), 2);
    assert!(surviving.iter().all(|r| r.ts >= cutoff));
}

#[tokio::test]
async fn trim_daily_respects_boundary() {
    let pool = test_pool().await;
    seed_tunnel(&pool, TUNNEL_A, "wg_ward0").await;
    let repo = SqliteTunnelMetricsRepository::new(pool);

    repo.insert_daily_batch(&[
        DailyMetricRow {
            tunnel_id: TUNNEL_A.to_owned(),
            day: "2024-01-01".to_owned(),
            bytes_tx_total: 1,
            bytes_rx_total: 1,
        },
        DailyMetricRow {
            tunnel_id: TUNNEL_A.to_owned(),
            day: "2025-06-15".to_owned(),
            bytes_tx_total: 2,
            bytes_rx_total: 2,
        },
        DailyMetricRow {
            tunnel_id: TUNNEL_A.to_owned(),
            day: "2026-04-01".to_owned(),
            bytes_tx_total: 3,
            bytes_rx_total: 3,
        },
    ])
    .await
    .unwrap();

    let deleted = repo.trim_daily("2025-06-15").await.unwrap();
    assert_eq!(deleted, 1);

    let surviving = repo
        .query_daily(TUNNEL_A, "1970-01-01", "9999-12-31")
        .await
        .unwrap();
    assert_eq!(surviving.len(), 2);
    assert!(surviving.iter().all(|r| r.day.as_str() >= "2025-06-15"));
}

#[tokio::test]
async fn cascade_delete_clears_history() {
    let pool = test_pool().await;
    seed_tunnel(&pool, TUNNEL_A, "wg_ward0").await;
    let metrics = SqliteTunnelMetricsRepository::new(pool.clone());

    metrics
        .insert_intraday(&IntradayMetricRow {
            tunnel_id: TUNNEL_A.to_owned(),
            ts: 1_700_000_000,
            bytes_tx_delta: 1,
            bytes_rx_delta: 1,
        })
        .await
        .unwrap();
    metrics
        .insert_daily_batch(&[DailyMetricRow {
            tunnel_id: TUNNEL_A.to_owned(),
            day: "2026-01-15".to_owned(),
            bytes_tx_total: 1,
            bytes_rx_total: 1,
        }])
        .await
        .unwrap();

    // Foreign keys enforce ON DELETE CASCADE only when the per-connection
    // pragma is on. The migration sets it on connection-init, but in
    // tests we set it explicitly to guarantee deterministic behavior.
    sqlx::query("PRAGMA foreign_keys = ON;")
        .execute(&pool)
        .await
        .unwrap();

    SqliteTunnelRepository::new(pool.clone())
        .delete(TUNNEL_A)
        .await
        .unwrap();

    assert!(
        metrics
            .query_intraday(TUNNEL_A, 0, i64::MAX)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        metrics
            .query_daily(TUNNEL_A, "1970-01-01", "9999-12-31")
            .await
            .unwrap()
            .is_empty()
    );
}
