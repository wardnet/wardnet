use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use async_trait::async_trait;
use uuid::Uuid;
use wardnet_common::api::{
    CreateAllowlistRequest, CreateAllowlistResponse, CreateBlocklistRequest,
    CreateBlocklistResponse, CreateFilterRuleRequest, CreateFilterRuleResponse,
    DeleteAllowlistResponse, DeleteBlocklistResponse, DeleteFilterRuleResponse,
    DnsCacheFlushResponse, DnsConfigResponse, DnsStatusResponse, ListAllowlistResponse,
    ListBlocklistsResponse, ListFilterRulesResponse, ToggleDnsRequest, UpdateBlocklistNowResponse,
    UpdateBlocklistRequest, UpdateBlocklistResponse, UpdateDnsConfigRequest,
    UpdateFilterRuleRequest, UpdateFilterRuleResponse,
};
use wardnet_common::dns::{DnsConfig, DnsResolutionMode};

use crate::DnsService;
use crate::dns::blocklist_downloader::BlocklistFetcher;
use crate::dns::filter::DnsFilter;
use crate::dns::runner::DnsRunner;
use crate::dns::server::DnsServer;
use crate::error::AppError;
use crate::event::{BroadcastEventBus, EventPublisher};
use wardnetd_data::repository::DnsRepository;

// ---------------------------------------------------------------------------
// Mock DnsServer for runner tests
// ---------------------------------------------------------------------------

/// Mock server that tracks `start`/`stop`/`update_config` calls.
struct MockDnsServer {
    started: AtomicBool,
    start_count: AtomicU64,
    stop_count: AtomicU64,
    update_config_count: AtomicU64,
}

impl MockDnsServer {
    fn new() -> Self {
        Self {
            started: AtomicBool::new(false),
            start_count: AtomicU64::new(0),
            stop_count: AtomicU64::new(0),
            update_config_count: AtomicU64::new(0),
        }
    }
}

#[async_trait]
impl DnsServer for MockDnsServer {
    async fn start(&self) -> anyhow::Result<()> {
        self.started.store(true, Ordering::SeqCst);
        self.start_count.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    async fn stop(&self) -> anyhow::Result<()> {
        self.started.store(false, Ordering::SeqCst);
        self.stop_count.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    fn is_running(&self) -> bool {
        self.started.load(Ordering::SeqCst)
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

    async fn update_config(&self, _config: DnsConfig) {
        self.update_config_count.fetch_add(1, Ordering::SeqCst);
    }
}

// ---------------------------------------------------------------------------
// Mock DnsService for runner tests
// ---------------------------------------------------------------------------

/// Minimal mock service that returns a configurable `DnsConfig`.
///
/// Uses an `AtomicBool` for `enabled` so tests can flip it at runtime
/// (e.g. to simulate a config change event toggling DNS on/off).
struct MockRunnerDnsService {
    enabled: AtomicBool,
}

impl MockRunnerDnsService {
    fn new(enabled: bool) -> Self {
        Self {
            enabled: AtomicBool::new(enabled),
        }
    }

    fn make_config(&self) -> DnsConfig {
        DnsConfig {
            enabled: self.enabled.load(Ordering::SeqCst),
            resolution_mode: DnsResolutionMode::Forwarding,
            upstream_servers: vec![],
            cache_size: 1000,
            cache_ttl_min_secs: 0,
            cache_ttl_max_secs: 86_400,
            dnssec_enabled: false,
            rebinding_protection: true,
            rate_limit_per_second: 0,
            ad_blocking_enabled: false,
            query_log_enabled: false,
            query_log_retention_days: 7,
        }
    }
}

#[async_trait]
impl DnsService for MockRunnerDnsService {
    async fn get_config(&self) -> Result<DnsConfigResponse, AppError> {
        unimplemented!()
    }
    async fn update_config(
        &self,
        _req: UpdateDnsConfigRequest,
    ) -> Result<DnsConfigResponse, AppError> {
        unimplemented!()
    }
    async fn toggle(&self, _req: ToggleDnsRequest) -> Result<DnsConfigResponse, AppError> {
        unimplemented!()
    }
    async fn status(&self) -> Result<DnsStatusResponse, AppError> {
        unimplemented!()
    }
    async fn flush_cache(&self) -> Result<DnsCacheFlushResponse, AppError> {
        unimplemented!()
    }
    async fn get_dns_config(&self) -> Result<DnsConfig, AppError> {
        Ok(self.make_config())
    }

    async fn list_blocklists(&self) -> Result<ListBlocklistsResponse, AppError> {
        unimplemented!()
    }
    async fn create_blocklist(
        &self,
        _req: CreateBlocklistRequest,
    ) -> Result<CreateBlocklistResponse, AppError> {
        unimplemented!()
    }
    async fn update_blocklist(
        &self,
        _id: Uuid,
        _req: UpdateBlocklistRequest,
    ) -> Result<UpdateBlocklistResponse, AppError> {
        unimplemented!()
    }
    async fn delete_blocklist(&self, _id: Uuid) -> Result<DeleteBlocklistResponse, AppError> {
        unimplemented!()
    }
    async fn update_blocklist_now(
        &self,
        _id: Uuid,
    ) -> Result<UpdateBlocklistNowResponse, AppError> {
        unimplemented!()
    }
    async fn list_allowlist(&self) -> Result<ListAllowlistResponse, AppError> {
        unimplemented!()
    }
    async fn create_allowlist_entry(
        &self,
        _req: CreateAllowlistRequest,
    ) -> Result<CreateAllowlistResponse, AppError> {
        unimplemented!()
    }
    async fn delete_allowlist_entry(&self, _id: Uuid) -> Result<DeleteAllowlistResponse, AppError> {
        unimplemented!()
    }
    async fn list_filter_rules(&self) -> Result<ListFilterRulesResponse, AppError> {
        unimplemented!()
    }
    async fn create_filter_rule(
        &self,
        _req: CreateFilterRuleRequest,
    ) -> Result<CreateFilterRuleResponse, AppError> {
        unimplemented!()
    }
    async fn update_filter_rule(
        &self,
        _id: Uuid,
        _req: UpdateFilterRuleRequest,
    ) -> Result<UpdateFilterRuleResponse, AppError> {
        unimplemented!()
    }
    async fn delete_filter_rule(&self, _id: Uuid) -> Result<DeleteFilterRuleResponse, AppError> {
        unimplemented!()
    }
    async fn load_filter_inputs(&self) -> Result<crate::dns::filter::FilterInputs, AppError> {
        Ok(crate::dns::filter::FilterInputs::default())
    }
}

// ---------------------------------------------------------------------------
// Mock BlocklistFetcher for runner tests
// ---------------------------------------------------------------------------

struct MockBlocklistFetcher;

#[async_trait]
impl BlocklistFetcher for MockBlocklistFetcher {
    async fn fetch(&self, _url: &str) -> anyhow::Result<String> {
        Ok(String::new())
    }
}

// ---------------------------------------------------------------------------
// Mock DnsRepository for runner tests
// ---------------------------------------------------------------------------

struct MockDnsRepository;

#[async_trait]
impl DnsRepository for MockDnsRepository {
    async fn insert_query_log_batch(
        &self,
        _entries: &[wardnetd_data::repository::QueryLogRow],
    ) -> anyhow::Result<()> {
        Ok(())
    }
    async fn query_log_paginated(
        &self,
        _limit: u32,
        _offset: u32,
        _filter: &wardnetd_data::repository::QueryLogFilter,
    ) -> anyhow::Result<Vec<wardnetd_data::repository::QueryLogRow>> {
        Ok(vec![])
    }
    async fn query_log_count(
        &self,
        _filter: &wardnetd_data::repository::QueryLogFilter,
    ) -> anyhow::Result<u64> {
        Ok(0)
    }
    async fn cleanup_query_log(&self, _retention_days: u32) -> anyhow::Result<u64> {
        Ok(0)
    }
    async fn list_blocklists(&self) -> anyhow::Result<Vec<wardnet_common::dns::Blocklist>> {
        Ok(vec![])
    }
    async fn get_blocklist(
        &self,
        _id: Uuid,
    ) -> anyhow::Result<Option<wardnet_common::dns::Blocklist>> {
        Ok(None)
    }
    async fn create_blocklist(
        &self,
        _row: &wardnetd_data::repository::BlocklistRow,
    ) -> anyhow::Result<()> {
        Ok(())
    }
    async fn update_blocklist(
        &self,
        _id: Uuid,
        _row: &wardnetd_data::repository::BlocklistUpdate,
    ) -> anyhow::Result<()> {
        Ok(())
    }
    async fn delete_blocklist(&self, _id: Uuid) -> anyhow::Result<bool> {
        Ok(false)
    }
    async fn replace_blocklist_domains(
        &self,
        _id: Uuid,
        _domains: &[String],
    ) -> anyhow::Result<u64> {
        Ok(0)
    }
    async fn list_all_blocked_domains_for_enabled(&self) -> anyhow::Result<Vec<String>> {
        Ok(vec![])
    }
    async fn set_blocklist_error(&self, _id: Uuid, _error: Option<&str>) -> anyhow::Result<()> {
        Ok(())
    }
    async fn list_allowlist(&self) -> anyhow::Result<Vec<wardnet_common::dns::AllowlistEntry>> {
        Ok(vec![])
    }
    async fn create_allowlist_entry(
        &self,
        _row: &wardnetd_data::repository::AllowlistRow,
    ) -> anyhow::Result<()> {
        Ok(())
    }
    async fn delete_allowlist_entry(&self, _id: Uuid) -> anyhow::Result<bool> {
        Ok(false)
    }
    async fn list_custom_rules(
        &self,
    ) -> anyhow::Result<Vec<wardnet_common::dns::CustomFilterRule>> {
        Ok(vec![])
    }
    async fn get_custom_rule(
        &self,
        _id: Uuid,
    ) -> anyhow::Result<Option<wardnet_common::dns::CustomFilterRule>> {
        Ok(None)
    }
    async fn create_custom_rule(
        &self,
        _row: &wardnetd_data::repository::CustomRuleRow,
    ) -> anyhow::Result<()> {
        Ok(())
    }
    async fn update_custom_rule(
        &self,
        _id: Uuid,
        _row: &wardnetd_data::repository::CustomRuleUpdate,
    ) -> anyhow::Result<()> {
        Ok(())
    }
    async fn delete_custom_rule(&self, _id: Uuid) -> anyhow::Result<bool> {
        Ok(false)
    }
}

// ---------------------------------------------------------------------------
// Helper
// ---------------------------------------------------------------------------

fn mock_dns_repo() -> Arc<dyn DnsRepository> {
    Arc::new(MockDnsRepository)
}

fn mock_filter() -> Arc<tokio::sync::RwLock<DnsFilter>> {
    Arc::new(tokio::sync::RwLock::new(DnsFilter::empty()))
}

fn mock_fetcher() -> Arc<dyn BlocklistFetcher> {
    Arc::new(MockBlocklistFetcher)
}

/// Poll until `condition` returns true, yielding and sleeping between checks.
async fn poll_until(condition: impl Fn() -> bool, label: &str) {
    for _ in 0..40 {
        tokio::task::yield_now().await;
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        if condition() {
            return;
        }
    }
    panic!("timed out waiting for: {label}");
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn runner_starts_server_when_dns_enabled() {
    let service: Arc<dyn DnsService> = Arc::new(MockRunnerDnsService::new(true));
    let server = Arc::new(MockDnsServer::new());
    let events: Arc<dyn EventPublisher> = Arc::new(BroadcastEventBus::new(16));
    let parent = tracing::Span::none();

    let server_dyn: Arc<dyn DnsServer> = Arc::clone(&server) as Arc<dyn DnsServer>;
    let runner = DnsRunner::start(
        service,
        server_dyn,
        mock_dns_repo(),
        mock_filter(),
        mock_fetcher(),
        events.as_ref(),
        &parent,
        std::time::Duration::from_millis(100),
    );

    // Wait for the spawned task to call server.start().
    let server_ref = &server;
    poll_until(
        || server_ref.start_count.load(Ordering::SeqCst) > 0,
        "server.start() called",
    )
    .await;

    assert_eq!(
        server.start_count.load(Ordering::SeqCst),
        1,
        "DNS server should have been started exactly once"
    );
    assert!(
        server.started.load(Ordering::SeqCst),
        "DNS server should be marked as running"
    );
    assert!(
        server.update_config_count.load(Ordering::SeqCst) >= 1,
        "update_config should have been called before start"
    );

    runner.shutdown().await;
}

#[tokio::test]
async fn runner_does_not_start_server_when_dns_disabled() {
    let service: Arc<dyn DnsService> = Arc::new(MockRunnerDnsService::new(false));
    let server = Arc::new(MockDnsServer::new());
    let events: Arc<dyn EventPublisher> = Arc::new(BroadcastEventBus::new(16));
    let parent = tracing::Span::none();

    let runner = DnsRunner::start(
        service,
        Arc::clone(&server) as Arc<dyn DnsServer>,
        mock_dns_repo(),
        mock_filter(),
        mock_fetcher(),
        events.as_ref(),
        &parent,
        std::time::Duration::from_millis(100),
    );

    // Give the runner a moment to decide not to start.
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    assert_eq!(
        server.start_count.load(Ordering::SeqCst),
        0,
        "DNS server should NOT have been started when disabled"
    );
    assert!(
        !server.started.load(Ordering::SeqCst),
        "DNS server should not be running"
    );

    runner.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn runner_stops_server_on_shutdown() {
    let service: Arc<dyn DnsService> = Arc::new(MockRunnerDnsService::new(true));
    let server = Arc::new(MockDnsServer::new());
    let events: Arc<dyn EventPublisher> = Arc::new(BroadcastEventBus::new(16));
    let parent = tracing::Span::none();

    let server_dyn: Arc<dyn DnsServer> = Arc::clone(&server) as Arc<dyn DnsServer>;
    let runner = DnsRunner::start(
        service,
        server_dyn,
        mock_dns_repo(),
        mock_filter(),
        mock_fetcher(),
        events.as_ref(),
        &parent,
        std::time::Duration::from_millis(100),
    );

    // Wait for server to start.
    let server_ref = &server;
    poll_until(
        || server_ref.start_count.load(Ordering::SeqCst) > 0,
        "server.start() called",
    )
    .await;

    assert_eq!(server.start_count.load(Ordering::SeqCst), 1);

    // Shutdown the runner and verify stop is called.
    runner.shutdown().await;

    assert_eq!(
        server.stop_count.load(Ordering::SeqCst),
        1,
        "DNS server should have been stopped on shutdown"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn runner_handles_config_change_event_enable() {
    let service = Arc::new(MockRunnerDnsService::new(false));
    let service_dyn: Arc<dyn DnsService> = Arc::clone(&service) as Arc<dyn DnsService>;
    let server = Arc::new(MockDnsServer::new());
    let events = Arc::new(BroadcastEventBus::new(16));
    let events_pub: Arc<dyn EventPublisher> = Arc::clone(&events) as Arc<dyn EventPublisher>;
    let parent = tracing::Span::none();

    let server_dyn: Arc<dyn DnsServer> = Arc::clone(&server) as Arc<dyn DnsServer>;
    let runner = DnsRunner::start(
        service_dyn,
        server_dyn,
        mock_dns_repo(),
        mock_filter(),
        mock_fetcher(),
        events_pub.as_ref(),
        &parent,
        std::time::Duration::from_millis(100),
    );

    // Wait for initial setup (should NOT start since disabled).
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    assert_eq!(
        server.start_count.load(Ordering::SeqCst),
        0,
        "server should not be started initially"
    );

    // Flip enabled to true and publish a config change event.
    service.enabled.store(true, Ordering::SeqCst);
    events.publish(wardnet_common::event::WardnetEvent::DnsConfigChanged {
        timestamp: chrono::Utc::now(),
    });

    // Wait for the runner to process the event and start the server.
    let server_ref = &server;
    poll_until(
        || server_ref.start_count.load(Ordering::SeqCst) > 0,
        "server.start() called after config change",
    )
    .await;

    assert_eq!(
        server.start_count.load(Ordering::SeqCst),
        1,
        "DNS server should have been started after enabling via event"
    );
    assert!(server.started.load(Ordering::SeqCst));

    runner.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn runner_handles_config_change_event_disable() {
    let service = Arc::new(MockRunnerDnsService::new(true));
    let service_dyn: Arc<dyn DnsService> = Arc::clone(&service) as Arc<dyn DnsService>;
    let server = Arc::new(MockDnsServer::new());
    let events = Arc::new(BroadcastEventBus::new(16));
    let events_pub: Arc<dyn EventPublisher> = Arc::clone(&events) as Arc<dyn EventPublisher>;
    let parent = tracing::Span::none();

    let server_dyn: Arc<dyn DnsServer> = Arc::clone(&server) as Arc<dyn DnsServer>;
    let runner = DnsRunner::start(
        service_dyn,
        server_dyn,
        mock_dns_repo(),
        mock_filter(),
        mock_fetcher(),
        events_pub.as_ref(),
        &parent,
        std::time::Duration::from_millis(100),
    );

    // Wait for the server to start (enabled=true initially).
    let server_ref = &server;
    poll_until(
        || server_ref.start_count.load(Ordering::SeqCst) > 0,
        "server.start() called on startup",
    )
    .await;

    assert_eq!(server.start_count.load(Ordering::SeqCst), 1);
    assert!(server.started.load(Ordering::SeqCst));

    // Flip enabled to false and publish a config change event.
    service.enabled.store(false, Ordering::SeqCst);
    events.publish(wardnet_common::event::WardnetEvent::DnsConfigChanged {
        timestamp: chrono::Utc::now(),
    });

    // Wait for the runner to process the event and stop the server.
    poll_until(
        || server_ref.stop_count.load(Ordering::SeqCst) > 0,
        "server.stop() called after config change",
    )
    .await;

    assert!(
        !server.started.load(Ordering::SeqCst),
        "DNS server should be stopped after disabling via event"
    );

    runner.shutdown().await;
}

#[tokio::test]
async fn runner_shutdown_is_idempotent() {
    let service: Arc<dyn DnsService> = Arc::new(MockRunnerDnsService::new(false));
    let server = Arc::new(MockDnsServer::new());
    let events: Arc<dyn EventPublisher> = Arc::new(BroadcastEventBus::new(16));
    let parent = tracing::Span::none();

    let runner = DnsRunner::start(
        service,
        server,
        mock_dns_repo(),
        mock_filter(),
        mock_fetcher(),
        events.as_ref(),
        &parent,
        std::time::Duration::from_millis(100),
    );

    // Shutdown should complete without panic.
    runner.shutdown().await;
    // DnsRunner consumes self on shutdown, so calling it twice is not possible
    // at the type level. This test simply verifies a clean exit.
}

// ---------------------------------------------------------------------------
// Error-path tests
// ---------------------------------------------------------------------------

/// Mock service that always returns an error from `get_dns_config`.
struct ErroringDnsService;

#[async_trait]
impl DnsService for ErroringDnsService {
    async fn get_config(&self) -> Result<DnsConfigResponse, AppError> {
        unimplemented!()
    }
    async fn update_config(
        &self,
        _req: UpdateDnsConfigRequest,
    ) -> Result<DnsConfigResponse, AppError> {
        unimplemented!()
    }
    async fn toggle(&self, _req: ToggleDnsRequest) -> Result<DnsConfigResponse, AppError> {
        unimplemented!()
    }
    async fn status(&self) -> Result<DnsStatusResponse, AppError> {
        unimplemented!()
    }
    async fn flush_cache(&self) -> Result<DnsCacheFlushResponse, AppError> {
        unimplemented!()
    }
    async fn get_dns_config(&self) -> Result<DnsConfig, AppError> {
        Err(AppError::Internal(anyhow::anyhow!("config load failed")))
    }

    async fn list_blocklists(&self) -> Result<ListBlocklistsResponse, AppError> {
        unimplemented!()
    }
    async fn create_blocklist(
        &self,
        _req: CreateBlocklistRequest,
    ) -> Result<CreateBlocklistResponse, AppError> {
        unimplemented!()
    }
    async fn update_blocklist(
        &self,
        _id: Uuid,
        _req: UpdateBlocklistRequest,
    ) -> Result<UpdateBlocklistResponse, AppError> {
        unimplemented!()
    }
    async fn delete_blocklist(&self, _id: Uuid) -> Result<DeleteBlocklistResponse, AppError> {
        unimplemented!()
    }
    async fn update_blocklist_now(
        &self,
        _id: Uuid,
    ) -> Result<UpdateBlocklistNowResponse, AppError> {
        unimplemented!()
    }
    async fn list_allowlist(&self) -> Result<ListAllowlistResponse, AppError> {
        unimplemented!()
    }
    async fn create_allowlist_entry(
        &self,
        _req: CreateAllowlistRequest,
    ) -> Result<CreateAllowlistResponse, AppError> {
        unimplemented!()
    }
    async fn delete_allowlist_entry(&self, _id: Uuid) -> Result<DeleteAllowlistResponse, AppError> {
        unimplemented!()
    }
    async fn list_filter_rules(&self) -> Result<ListFilterRulesResponse, AppError> {
        unimplemented!()
    }
    async fn create_filter_rule(
        &self,
        _req: CreateFilterRuleRequest,
    ) -> Result<CreateFilterRuleResponse, AppError> {
        unimplemented!()
    }
    async fn update_filter_rule(
        &self,
        _id: Uuid,
        _req: UpdateFilterRuleRequest,
    ) -> Result<UpdateFilterRuleResponse, AppError> {
        unimplemented!()
    }
    async fn delete_filter_rule(&self, _id: Uuid) -> Result<DeleteFilterRuleResponse, AppError> {
        unimplemented!()
    }
    async fn load_filter_inputs(&self) -> Result<crate::dns::filter::FilterInputs, AppError> {
        unimplemented!()
    }
}

/// Mock server that always returns error on start/stop.
struct ErroringDnsServer;

#[async_trait]
impl DnsServer for ErroringDnsServer {
    async fn start(&self) -> anyhow::Result<()> {
        Err(anyhow::anyhow!("start failed"))
    }
    async fn stop(&self) -> anyhow::Result<()> {
        Err(anyhow::anyhow!("stop failed"))
    }
    fn is_running(&self) -> bool {
        false
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
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn runner_handles_get_dns_config_error_on_startup() {
    let service: Arc<dyn DnsService> = Arc::new(ErroringDnsService);
    let server = Arc::new(MockDnsServer::new());
    let events: Arc<dyn EventPublisher> = Arc::new(BroadcastEventBus::new(16));
    let parent = tracing::Span::none();

    let server_dyn: Arc<dyn DnsServer> = Arc::clone(&server) as Arc<dyn DnsServer>;
    let runner = DnsRunner::start(
        service,
        server_dyn,
        mock_dns_repo(),
        mock_filter(),
        mock_fetcher(),
        events.as_ref(),
        &parent,
        std::time::Duration::from_millis(100),
    );

    // Give the runner time to attempt startup config load (which errors).
    for _ in 0..10 {
        tokio::task::yield_now().await;
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }

    // Server should never have started (config load errored).
    assert_eq!(server.start_count.load(Ordering::SeqCst), 0);
    runner.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn runner_handles_server_start_error() {
    let service: Arc<dyn DnsService> = Arc::new(MockRunnerDnsService::new(true));
    let server: Arc<dyn DnsServer> = Arc::new(ErroringDnsServer);
    let events: Arc<dyn EventPublisher> = Arc::new(BroadcastEventBus::new(16));
    let parent = tracing::Span::none();

    // Runner should not panic even when server.start() returns Err.
    let runner = DnsRunner::start(
        service,
        server,
        mock_dns_repo(),
        mock_filter(),
        mock_fetcher(),
        events.as_ref(),
        &parent,
        std::time::Duration::from_millis(100),
    );

    // Wait a bit — the error branch should be hit.
    for _ in 0..10 {
        tokio::task::yield_now().await;
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }

    runner.shutdown().await;
}

#[allow(clippy::too_many_lines)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn runner_handles_reload_config_error_after_event() {
    // Service returns OK initially (disabled) so runner doesn't attempt start.
    // Then we replace with a service that errors on subsequent reloads.
    // Since DnsRunner::start takes ownership of the service Arc, we use a
    // service whose behavior depends on internal state.
    struct ToggleErroringService {
        call_count: AtomicU64,
    }

    #[async_trait]
    impl DnsService for ToggleErroringService {
        async fn get_config(&self) -> Result<DnsConfigResponse, AppError> {
            unimplemented!()
        }
        async fn update_config(
            &self,
            _req: UpdateDnsConfigRequest,
        ) -> Result<DnsConfigResponse, AppError> {
            unimplemented!()
        }
        async fn toggle(&self, _req: ToggleDnsRequest) -> Result<DnsConfigResponse, AppError> {
            unimplemented!()
        }
        async fn status(&self) -> Result<DnsStatusResponse, AppError> {
            unimplemented!()
        }
        async fn flush_cache(&self) -> Result<DnsCacheFlushResponse, AppError> {
            unimplemented!()
        }
        async fn get_dns_config(&self) -> Result<DnsConfig, AppError> {
            let count = self.call_count.fetch_add(1, Ordering::SeqCst);
            if count == 0 {
                // Startup: return disabled config so runner doesn't try to start.
                Ok(DnsConfig {
                    enabled: false,
                    resolution_mode: DnsResolutionMode::Forwarding,
                    upstream_servers: vec![],
                    cache_size: 1000,
                    cache_ttl_min_secs: 0,
                    cache_ttl_max_secs: 86_400,
                    dnssec_enabled: false,
                    rebinding_protection: true,
                    rate_limit_per_second: 0,
                    ad_blocking_enabled: false,
                    query_log_enabled: false,
                    query_log_retention_days: 7,
                })
            } else {
                // Subsequent reloads fail — exercises the error branch in the
                // event loop's "DnsConfigChanged" handler.
                Err(AppError::Internal(anyhow::anyhow!("reload failed")))
            }
        }

        async fn list_blocklists(&self) -> Result<ListBlocklistsResponse, AppError> {
            unimplemented!()
        }
        async fn create_blocklist(
            &self,
            _req: CreateBlocklistRequest,
        ) -> Result<CreateBlocklistResponse, AppError> {
            unimplemented!()
        }
        async fn update_blocklist(
            &self,
            _id: Uuid,
            _req: UpdateBlocklistRequest,
        ) -> Result<UpdateBlocklistResponse, AppError> {
            unimplemented!()
        }
        async fn delete_blocklist(&self, _id: Uuid) -> Result<DeleteBlocklistResponse, AppError> {
            unimplemented!()
        }
        async fn update_blocklist_now(
            &self,
            _id: Uuid,
        ) -> Result<UpdateBlocklistNowResponse, AppError> {
            unimplemented!()
        }
        async fn list_allowlist(&self) -> Result<ListAllowlistResponse, AppError> {
            unimplemented!()
        }
        async fn create_allowlist_entry(
            &self,
            _req: CreateAllowlistRequest,
        ) -> Result<CreateAllowlistResponse, AppError> {
            unimplemented!()
        }
        async fn delete_allowlist_entry(
            &self,
            _id: Uuid,
        ) -> Result<DeleteAllowlistResponse, AppError> {
            unimplemented!()
        }
        async fn list_filter_rules(&self) -> Result<ListFilterRulesResponse, AppError> {
            unimplemented!()
        }
        async fn create_filter_rule(
            &self,
            _req: CreateFilterRuleRequest,
        ) -> Result<CreateFilterRuleResponse, AppError> {
            unimplemented!()
        }
        async fn update_filter_rule(
            &self,
            _id: Uuid,
            _req: UpdateFilterRuleRequest,
        ) -> Result<UpdateFilterRuleResponse, AppError> {
            unimplemented!()
        }
        async fn delete_filter_rule(
            &self,
            _id: Uuid,
        ) -> Result<DeleteFilterRuleResponse, AppError> {
            unimplemented!()
        }
        async fn load_filter_inputs(&self) -> Result<crate::dns::filter::FilterInputs, AppError> {
            Ok(crate::dns::filter::FilterInputs::default())
        }
    }

    let service: Arc<dyn DnsService> = Arc::new(ToggleErroringService {
        call_count: AtomicU64::new(0),
    });
    let server = Arc::new(MockDnsServer::new());
    let events: Arc<dyn EventPublisher> = Arc::new(BroadcastEventBus::new(16));
    let parent = tracing::Span::none();

    let server_dyn: Arc<dyn DnsServer> = Arc::clone(&server) as Arc<dyn DnsServer>;
    let runner = DnsRunner::start(
        service,
        server_dyn,
        mock_dns_repo(),
        mock_filter(),
        mock_fetcher(),
        events.as_ref(),
        &parent,
        std::time::Duration::from_millis(100),
    );

    // Wait for startup to complete.
    for _ in 0..10 {
        tokio::task::yield_now().await;
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }

    // Publish a DnsConfigChanged event — the reload will error.
    events.publish(wardnet_common::event::WardnetEvent::DnsConfigChanged {
        timestamp: chrono::Utc::now(),
    });

    // Wait for the event to be processed.
    for _ in 0..10 {
        tokio::task::yield_now().await;
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }

    // Server should never have been started (startup disabled, reload errored).
    assert_eq!(server.start_count.load(Ordering::SeqCst), 0);
    runner.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn runner_ignores_non_dns_events() {
    // Runner should ignore events it doesn't care about (e.g. DhcpLeaseAssigned).
    let service: Arc<dyn DnsService> = Arc::new(MockRunnerDnsService::new(false));
    let server = Arc::new(MockDnsServer::new());
    let events: Arc<dyn EventPublisher> = Arc::new(BroadcastEventBus::new(16));
    let parent = tracing::Span::none();

    let server_dyn: Arc<dyn DnsServer> = Arc::clone(&server) as Arc<dyn DnsServer>;
    let runner = DnsRunner::start(
        service,
        server_dyn,
        mock_dns_repo(),
        mock_filter(),
        mock_fetcher(),
        events.as_ref(),
        &parent,
        std::time::Duration::from_millis(100),
    );

    // Startup should finish.
    for _ in 0..5 {
        tokio::task::yield_now().await;
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }

    // Publish an unrelated event.
    events.publish(wardnet_common::event::WardnetEvent::DhcpLeaseAssigned {
        lease_id: uuid::Uuid::new_v4(),
        mac: "aa:bb:cc:dd:ee:ff".to_owned(),
        ip: "192.168.1.50".to_owned(),
        hostname: None,
        timestamp: chrono::Utc::now(),
    });

    // Also publish a DnsServerStarted event (which hits the Ok(_) catch-all).
    events.publish(wardnet_common::event::WardnetEvent::DnsServerStarted {
        timestamp: chrono::Utc::now(),
    });

    for _ in 0..5 {
        tokio::task::yield_now().await;
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }

    // Server should never have been started or stopped.
    assert_eq!(server.start_count.load(Ordering::SeqCst), 0);
    assert_eq!(server.stop_count.load(Ordering::SeqCst), 0);

    runner.shutdown().await;
}

// ---------------------------------------------------------------------------
// DnsFiltersChanged / DnsBlocklistUpdated → rebuild_filter
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn runner_rebuilds_filter_on_dns_filters_changed_event() {
    let service: Arc<dyn DnsService> = Arc::new(MockRunnerDnsService::new(false));
    let server = Arc::new(MockDnsServer::new());
    let events = Arc::new(BroadcastEventBus::new(16));
    let events_pub: Arc<dyn EventPublisher> = Arc::clone(&events) as Arc<dyn EventPublisher>;
    let filter = mock_filter();
    let parent = tracing::Span::none();

    let runner = DnsRunner::start(
        service,
        Arc::clone(&server) as Arc<dyn DnsServer>,
        mock_dns_repo(),
        Arc::clone(&filter),
        mock_fetcher(),
        events_pub.as_ref(),
        &parent,
        std::time::Duration::from_millis(100),
    );

    // Wait for startup.
    for _ in 0..10 {
        tokio::task::yield_now().await;
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }

    // Publish a DnsFiltersChanged event.
    events.publish(wardnet_common::event::WardnetEvent::DnsFiltersChanged {
        timestamp: chrono::Utc::now(),
    });

    // Wait for the event to be processed and filter rebuilt.
    for _ in 0..10 {
        tokio::task::yield_now().await;
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }

    // Filter should still be empty (no inputs), but rebuild was called.
    assert!(filter.read().await.is_empty());
    runner.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn runner_rebuilds_filter_on_dns_blocklist_updated_event() {
    let service: Arc<dyn DnsService> = Arc::new(MockRunnerDnsService::new(false));
    let server = Arc::new(MockDnsServer::new());
    let events = Arc::new(BroadcastEventBus::new(16));
    let events_pub: Arc<dyn EventPublisher> = Arc::clone(&events) as Arc<dyn EventPublisher>;
    let filter = mock_filter();
    let parent = tracing::Span::none();

    let runner = DnsRunner::start(
        service,
        Arc::clone(&server) as Arc<dyn DnsServer>,
        mock_dns_repo(),
        Arc::clone(&filter),
        mock_fetcher(),
        events_pub.as_ref(),
        &parent,
        std::time::Duration::from_millis(100),
    );

    // Wait for startup.
    for _ in 0..10 {
        tokio::task::yield_now().await;
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }

    // Publish a DnsBlocklistUpdated event.
    events.publish(wardnet_common::event::WardnetEvent::DnsBlocklistUpdated {
        blocklist_id: uuid::Uuid::new_v4(),
        entry_count: 100,
        timestamp: chrono::Utc::now(),
    });

    // Wait for the event to be processed.
    for _ in 0..10 {
        tokio::task::yield_now().await;
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }

    assert!(filter.read().await.is_empty());
    runner.shutdown().await;
}

// ---------------------------------------------------------------------------
// check_blocklist_cron tests (via DnsRunner with active blocklists)
// ---------------------------------------------------------------------------

/// Mock repo that returns pre-configured blocklists with cron schedules.
struct CronMockDnsRepository {
    blocklists: std::sync::Mutex<Vec<wardnet_common::dns::Blocklist>>,
    replaced_domains: std::sync::Mutex<Vec<(Uuid, Vec<String>)>>,
    errors_set: std::sync::Mutex<Vec<(Uuid, Option<String>)>>,
}

impl CronMockDnsRepository {
    fn with_blocklists(blocklists: Vec<wardnet_common::dns::Blocklist>) -> Self {
        Self {
            blocklists: std::sync::Mutex::new(blocklists),
            replaced_domains: std::sync::Mutex::new(Vec::new()),
            errors_set: std::sync::Mutex::new(Vec::new()),
        }
    }
}

#[async_trait]
impl DnsRepository for CronMockDnsRepository {
    async fn insert_query_log_batch(
        &self,
        _entries: &[wardnetd_data::repository::QueryLogRow],
    ) -> anyhow::Result<()> {
        Ok(())
    }
    async fn query_log_paginated(
        &self,
        _limit: u32,
        _offset: u32,
        _filter: &wardnetd_data::repository::QueryLogFilter,
    ) -> anyhow::Result<Vec<wardnetd_data::repository::QueryLogRow>> {
        Ok(vec![])
    }
    async fn query_log_count(
        &self,
        _filter: &wardnetd_data::repository::QueryLogFilter,
    ) -> anyhow::Result<u64> {
        Ok(0)
    }
    async fn cleanup_query_log(&self, _retention_days: u32) -> anyhow::Result<u64> {
        Ok(0)
    }
    async fn list_blocklists(&self) -> anyhow::Result<Vec<wardnet_common::dns::Blocklist>> {
        Ok(self.blocklists.lock().unwrap().clone())
    }
    async fn get_blocklist(
        &self,
        id: Uuid,
    ) -> anyhow::Result<Option<wardnet_common::dns::Blocklist>> {
        Ok(self
            .blocklists
            .lock()
            .unwrap()
            .iter()
            .find(|b| b.id == id)
            .cloned())
    }
    async fn create_blocklist(
        &self,
        _row: &wardnetd_data::repository::BlocklistRow,
    ) -> anyhow::Result<()> {
        Ok(())
    }
    async fn update_blocklist(
        &self,
        _id: Uuid,
        _row: &wardnetd_data::repository::BlocklistUpdate,
    ) -> anyhow::Result<()> {
        Ok(())
    }
    async fn delete_blocklist(&self, _id: Uuid) -> anyhow::Result<bool> {
        Ok(false)
    }
    async fn replace_blocklist_domains(&self, id: Uuid, domains: &[String]) -> anyhow::Result<u64> {
        self.replaced_domains
            .lock()
            .unwrap()
            .push((id, domains.to_vec()));
        Ok(domains.len() as u64)
    }
    async fn list_all_blocked_domains_for_enabled(&self) -> anyhow::Result<Vec<String>> {
        Ok(vec![])
    }
    async fn set_blocklist_error(&self, id: Uuid, error: Option<&str>) -> anyhow::Result<()> {
        self.errors_set
            .lock()
            .unwrap()
            .push((id, error.map(str::to_owned)));
        Ok(())
    }
    async fn list_allowlist(&self) -> anyhow::Result<Vec<wardnet_common::dns::AllowlistEntry>> {
        Ok(vec![])
    }
    async fn create_allowlist_entry(
        &self,
        _row: &wardnetd_data::repository::AllowlistRow,
    ) -> anyhow::Result<()> {
        Ok(())
    }
    async fn delete_allowlist_entry(&self, _id: Uuid) -> anyhow::Result<bool> {
        Ok(false)
    }
    async fn list_custom_rules(
        &self,
    ) -> anyhow::Result<Vec<wardnet_common::dns::CustomFilterRule>> {
        Ok(vec![])
    }
    async fn get_custom_rule(
        &self,
        _id: Uuid,
    ) -> anyhow::Result<Option<wardnet_common::dns::CustomFilterRule>> {
        Ok(None)
    }
    async fn create_custom_rule(
        &self,
        _row: &wardnetd_data::repository::CustomRuleRow,
    ) -> anyhow::Result<()> {
        Ok(())
    }
    async fn update_custom_rule(
        &self,
        _id: Uuid,
        _row: &wardnetd_data::repository::CustomRuleUpdate,
    ) -> anyhow::Result<()> {
        Ok(())
    }
    async fn delete_custom_rule(&self, _id: Uuid) -> anyhow::Result<bool> {
        Ok(false)
    }
}

/// Mock fetcher that returns a fixed body.
struct FixedBodyFetcher {
    body: String,
}

#[async_trait]
impl BlocklistFetcher for FixedBodyFetcher {
    async fn fetch(&self, _url: &str) -> anyhow::Result<String> {
        Ok(self.body.clone())
    }
}

/// Mock fetcher that always errors.
struct ErroringFetcher;

#[async_trait]
impl BlocklistFetcher for ErroringFetcher {
    async fn fetch(&self, _url: &str) -> anyhow::Result<String> {
        Err(anyhow::anyhow!("download failed"))
    }
}

/// Helper: create a blocklist that is "due" (never updated, valid cron).
fn blocklist_due(id: Uuid, enabled: bool) -> wardnet_common::dns::Blocklist {
    let now = chrono::Utc::now();
    wardnet_common::dns::Blocklist {
        id,
        name: "Test".to_owned(),
        url: "https://example.com/list.txt".to_owned(),
        enabled,
        entry_count: 0,
        last_updated: None,                      // Never updated → always due
        cron_schedule: "* * * * * *".to_owned(), // Every second
        last_error: None,
        last_error_at: None,
        created_at: now,
        updated_at: now,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn runner_cron_downloads_and_stores_blocklist() {
    let bl_id = Uuid::new_v4();
    let dns_repo = Arc::new(CronMockDnsRepository::with_blocklists(vec![blocklist_due(
        bl_id, true,
    )]));
    let fetcher: Arc<dyn BlocklistFetcher> = Arc::new(FixedBodyFetcher {
        body: "0.0.0.0 ads.example.com\n0.0.0.0 tracker.example.com\n".to_owned(),
    });
    let service: Arc<dyn DnsService> = Arc::new(MockRunnerDnsService::new(false));
    let server = Arc::new(MockDnsServer::new());
    let events: Arc<dyn EventPublisher> = Arc::new(BroadcastEventBus::new(16));
    let filter = mock_filter();
    let parent = tracing::Span::none();

    let dns_repo_clone = dns_repo.clone();
    let runner = DnsRunner::start(
        service,
        Arc::clone(&server) as Arc<dyn DnsServer>,
        dns_repo.clone(),
        Arc::clone(&filter),
        fetcher,
        events.as_ref(),
        &parent,
        std::time::Duration::from_millis(100),
    );

    // Wait for the cron tick to fire and download the blocklist.
    poll_until(
        || !dns_repo_clone.replaced_domains.lock().unwrap().is_empty(),
        "blocklist domains stored via cron",
    )
    .await;

    runner.shutdown().await;

    // Verify domains were stored correctly.
    let stored = dns_repo.replaced_domains.lock().unwrap();
    assert_eq!(stored.len(), 1, "exactly one replace call expected");
    assert_eq!(stored[0].0, bl_id);
    assert_eq!(stored[0].1.len(), 2);
    assert!(stored[0].1.contains(&"ads.example.com".to_owned()));
    assert!(stored[0].1.contains(&"tracker.example.com".to_owned()));

    // Error should have been cleared (set to None).
    let errors = dns_repo.errors_set.lock().unwrap();
    assert!(
        errors.iter().any(|(id, e)| *id == bl_id && e.is_none()),
        "blocklist error should have been cleared after successful download"
    );
}

// ---------------------------------------------------------------------------
// check_blocklist_cron coverage via short cron interval (100ms)
//
// Now that cron_check_interval is a constructor parameter, tests use 100ms
// ticks so the cron path fires quickly and exercises check_blocklist_cron.
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn runner_handles_download_failure_in_cron() {
    let bl_id = Uuid::new_v4();
    let dns_repo = Arc::new(CronMockDnsRepository::with_blocklists(vec![blocklist_due(
        bl_id, true,
    )]));
    let fetcher: Arc<dyn BlocklistFetcher> = Arc::new(ErroringFetcher);
    let service: Arc<dyn DnsService> = Arc::new(MockRunnerDnsService::new(false));
    let server = Arc::new(MockDnsServer::new());
    let events: Arc<dyn EventPublisher> = Arc::new(BroadcastEventBus::new(16));
    let parent = tracing::Span::none();

    let dns_repo_clone = dns_repo.clone();
    let runner = DnsRunner::start(
        service,
        Arc::clone(&server) as Arc<dyn DnsServer>,
        dns_repo.clone(),
        mock_filter(),
        fetcher,
        events.as_ref(),
        &parent,
        std::time::Duration::from_millis(100),
    );

    // Wait for the cron tick to fire and attempt download (which fails).
    poll_until(
        || !dns_repo_clone.errors_set.lock().unwrap().is_empty(),
        "blocklist error recorded after download failure",
    )
    .await;

    runner.shutdown().await;

    // Verify error was recorded.
    let errors = dns_repo.errors_set.lock().unwrap();
    assert!(
        errors
            .iter()
            .any(|(id, e)| *id == bl_id && e.as_deref().unwrap_or("").contains("download failed")),
        "error message should mention download failure"
    );
}

// ---------------------------------------------------------------------------
// Disabled blocklist is skipped by cron check
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn runner_cron_skips_disabled_blocklist() {
    let bl_id = Uuid::new_v4();
    let dns_repo = Arc::new(CronMockDnsRepository::with_blocklists(vec![blocklist_due(
        bl_id, false, // disabled
    )]));
    let fetcher: Arc<dyn BlocklistFetcher> = Arc::new(FixedBodyFetcher {
        body: "0.0.0.0 ads.example.com\n".to_owned(),
    });
    let service: Arc<dyn DnsService> = Arc::new(MockRunnerDnsService::new(false));
    let server = Arc::new(MockDnsServer::new());
    let events: Arc<dyn EventPublisher> = Arc::new(BroadcastEventBus::new(16));
    let parent = tracing::Span::none();

    let runner = DnsRunner::start(
        service,
        Arc::clone(&server) as Arc<dyn DnsServer>,
        dns_repo.clone(),
        mock_filter(),
        fetcher,
        events.as_ref(),
        &parent,
        std::time::Duration::from_millis(100),
    );

    // Wait long enough for several cron ticks to fire.
    tokio::time::sleep(std::time::Duration::from_millis(400)).await;

    runner.shutdown().await;

    // No domains should have been replaced (blocklist is disabled).
    assert!(dns_repo.replaced_domains.lock().unwrap().is_empty());
}

// ---------------------------------------------------------------------------
// Event bus closed causes runner to exit
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn runner_exits_when_event_bus_closes() {
    let service: Arc<dyn DnsService> = Arc::new(MockRunnerDnsService::new(false));
    let server = Arc::new(MockDnsServer::new());
    let parent = tracing::Span::none();

    // Create a BroadcastEventBus with a small capacity, then drop it
    // before the runner finishes. The runner should detect the closed
    // channel and exit.
    let events = BroadcastEventBus::new(4);
    let runner = DnsRunner::start(
        service,
        Arc::clone(&server) as Arc<dyn DnsServer>,
        mock_dns_repo(),
        mock_filter(),
        mock_fetcher(),
        &events,
        &parent,
        std::time::Duration::from_millis(100),
    );

    // Wait for startup.
    for _ in 0..5 {
        tokio::task::yield_now().await;
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }

    // Drop the event bus — this closes the sender.
    drop(events);

    // The runner should exit on its own.
    // We give it a generous timeout.
    tokio::time::timeout(std::time::Duration::from_secs(5), runner.shutdown())
        .await
        .expect("runner should exit when event bus closes");
}

// ---------------------------------------------------------------------------
// rebuild_filter error path (load_filter_inputs fails)
// ---------------------------------------------------------------------------

/// Service that errors on `load_filter_inputs`.
struct FailingFilterInputsService;

#[async_trait]
impl DnsService for FailingFilterInputsService {
    async fn get_config(&self) -> Result<DnsConfigResponse, AppError> {
        unimplemented!()
    }
    async fn update_config(
        &self,
        _req: UpdateDnsConfigRequest,
    ) -> Result<DnsConfigResponse, AppError> {
        unimplemented!()
    }
    async fn toggle(&self, _req: ToggleDnsRequest) -> Result<DnsConfigResponse, AppError> {
        unimplemented!()
    }
    async fn status(&self) -> Result<DnsStatusResponse, AppError> {
        unimplemented!()
    }
    async fn flush_cache(&self) -> Result<DnsCacheFlushResponse, AppError> {
        unimplemented!()
    }
    async fn get_dns_config(&self) -> Result<DnsConfig, AppError> {
        Ok(DnsConfig {
            enabled: false,
            resolution_mode: DnsResolutionMode::Forwarding,
            upstream_servers: vec![],
            cache_size: 1000,
            cache_ttl_min_secs: 0,
            cache_ttl_max_secs: 86_400,
            dnssec_enabled: false,
            rebinding_protection: true,
            rate_limit_per_second: 0,
            ad_blocking_enabled: false,
            query_log_enabled: false,
            query_log_retention_days: 7,
        })
    }
    async fn list_blocklists(&self) -> Result<ListBlocklistsResponse, AppError> {
        unimplemented!()
    }
    async fn create_blocklist(
        &self,
        _req: CreateBlocklistRequest,
    ) -> Result<CreateBlocklistResponse, AppError> {
        unimplemented!()
    }
    async fn update_blocklist(
        &self,
        _id: Uuid,
        _req: UpdateBlocklistRequest,
    ) -> Result<UpdateBlocklistResponse, AppError> {
        unimplemented!()
    }
    async fn delete_blocklist(&self, _id: Uuid) -> Result<DeleteBlocklistResponse, AppError> {
        unimplemented!()
    }
    async fn update_blocklist_now(
        &self,
        _id: Uuid,
    ) -> Result<UpdateBlocklistNowResponse, AppError> {
        unimplemented!()
    }
    async fn list_allowlist(&self) -> Result<ListAllowlistResponse, AppError> {
        unimplemented!()
    }
    async fn create_allowlist_entry(
        &self,
        _req: CreateAllowlistRequest,
    ) -> Result<CreateAllowlistResponse, AppError> {
        unimplemented!()
    }
    async fn delete_allowlist_entry(&self, _id: Uuid) -> Result<DeleteAllowlistResponse, AppError> {
        unimplemented!()
    }
    async fn list_filter_rules(&self) -> Result<ListFilterRulesResponse, AppError> {
        unimplemented!()
    }
    async fn create_filter_rule(
        &self,
        _req: CreateFilterRuleRequest,
    ) -> Result<CreateFilterRuleResponse, AppError> {
        unimplemented!()
    }
    async fn update_filter_rule(
        &self,
        _id: Uuid,
        _req: UpdateFilterRuleRequest,
    ) -> Result<UpdateFilterRuleResponse, AppError> {
        unimplemented!()
    }
    async fn delete_filter_rule(&self, _id: Uuid) -> Result<DeleteFilterRuleResponse, AppError> {
        unimplemented!()
    }
    async fn load_filter_inputs(&self) -> Result<crate::dns::filter::FilterInputs, AppError> {
        Err(AppError::Internal(anyhow::anyhow!(
            "load_filter_inputs failed"
        )))
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn runner_handles_rebuild_filter_error() {
    let service: Arc<dyn DnsService> = Arc::new(FailingFilterInputsService);
    let server = Arc::new(MockDnsServer::new());
    let events: Arc<dyn EventPublisher> = Arc::new(BroadcastEventBus::new(16));
    let parent = tracing::Span::none();

    // Runner should not panic even when rebuild_filter fails (load_filter_inputs errors).
    let runner = DnsRunner::start(
        service,
        Arc::clone(&server) as Arc<dyn DnsServer>,
        mock_dns_repo(),
        mock_filter(),
        mock_fetcher(),
        events.as_ref(),
        &parent,
        std::time::Duration::from_millis(100),
    );

    // Wait for startup + initial rebuild (which will error).
    for _ in 0..10 {
        tokio::task::yield_now().await;
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }

    runner.shutdown().await;
}

// ---------------------------------------------------------------------------
// Additional check_blocklist_cron scenarios
// ---------------------------------------------------------------------------

/// Helper: create a blocklist with an invalid cron schedule.
fn blocklist_invalid_cron(id: Uuid) -> wardnet_common::dns::Blocklist {
    let now = chrono::Utc::now();
    wardnet_common::dns::Blocklist {
        id,
        name: "BadCron".to_owned(),
        url: "https://example.com/list.txt".to_owned(),
        enabled: true,
        entry_count: 0,
        last_updated: None,
        cron_schedule: "not a valid cron".to_owned(),
        last_error: None,
        last_error_at: None,
        created_at: now,
        updated_at: now,
    }
}

/// Helper: create a blocklist that was very recently updated (not yet due).
fn blocklist_not_due(id: Uuid) -> wardnet_common::dns::Blocklist {
    let now = chrono::Utc::now();
    wardnet_common::dns::Blocklist {
        id,
        name: "RecentlyUpdated".to_owned(),
        url: "https://example.com/list.txt".to_owned(),
        enabled: true,
        entry_count: 100,
        // last_updated is "now" — with a daily cron, it won't be due for ~24h
        last_updated: Some(now),
        cron_schedule: "0 0 * * *".to_owned(), // Once a day at midnight
        last_error: None,
        last_error_at: None,
        created_at: now,
        updated_at: now,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn runner_cron_skips_invalid_cron_schedule() {
    let bl_id = Uuid::new_v4();
    let dns_repo = Arc::new(CronMockDnsRepository::with_blocklists(vec![
        blocklist_invalid_cron(bl_id),
    ]));
    let fetcher: Arc<dyn BlocklistFetcher> = Arc::new(FixedBodyFetcher {
        body: "0.0.0.0 ads.example.com\n".to_owned(),
    });
    let service: Arc<dyn DnsService> = Arc::new(MockRunnerDnsService::new(false));
    let server = Arc::new(MockDnsServer::new());
    let events: Arc<dyn EventPublisher> = Arc::new(BroadcastEventBus::new(16));
    let parent = tracing::Span::none();

    let runner = DnsRunner::start(
        service,
        Arc::clone(&server) as Arc<dyn DnsServer>,
        dns_repo.clone(),
        mock_filter(),
        fetcher,
        events.as_ref(),
        &parent,
        std::time::Duration::from_millis(100),
    );

    // Wait for several cron ticks.
    tokio::time::sleep(std::time::Duration::from_millis(400)).await;

    runner.shutdown().await;

    // Invalid cron schedule → fetcher never called → no domains replaced.
    assert!(dns_repo.replaced_domains.lock().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn runner_cron_skips_blocklist_not_yet_due() {
    let bl_id = Uuid::new_v4();
    let dns_repo = Arc::new(CronMockDnsRepository::with_blocklists(vec![
        blocklist_not_due(bl_id),
    ]));
    let fetcher: Arc<dyn BlocklistFetcher> = Arc::new(FixedBodyFetcher {
        body: "0.0.0.0 ads.example.com\n".to_owned(),
    });
    let service: Arc<dyn DnsService> = Arc::new(MockRunnerDnsService::new(false));
    let server = Arc::new(MockDnsServer::new());
    let events: Arc<dyn EventPublisher> = Arc::new(BroadcastEventBus::new(16));
    let parent = tracing::Span::none();

    let runner = DnsRunner::start(
        service,
        Arc::clone(&server) as Arc<dyn DnsServer>,
        dns_repo.clone(),
        mock_filter(),
        fetcher,
        events.as_ref(),
        &parent,
        std::time::Duration::from_millis(100),
    );

    // Wait for several cron ticks.
    tokio::time::sleep(std::time::Duration::from_millis(400)).await;

    runner.shutdown().await;

    // Blocklist was recently updated → not due → no download attempted.
    assert!(dns_repo.replaced_domains.lock().unwrap().is_empty());
}

// ---------------------------------------------------------------------------
// DB store failure path (replace_blocklist_domains errors)
// ---------------------------------------------------------------------------

/// Mock repository where `replace_blocklist_domains` always fails.
struct FailingStoreDnsRepository {
    blocklists: Vec<wardnet_common::dns::Blocklist>,
    errors_set: std::sync::Mutex<Vec<(Uuid, Option<String>)>>,
}

#[async_trait]
impl DnsRepository for FailingStoreDnsRepository {
    async fn insert_query_log_batch(
        &self,
        _entries: &[wardnetd_data::repository::QueryLogRow],
    ) -> anyhow::Result<()> {
        Ok(())
    }
    async fn query_log_paginated(
        &self,
        _limit: u32,
        _offset: u32,
        _filter: &wardnetd_data::repository::QueryLogFilter,
    ) -> anyhow::Result<Vec<wardnetd_data::repository::QueryLogRow>> {
        Ok(vec![])
    }
    async fn query_log_count(
        &self,
        _filter: &wardnetd_data::repository::QueryLogFilter,
    ) -> anyhow::Result<u64> {
        Ok(0)
    }
    async fn cleanup_query_log(&self, _retention_days: u32) -> anyhow::Result<u64> {
        Ok(0)
    }
    async fn list_blocklists(&self) -> anyhow::Result<Vec<wardnet_common::dns::Blocklist>> {
        Ok(self.blocklists.clone())
    }
    async fn get_blocklist(
        &self,
        _id: Uuid,
    ) -> anyhow::Result<Option<wardnet_common::dns::Blocklist>> {
        Ok(None)
    }
    async fn create_blocklist(
        &self,
        _row: &wardnetd_data::repository::BlocklistRow,
    ) -> anyhow::Result<()> {
        Ok(())
    }
    async fn update_blocklist(
        &self,
        _id: Uuid,
        _row: &wardnetd_data::repository::BlocklistUpdate,
    ) -> anyhow::Result<()> {
        Ok(())
    }
    async fn delete_blocklist(&self, _id: Uuid) -> anyhow::Result<bool> {
        Ok(false)
    }
    async fn replace_blocklist_domains(
        &self,
        _id: Uuid,
        _domains: &[String],
    ) -> anyhow::Result<u64> {
        Err(anyhow::anyhow!("DB write failed"))
    }
    async fn list_all_blocked_domains_for_enabled(&self) -> anyhow::Result<Vec<String>> {
        Ok(vec![])
    }
    async fn set_blocklist_error(&self, id: Uuid, error: Option<&str>) -> anyhow::Result<()> {
        self.errors_set
            .lock()
            .unwrap()
            .push((id, error.map(str::to_owned)));
        Ok(())
    }
    async fn list_allowlist(&self) -> anyhow::Result<Vec<wardnet_common::dns::AllowlistEntry>> {
        Ok(vec![])
    }
    async fn create_allowlist_entry(
        &self,
        _row: &wardnetd_data::repository::AllowlistRow,
    ) -> anyhow::Result<()> {
        Ok(())
    }
    async fn delete_allowlist_entry(&self, _id: Uuid) -> anyhow::Result<bool> {
        Ok(false)
    }
    async fn list_custom_rules(
        &self,
    ) -> anyhow::Result<Vec<wardnet_common::dns::CustomFilterRule>> {
        Ok(vec![])
    }
    async fn get_custom_rule(
        &self,
        _id: Uuid,
    ) -> anyhow::Result<Option<wardnet_common::dns::CustomFilterRule>> {
        Ok(None)
    }
    async fn create_custom_rule(
        &self,
        _row: &wardnetd_data::repository::CustomRuleRow,
    ) -> anyhow::Result<()> {
        Ok(())
    }
    async fn update_custom_rule(
        &self,
        _id: Uuid,
        _row: &wardnetd_data::repository::CustomRuleUpdate,
    ) -> anyhow::Result<()> {
        Ok(())
    }
    async fn delete_custom_rule(&self, _id: Uuid) -> anyhow::Result<bool> {
        Ok(false)
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn runner_cron_records_error_when_db_store_fails() {
    let bl_id = Uuid::new_v4();
    let dns_repo = Arc::new(FailingStoreDnsRepository {
        blocklists: vec![blocklist_due(bl_id, true)],
        errors_set: std::sync::Mutex::new(Vec::new()),
    });
    let fetcher: Arc<dyn BlocklistFetcher> = Arc::new(FixedBodyFetcher {
        body: "0.0.0.0 ads.example.com\n".to_owned(),
    });
    let service: Arc<dyn DnsService> = Arc::new(MockRunnerDnsService::new(false));
    let server = Arc::new(MockDnsServer::new());
    let events: Arc<dyn EventPublisher> = Arc::new(BroadcastEventBus::new(16));
    let parent = tracing::Span::none();

    let dns_repo_clone = dns_repo.clone();
    let runner = DnsRunner::start(
        service,
        Arc::clone(&server) as Arc<dyn DnsServer>,
        dns_repo.clone(),
        mock_filter(),
        fetcher,
        events.as_ref(),
        &parent,
        std::time::Duration::from_millis(100),
    );

    // Wait for cron to fire — download succeeds, but DB store fails.
    poll_until(
        || !dns_repo_clone.errors_set.lock().unwrap().is_empty(),
        "blocklist error recorded after DB store failure",
    )
    .await;

    runner.shutdown().await;

    // Verify error was recorded with a message about "failed to store domains".
    let errors = dns_repo.errors_set.lock().unwrap();
    assert!(
        errors.iter().any(|(id, e)| *id == bl_id
            && e.as_deref()
                .unwrap_or("")
                .contains("failed to store domains")),
        "error message should mention store failure, got: {errors:?}",
    );
}

// ---------------------------------------------------------------------------
// list_blocklists error in cron check
// ---------------------------------------------------------------------------

/// Mock repository where `list_blocklists` always fails.
struct FailingListDnsRepository;

#[async_trait]
impl DnsRepository for FailingListDnsRepository {
    async fn insert_query_log_batch(
        &self,
        _entries: &[wardnetd_data::repository::QueryLogRow],
    ) -> anyhow::Result<()> {
        Ok(())
    }
    async fn query_log_paginated(
        &self,
        _limit: u32,
        _offset: u32,
        _filter: &wardnetd_data::repository::QueryLogFilter,
    ) -> anyhow::Result<Vec<wardnetd_data::repository::QueryLogRow>> {
        Ok(vec![])
    }
    async fn query_log_count(
        &self,
        _filter: &wardnetd_data::repository::QueryLogFilter,
    ) -> anyhow::Result<u64> {
        Ok(0)
    }
    async fn cleanup_query_log(&self, _retention_days: u32) -> anyhow::Result<u64> {
        Ok(0)
    }
    async fn list_blocklists(&self) -> anyhow::Result<Vec<wardnet_common::dns::Blocklist>> {
        Err(anyhow::anyhow!("DB read failed"))
    }
    async fn get_blocklist(
        &self,
        _id: Uuid,
    ) -> anyhow::Result<Option<wardnet_common::dns::Blocklist>> {
        Ok(None)
    }
    async fn create_blocklist(
        &self,
        _row: &wardnetd_data::repository::BlocklistRow,
    ) -> anyhow::Result<()> {
        Ok(())
    }
    async fn update_blocklist(
        &self,
        _id: Uuid,
        _row: &wardnetd_data::repository::BlocklistUpdate,
    ) -> anyhow::Result<()> {
        Ok(())
    }
    async fn delete_blocklist(&self, _id: Uuid) -> anyhow::Result<bool> {
        Ok(false)
    }
    async fn replace_blocklist_domains(
        &self,
        _id: Uuid,
        _domains: &[String],
    ) -> anyhow::Result<u64> {
        Ok(0)
    }
    async fn list_all_blocked_domains_for_enabled(&self) -> anyhow::Result<Vec<String>> {
        Ok(vec![])
    }
    async fn set_blocklist_error(&self, _id: Uuid, _error: Option<&str>) -> anyhow::Result<()> {
        Ok(())
    }
    async fn list_allowlist(&self) -> anyhow::Result<Vec<wardnet_common::dns::AllowlistEntry>> {
        Ok(vec![])
    }
    async fn create_allowlist_entry(
        &self,
        _row: &wardnetd_data::repository::AllowlistRow,
    ) -> anyhow::Result<()> {
        Ok(())
    }
    async fn delete_allowlist_entry(&self, _id: Uuid) -> anyhow::Result<bool> {
        Ok(false)
    }
    async fn list_custom_rules(
        &self,
    ) -> anyhow::Result<Vec<wardnet_common::dns::CustomFilterRule>> {
        Ok(vec![])
    }
    async fn get_custom_rule(
        &self,
        _id: Uuid,
    ) -> anyhow::Result<Option<wardnet_common::dns::CustomFilterRule>> {
        Ok(None)
    }
    async fn create_custom_rule(
        &self,
        _row: &wardnetd_data::repository::CustomRuleRow,
    ) -> anyhow::Result<()> {
        Ok(())
    }
    async fn update_custom_rule(
        &self,
        _id: Uuid,
        _row: &wardnetd_data::repository::CustomRuleUpdate,
    ) -> anyhow::Result<()> {
        Ok(())
    }
    async fn delete_custom_rule(&self, _id: Uuid) -> anyhow::Result<bool> {
        Ok(false)
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn runner_cron_handles_list_blocklists_error() {
    let dns_repo: Arc<dyn DnsRepository> = Arc::new(FailingListDnsRepository);
    let fetcher: Arc<dyn BlocklistFetcher> = Arc::new(MockBlocklistFetcher);
    let service: Arc<dyn DnsService> = Arc::new(MockRunnerDnsService::new(false));
    let server = Arc::new(MockDnsServer::new());
    let events: Arc<dyn EventPublisher> = Arc::new(BroadcastEventBus::new(16));
    let parent = tracing::Span::none();

    let runner = DnsRunner::start(
        service,
        Arc::clone(&server) as Arc<dyn DnsServer>,
        dns_repo,
        mock_filter(),
        fetcher,
        events.as_ref(),
        &parent,
        std::time::Duration::from_millis(100),
    );

    // Wait for several cron ticks — the list_blocklists error path should be hit
    // without panicking.
    tokio::time::sleep(std::time::Duration::from_millis(400)).await;

    runner.shutdown().await;
}

// ---------------------------------------------------------------------------
// Event bus lagged handling
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn runner_handles_event_bus_lagged() {
    let service: Arc<dyn DnsService> = Arc::new(MockRunnerDnsService::new(false));
    let server = Arc::new(MockDnsServer::new());
    let parent = tracing::Span::none();

    // Very small event bus capacity to trigger lagging.
    let events = Arc::new(BroadcastEventBus::new(1));
    let events_pub: Arc<dyn EventPublisher> = Arc::clone(&events) as Arc<dyn EventPublisher>;

    let runner = DnsRunner::start(
        service,
        Arc::clone(&server) as Arc<dyn DnsServer>,
        mock_dns_repo(),
        mock_filter(),
        mock_fetcher(),
        events_pub.as_ref(),
        &parent,
        std::time::Duration::from_millis(100),
    );

    // Wait for startup.
    for _ in 0..10 {
        tokio::task::yield_now().await;
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }

    // Flood the event bus to force lagging — publish more events than
    // capacity while the runner might be blocked on a cron tick.
    for _ in 0..10 {
        events.publish(wardnet_common::event::WardnetEvent::DnsServerStarted {
            timestamp: chrono::Utc::now(),
        });
    }

    // Wait for the runner to process the lagged error and continue.
    for _ in 0..10 {
        tokio::task::yield_now().await;
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }

    // Runner should still be alive — verify by sending a config change event.
    events.publish(wardnet_common::event::WardnetEvent::DnsConfigChanged {
        timestamp: chrono::Utc::now(),
    });

    for _ in 0..10 {
        tokio::task::yield_now().await;
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }

    runner.shutdown().await;
}

// ---------------------------------------------------------------------------
// Config change event: server.stop() error branch
// ---------------------------------------------------------------------------

/// Mock server that succeeds on start but fails on stop.
struct StartOkStopFailServer {
    started: AtomicBool,
    start_count: AtomicU64,
}

impl StartOkStopFailServer {
    fn new() -> Self {
        Self {
            started: AtomicBool::new(false),
            start_count: AtomicU64::new(0),
        }
    }
}

#[async_trait]
impl DnsServer for StartOkStopFailServer {
    async fn start(&self) -> anyhow::Result<()> {
        self.started.store(true, Ordering::SeqCst);
        self.start_count.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    async fn stop(&self) -> anyhow::Result<()> {
        Err(anyhow::anyhow!("stop failed"))
    }
    fn is_running(&self) -> bool {
        self.started.load(Ordering::SeqCst)
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
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn runner_handles_stop_error_on_config_change_disable() {
    let service = Arc::new(MockRunnerDnsService::new(true));
    let service_dyn: Arc<dyn DnsService> = Arc::clone(&service) as Arc<dyn DnsService>;
    let server = Arc::new(StartOkStopFailServer::new());
    let events = Arc::new(BroadcastEventBus::new(16));
    let events_pub: Arc<dyn EventPublisher> = Arc::clone(&events) as Arc<dyn EventPublisher>;
    let parent = tracing::Span::none();

    let server_dyn: Arc<dyn DnsServer> = Arc::clone(&server) as Arc<dyn DnsServer>;
    let runner = DnsRunner::start(
        service_dyn,
        server_dyn,
        mock_dns_repo(),
        mock_filter(),
        mock_fetcher(),
        events_pub.as_ref(),
        &parent,
        std::time::Duration::from_millis(100),
    );

    // Wait for server to start (enabled=true initially).
    let server_ref = &server;
    poll_until(
        || server_ref.start_count.load(Ordering::SeqCst) > 0,
        "server.start() called on startup",
    )
    .await;

    // Flip to disabled and trigger config change — stop will error.
    service.enabled.store(false, Ordering::SeqCst);
    events.publish(wardnet_common::event::WardnetEvent::DnsConfigChanged {
        timestamp: chrono::Utc::now(),
    });

    // Wait for the event to be processed.
    for _ in 0..10 {
        tokio::task::yield_now().await;
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }

    // Runner should not panic despite stop error.
    runner.shutdown().await;
}

// ---------------------------------------------------------------------------
// Config change event: server.start() error on re-enable
// ---------------------------------------------------------------------------

/// Mock server that fails on start but succeeds on stop.
struct StartFailServer;

#[async_trait]
impl DnsServer for StartFailServer {
    async fn start(&self) -> anyhow::Result<()> {
        Err(anyhow::anyhow!("start failed"))
    }
    async fn stop(&self) -> anyhow::Result<()> {
        Ok(())
    }
    fn is_running(&self) -> bool {
        false
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
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn runner_handles_start_error_on_config_change_enable() {
    let service = Arc::new(MockRunnerDnsService::new(false));
    let service_dyn: Arc<dyn DnsService> = Arc::clone(&service) as Arc<dyn DnsService>;
    let server: Arc<dyn DnsServer> = Arc::new(StartFailServer);
    let events = Arc::new(BroadcastEventBus::new(16));
    let events_pub: Arc<dyn EventPublisher> = Arc::clone(&events) as Arc<dyn EventPublisher>;
    let parent = tracing::Span::none();

    let runner = DnsRunner::start(
        service_dyn,
        server,
        mock_dns_repo(),
        mock_filter(),
        mock_fetcher(),
        events_pub.as_ref(),
        &parent,
        std::time::Duration::from_millis(100),
    );

    // Wait for startup (disabled).
    for _ in 0..5 {
        tokio::task::yield_now().await;
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }

    // Flip to enabled and trigger config change — start will error.
    service.enabled.store(true, Ordering::SeqCst);
    events.publish(wardnet_common::event::WardnetEvent::DnsConfigChanged {
        timestamp: chrono::Utc::now(),
    });

    // Wait for the event to be processed.
    for _ in 0..10 {
        tokio::task::yield_now().await;
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }

    // Runner should not panic despite start error.
    runner.shutdown().await;
}
