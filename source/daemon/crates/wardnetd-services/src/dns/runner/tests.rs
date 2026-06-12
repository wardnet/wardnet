//! Tests for the DNS runner: `build_authoritative_view` against a real
//! SQLite-backed `DnsLocalServiceImpl` (the `.lan` zone is migration-seeded),
//! and the start → event → shutdown lifecycle with mock `DnsService` /
//! `DnsServer`.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use sqlx::SqlitePool;
use sqlx::sqlite::SqlitePoolOptions;
use uuid::Uuid;
use wardnet_common::auth::AuthContext;
use wardnet_common::dns::DnsConfig;
use wardnet_common::event::WardnetEvent;
use wardnetd_data::repository::SqliteDnsLocalRepository;

use super::{DnsRunner, build_authoritative_view};
use crate::dns::authoritative::AuthoritativeView;
use crate::dns::server::DnsServer;
use crate::dns::service::DnsService;
use crate::dns_local::DnsLocalService;
use crate::error::AppError;
use crate::event::{BroadcastEventBus, EventPublisher};

fn admin() -> AuthContext {
    AuthContext::Admin {
        admin_id: Uuid::nil(),
    }
}

async fn dns_local() -> Arc<dyn DnsLocalService> {
    let pool: SqlitePool = SqlitePoolOptions::new()
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("../wardnetd-data/migrations")
        .run(&pool)
        .await
        .unwrap();
    let repo = Arc::new(SqliteDnsLocalRepository::new(pool));
    let events: Arc<dyn EventPublisher> = Arc::new(BroadcastEventBus::new(16));
    Arc::new(crate::dns_local::service::DnsLocalServiceImpl::new(
        repo, events,
    ))
}

// ── build_authoritative_view ──────────────────────────────────────────────

#[tokio::test]
async fn build_view_includes_seeded_lan_zone() {
    let svc = dns_local().await;
    let view = build_authoritative_view(&svc, &admin()).await;
    // The migration-seeded `.lan` zone makes the runner authoritative for it.
    assert!(
        view.authoritative_zone("lan").is_some(),
        "view should be authoritative for the seeded .lan zone"
    );
}

// ── Lifecycle: start → events → shutdown ──────────────────────────────────

struct MockDnsService {
    enabled: bool,
}

#[async_trait]
impl DnsService for MockDnsService {
    async fn get_dns_config(&self) -> Result<DnsConfig, AppError> {
        Ok(DnsConfig {
            enabled: self.enabled,
            ..Default::default()
        })
    }

    async fn get_config(&self) -> Result<wardnet_common::api::DnsConfigResponse, AppError> {
        unimplemented!()
    }
    async fn update_config(
        &self,
        _req: wardnet_common::api::UpdateDnsConfigRequest,
    ) -> Result<wardnet_common::api::DnsConfigResponse, AppError> {
        unimplemented!()
    }
    async fn toggle(
        &self,
        _req: wardnet_common::api::ToggleDnsRequest,
    ) -> Result<wardnet_common::api::DnsConfigResponse, AppError> {
        unimplemented!()
    }
    async fn status(&self) -> Result<wardnet_common::api::DnsStatusResponse, AppError> {
        unimplemented!()
    }
    async fn flush_cache(&self) -> Result<wardnet_common::api::DnsCacheFlushResponse, AppError> {
        unimplemented!()
    }
    async fn list_query_log(
        &self,
        _params: wardnet_common::api::ListQueryLogParams,
    ) -> Result<wardnet_common::api::ListQueryLogResponse, AppError> {
        unimplemented!()
    }
    fn subscribe_query_stream(
        &self,
    ) -> Result<tokio::sync::broadcast::Receiver<wardnet_common::api::QueryLogEvent>, AppError>
    {
        unimplemented!()
    }
    async fn flush_query_log(&self) -> Result<u64, AppError> {
        unimplemented!()
    }
    async fn insert_query_log_batch(
        &self,
        _entries: &[wardnetd_data::repository::QueryLogRow],
    ) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn cleanup_query_log(&self, _retention_days: u32) -> Result<u64, AppError> {
        unimplemented!()
    }
}

#[derive(Default)]
struct ServerState {
    running: AtomicBool,
    stopped: AtomicBool,
    view_updates: AtomicUsize,
}

struct MockDnsServer {
    state: Arc<ServerState>,
}

#[async_trait]
impl DnsServer for MockDnsServer {
    async fn start(&self) -> anyhow::Result<()> {
        self.state.running.store(true, Ordering::SeqCst);
        Ok(())
    }
    async fn stop(&self) -> anyhow::Result<()> {
        self.state.running.store(false, Ordering::SeqCst);
        self.state.stopped.store(true, Ordering::SeqCst);
        Ok(())
    }
    fn is_running(&self) -> bool {
        self.state.running.load(Ordering::SeqCst)
    }
    async fn flush_cache(&self) -> u64 {
        0
    }
    async fn cache_size(&self) -> u64 {
        0
    }
    async fn cache_hit_rate(&self) -> f64 {
        0.0
    }
    async fn update_config(&self, _config: DnsConfig) {}
    async fn update_authoritative_view(&self, _view: AuthoritativeView) {
        self.state.view_updates.fetch_add(1, Ordering::SeqCst);
    }
    async fn invalidate_domain(&self, _domain: &str) {}
}

async fn poll_until(f: impl Fn() -> bool) -> bool {
    for _ in 0..200 {
        if f() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    false
}

#[tokio::test]
async fn runner_starts_server_loads_view_and_rebuilds_on_event() {
    let svc: Arc<dyn DnsService> = Arc::new(MockDnsService { enabled: true });
    let state = Arc::new(ServerState::default());
    let server: Arc<dyn DnsServer> = Arc::new(MockDnsServer {
        state: state.clone(),
    });
    let local = dns_local().await;
    let events = BroadcastEventBus::new(16);
    let span = tracing::Span::none();

    let runner = DnsRunner::start(svc, server, local, &events, &span);

    // Startup: DNS enabled ⇒ server started + initial authoritative view loaded.
    assert!(
        poll_until(|| state.running.load(Ordering::SeqCst)
            && state.view_updates.load(Ordering::SeqCst) >= 1)
        .await,
        "runner should start the server and load the initial view"
    );
    let after_start = state.view_updates.load(Ordering::SeqCst);

    // A local-DNS change rebuilds the authoritative view.
    events.publish(WardnetEvent::DnsLocalChanged {
        domain: Some("nas.lab".to_owned()),
        timestamp: chrono::Utc::now(),
    });
    assert!(
        poll_until(|| state.view_updates.load(Ordering::SeqCst) > after_start).await,
        "DnsLocalChanged should rebuild the authoritative view"
    );

    runner.shutdown().await;
    assert!(
        state.stopped.load(Ordering::SeqCst),
        "shutdown should stop the DNS server"
    );
}
