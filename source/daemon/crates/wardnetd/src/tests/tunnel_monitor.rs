use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use tokio::sync::broadcast;
use uuid::Uuid;
use wardnet_types::event::WardnetEvent;
use wardnet_types::tunnel::{Tunnel, TunnelConfig, TunnelStatus};

use crate::event::EventPublisher;
use crate::repository::TunnelRepository;
use crate::repository::tunnel::TunnelRow;
use crate::tunnel_interface::{CreateTunnelParams, TunnelInterface, TunnelStats};
use crate::tunnel_monitor::TunnelMonitor;

// -- Mock TunnelRepository ------------------------------------------------

/// Tracks calls to `update_stats` for assertion.
struct MockTunnelRepo {
    tunnels: Mutex<Vec<Tunnel>>,
    stats_updates: Mutex<Vec<StatsUpdate>>,
    find_all_error: Mutex<bool>,
}

#[derive(Debug, Clone)]
struct StatsUpdate {
    id: String,
    bytes_tx: i64,
    bytes_rx: i64,
    last_handshake: Option<String>,
}

impl MockTunnelRepo {
    fn new(tunnels: Vec<Tunnel>) -> Self {
        Self {
            tunnels: Mutex::new(tunnels),
            stats_updates: Mutex::new(Vec::new()),
            find_all_error: Mutex::new(false),
        }
    }

    fn stats_updates(&self) -> Vec<StatsUpdate> {
        self.stats_updates.lock().unwrap().clone()
    }
}

#[async_trait]
impl TunnelRepository for MockTunnelRepo {
    async fn find_all(&self) -> anyhow::Result<Vec<Tunnel>> {
        if *self.find_all_error.lock().unwrap() {
            anyhow::bail!("simulated find_all error");
        }
        Ok(self.tunnels.lock().unwrap().clone())
    }

    async fn find_by_id(&self, _id: &str) -> anyhow::Result<Option<Tunnel>> {
        Ok(None)
    }

    async fn find_config_by_id(&self, _id: &str) -> anyhow::Result<Option<TunnelConfig>> {
        Ok(None)
    }

    async fn insert(&self, _row: &TunnelRow) -> anyhow::Result<()> {
        Ok(())
    }

    async fn update_status(&self, _id: &str, _status: &str) -> anyhow::Result<()> {
        Ok(())
    }

    async fn update_stats(
        &self,
        id: &str,
        bytes_tx: i64,
        bytes_rx: i64,
        last_handshake: Option<&str>,
    ) -> anyhow::Result<()> {
        self.stats_updates.lock().unwrap().push(StatsUpdate {
            id: id.to_owned(),
            bytes_tx,
            bytes_rx,
            last_handshake: last_handshake.map(ToOwned::to_owned),
        });
        Ok(())
    }

    async fn delete(&self, _id: &str) -> anyhow::Result<()> {
        Ok(())
    }

    async fn next_interface_index(&self) -> anyhow::Result<i64> {
        Ok(0)
    }

    async fn count(&self) -> anyhow::Result<i64> {
        Ok(0)
    }
}

// -- Mock TunnelInterface -------------------------------------------------

/// Returns predetermined stats for `get_stats`.
struct MockTunnelInterface {
    stats: Mutex<Option<TunnelStats>>,
    get_stats_error: Mutex<bool>,
}

impl MockTunnelInterface {
    fn with_stats(stats: TunnelStats) -> Self {
        Self {
            stats: Mutex::new(Some(stats)),
            get_stats_error: Mutex::new(false),
        }
    }

    fn returning_none() -> Self {
        Self {
            stats: Mutex::new(None),
            get_stats_error: Mutex::new(false),
        }
    }

    fn returning_error() -> Self {
        Self {
            stats: Mutex::new(None),
            get_stats_error: Mutex::new(true),
        }
    }
}

#[async_trait]
impl TunnelInterface for MockTunnelInterface {
    async fn create(&self, _params: CreateTunnelParams) -> anyhow::Result<()> {
        Ok(())
    }

    async fn bring_up(&self, _interface_name: &str) -> anyhow::Result<()> {
        Ok(())
    }

    async fn tear_down(&self, _interface_name: &str) -> anyhow::Result<()> {
        Ok(())
    }

    async fn remove(&self, _interface_name: &str) -> anyhow::Result<()> {
        Ok(())
    }

    async fn get_stats(&self, _interface_name: &str) -> anyhow::Result<Option<TunnelStats>> {
        if *self.get_stats_error.lock().unwrap() {
            anyhow::bail!("simulated get_stats error");
        }
        Ok(self.stats.lock().unwrap().clone())
    }

    async fn list(&self) -> anyhow::Result<Vec<String>> {
        Ok(Vec::new())
    }
}

// -- Mock EventPublisher --------------------------------------------------

/// Captures published events for assertion.
struct MockEventPublisher {
    events: Mutex<Vec<WardnetEvent>>,
}

impl MockEventPublisher {
    fn new() -> Self {
        Self {
            events: Mutex::new(Vec::new()),
        }
    }

    fn published_events(&self) -> Vec<WardnetEvent> {
        self.events.lock().unwrap().clone()
    }
}

impl EventPublisher for MockEventPublisher {
    fn publish(&self, event: WardnetEvent) {
        self.events.lock().unwrap().push(event);
    }

    fn subscribe(&self) -> broadcast::Receiver<WardnetEvent> {
        let (_, rx) = broadcast::channel(16);
        rx
    }
}

// -- Helpers --------------------------------------------------------------

fn make_tunnel(id: Uuid, interface_name: &str, status: TunnelStatus) -> Tunnel {
    Tunnel {
        id,
        label: "Test Tunnel".to_owned(),
        country_code: "SE".to_owned(),
        provider: Some("TestVPN".to_owned()),
        interface_name: interface_name.to_owned(),
        endpoint: "198.51.100.1:51820".to_owned(),
        status,
        last_handshake: None,
        bytes_tx: 0,
        bytes_rx: 0,
        created_at: Utc::now(),
    }
}

fn make_stats(bytes_tx: u64, bytes_rx: u64, last_handshake: Option<DateTime<Utc>>) -> TunnelStats {
    TunnelStats {
        bytes_tx,
        bytes_rx,
        last_handshake,
    }
}

// -- Tests ----------------------------------------------------------------

#[tokio::test]
async fn stats_loop_updates_stats_for_up_tunnel() {
    let tunnel_id = Uuid::new_v4();
    let tunnel = make_tunnel(tunnel_id, "wg_ward0", TunnelStatus::Up);

    let repo = Arc::new(MockTunnelRepo::new(vec![tunnel]));
    let wg = Arc::new(MockTunnelInterface::with_stats(make_stats(
        1000,
        2000,
        Some(Utc::now()),
    )));
    let events = Arc::new(MockEventPublisher::new());

    let parent = tracing::info_span!("test");
    let monitor = TunnelMonitor::start(
        repo.clone(),
        wg,
        events.clone(),
        1, // 1-second stats interval
        60,
        &parent,
    );

    // Allow the stats loop to fire at least once.
    tokio::time::sleep(tokio::time::Duration::from_millis(1500)).await;
    monitor.shutdown().await;

    // Verify stats were updated in the repository.
    let updates = repo.stats_updates();
    assert!(!updates.is_empty(), "expected at least one stats update");
    assert_eq!(updates[0].id, tunnel_id.to_string());
    assert_eq!(updates[0].bytes_tx, 1000);
    assert_eq!(updates[0].bytes_rx, 2000);
    assert!(updates[0].last_handshake.is_some());

    // Verify TunnelStatsUpdated event was published.
    let published = events.published_events();
    assert!(
        !published.is_empty(),
        "expected at least one published event"
    );
    assert!(matches!(
        &published[0],
        WardnetEvent::TunnelStatsUpdated {
            tunnel_id: id,
            bytes_tx: 1000,
            bytes_rx: 2000,
            ..
        } if *id == tunnel_id
    ));
}

#[tokio::test]
async fn stats_loop_skips_down_tunnels() {
    let tunnel = make_tunnel(Uuid::new_v4(), "wg_ward0", TunnelStatus::Down);

    let repo = Arc::new(MockTunnelRepo::new(vec![tunnel]));
    let wg = Arc::new(MockTunnelInterface::with_stats(make_stats(100, 200, None)));
    let events = Arc::new(MockEventPublisher::new());

    let parent = tracing::info_span!("test");
    let monitor = TunnelMonitor::start(repo.clone(), wg, events.clone(), 1, 60, &parent);

    tokio::time::sleep(tokio::time::Duration::from_millis(1500)).await;
    monitor.shutdown().await;

    // Down tunnels should not trigger stats updates.
    let updates = repo.stats_updates();
    assert!(
        updates.is_empty(),
        "down tunnels should not be polled for stats"
    );

    let published = events.published_events();
    assert!(
        published.is_empty(),
        "no events should be published for down tunnels"
    );
}

#[tokio::test]
async fn stats_loop_handles_get_stats_error_gracefully() {
    let tunnel = make_tunnel(Uuid::new_v4(), "wg_ward0", TunnelStatus::Up);

    let repo = Arc::new(MockTunnelRepo::new(vec![tunnel]));
    let wg = Arc::new(MockTunnelInterface::returning_error());
    let events = Arc::new(MockEventPublisher::new());

    let parent = tracing::info_span!("test");
    let monitor = TunnelMonitor::start(repo.clone(), wg, events.clone(), 1, 60, &parent);

    tokio::time::sleep(tokio::time::Duration::from_millis(1500)).await;
    monitor.shutdown().await;

    // Error in get_stats should not crash; no stats updates or events.
    let updates = repo.stats_updates();
    assert!(
        updates.is_empty(),
        "no stats should be saved when get_stats errors"
    );

    let published = events.published_events();
    assert!(published.is_empty(), "no events when get_stats errors");
}

#[tokio::test]
async fn stats_loop_handles_none_stats() {
    let tunnel = make_tunnel(Uuid::new_v4(), "wg_ward0", TunnelStatus::Up);

    let repo = Arc::new(MockTunnelRepo::new(vec![tunnel]));
    let wg = Arc::new(MockTunnelInterface::returning_none());
    let events = Arc::new(MockEventPublisher::new());

    let parent = tracing::info_span!("test");
    let monitor = TunnelMonitor::start(repo.clone(), wg, events.clone(), 1, 60, &parent);

    tokio::time::sleep(tokio::time::Duration::from_millis(1500)).await;
    monitor.shutdown().await;

    // None stats (interface not found) should be silently skipped.
    let updates = repo.stats_updates();
    assert!(updates.is_empty());
}

#[tokio::test]
async fn health_loop_runs_without_error_for_healthy_tunnel() {
    let tunnel = make_tunnel(Uuid::new_v4(), "wg_ward0", TunnelStatus::Up);

    // Recent handshake -- should not generate warnings (just verifying no crash).
    let repo = Arc::new(MockTunnelRepo::new(vec![tunnel]));
    let wg = Arc::new(MockTunnelInterface::with_stats(make_stats(
        0,
        0,
        Some(Utc::now()),
    )));
    let events = Arc::new(MockEventPublisher::new());

    let parent = tracing::info_span!("test");
    let monitor = TunnelMonitor::start(repo, wg, events, 60, 1, &parent);

    // Let health loop run at least one tick.
    tokio::time::sleep(tokio::time::Duration::from_millis(1500)).await;
    monitor.shutdown().await;
    // If we reach here without panic, the health loop handled a healthy tunnel correctly.
}

#[tokio::test]
async fn health_loop_handles_stale_handshake() {
    let tunnel = make_tunnel(Uuid::new_v4(), "wg_ward0", TunnelStatus::Up);

    // Handshake from 10 minutes ago -- stale.
    let stale_time = Utc::now() - chrono::Duration::minutes(10);
    let repo = Arc::new(MockTunnelRepo::new(vec![tunnel]));
    let wg = Arc::new(MockTunnelInterface::with_stats(make_stats(
        0,
        0,
        Some(stale_time),
    )));
    let events = Arc::new(MockEventPublisher::new());

    let parent = tracing::info_span!("test");
    let monitor = TunnelMonitor::start(repo, wg, events, 60, 1, &parent);

    tokio::time::sleep(tokio::time::Duration::from_millis(1500)).await;
    monitor.shutdown().await;
    // The health loop should log a warning but not crash.
}

#[tokio::test]
async fn health_loop_handles_missing_interface() {
    let tunnel = make_tunnel(Uuid::new_v4(), "wg_ward0", TunnelStatus::Up);

    let repo = Arc::new(MockTunnelRepo::new(vec![tunnel]));
    let wg = Arc::new(MockTunnelInterface::returning_none());
    let events = Arc::new(MockEventPublisher::new());

    let parent = tracing::info_span!("test");
    let monitor = TunnelMonitor::start(repo, wg, events, 60, 1, &parent);

    tokio::time::sleep(tokio::time::Duration::from_millis(1500)).await;
    monitor.shutdown().await;
    // Should log an error but not crash.
}

#[tokio::test]
async fn health_loop_handles_get_stats_error() {
    let tunnel = make_tunnel(Uuid::new_v4(), "wg_ward0", TunnelStatus::Up);

    let repo = Arc::new(MockTunnelRepo::new(vec![tunnel]));
    let wg = Arc::new(MockTunnelInterface::returning_error());
    let events = Arc::new(MockEventPublisher::new());

    let parent = tracing::info_span!("test");
    let monitor = TunnelMonitor::start(repo, wg, events, 60, 1, &parent);

    tokio::time::sleep(tokio::time::Duration::from_millis(1500)).await;
    monitor.shutdown().await;
    // Should log an error but not crash.
}

#[tokio::test]
async fn shutdown_stops_both_loops() {
    let repo = Arc::new(MockTunnelRepo::new(vec![]));
    let wg = Arc::new(MockTunnelInterface::returning_none());
    let events = Arc::new(MockEventPublisher::new());

    let parent = tracing::info_span!("test");
    let monitor = TunnelMonitor::start(repo, wg, events, 1, 1, &parent);

    // Shutdown immediately -- should complete without hanging.
    monitor.shutdown().await;
}

#[tokio::test]
async fn stats_loop_handles_find_all_error() {
    let repo = Arc::new(MockTunnelRepo::new(vec![]));
    *repo.find_all_error.lock().unwrap() = true;
    let wg = Arc::new(MockTunnelInterface::returning_none());
    let events = Arc::new(MockEventPublisher::new());

    let parent = tracing::info_span!("test");
    let monitor = TunnelMonitor::start(repo.clone(), wg, events.clone(), 1, 60, &parent);

    tokio::time::sleep(tokio::time::Duration::from_millis(1500)).await;
    monitor.shutdown().await;

    // Error in find_all should not crash the loop; no stats updates or events.
    let updates = repo.stats_updates();
    assert!(updates.is_empty());
    let published = events.published_events();
    assert!(published.is_empty());
}

#[tokio::test]
async fn health_loop_handles_find_all_error() {
    let repo = Arc::new(MockTunnelRepo::new(vec![]));
    *repo.find_all_error.lock().unwrap() = true;
    let wg = Arc::new(MockTunnelInterface::returning_none());
    let events = Arc::new(MockEventPublisher::new());

    let parent = tracing::info_span!("test");
    let monitor = TunnelMonitor::start(repo, wg, events, 60, 1, &parent);

    tokio::time::sleep(tokio::time::Duration::from_millis(1500)).await;
    monitor.shutdown().await;
    // If we reach here, the health loop handled the error without crashing.
}

#[tokio::test]
async fn stats_loop_handles_no_last_handshake() {
    let tunnel_id = Uuid::new_v4();
    let tunnel = make_tunnel(tunnel_id, "wg_ward0", TunnelStatus::Up);

    let repo = Arc::new(MockTunnelRepo::new(vec![tunnel]));
    let wg = Arc::new(MockTunnelInterface::with_stats(make_stats(500, 600, None)));
    let events = Arc::new(MockEventPublisher::new());

    let parent = tracing::info_span!("test");
    let monitor = TunnelMonitor::start(repo.clone(), wg, events.clone(), 1, 60, &parent);

    tokio::time::sleep(tokio::time::Duration::from_millis(1500)).await;
    monitor.shutdown().await;

    let updates = repo.stats_updates();
    assert!(!updates.is_empty());
    assert_eq!(updates[0].bytes_tx, 500);
    assert_eq!(updates[0].bytes_rx, 600);
    assert!(updates[0].last_handshake.is_none());
}
