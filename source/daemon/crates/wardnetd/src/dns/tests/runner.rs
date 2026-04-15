use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use async_trait::async_trait;
use wardnet_types::api::{
    DnsCacheFlushResponse, DnsConfigResponse, DnsStatusResponse, ToggleDnsRequest,
    UpdateDnsConfigRequest,
};
use wardnet_types::dns::{DnsConfig, DnsResolutionMode};

use crate::dns::runner::DnsRunner;
use crate::dns::server::DnsServer;
use crate::error::AppError;
use crate::event::{BroadcastEventBus, EventPublisher};
use crate::service::DnsService;

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
}

// ---------------------------------------------------------------------------
// Helper
// ---------------------------------------------------------------------------

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
    let runner = DnsRunner::start(service, server_dyn, events.as_ref(), &parent);

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
        events.as_ref(),
        &parent,
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
    let runner = DnsRunner::start(service, server_dyn, events.as_ref(), &parent);

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
    let runner = DnsRunner::start(service_dyn, server_dyn, events_pub.as_ref(), &parent);

    // Wait for initial setup (should NOT start since disabled).
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    assert_eq!(
        server.start_count.load(Ordering::SeqCst),
        0,
        "server should not be started initially"
    );

    // Flip enabled to true and publish a config change event.
    service.enabled.store(true, Ordering::SeqCst);
    events.publish(wardnet_types::event::WardnetEvent::DnsConfigChanged {
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
    let runner = DnsRunner::start(service_dyn, server_dyn, events_pub.as_ref(), &parent);

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
    events.publish(wardnet_types::event::WardnetEvent::DnsConfigChanged {
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

    let runner = DnsRunner::start(service, server, events.as_ref(), &parent);

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
    let runner = DnsRunner::start(service, server_dyn, events.as_ref(), &parent);

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
    let runner = DnsRunner::start(service, server, events.as_ref(), &parent);

    // Wait a bit — the error branch should be hit.
    for _ in 0..10 {
        tokio::task::yield_now().await;
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }

    runner.shutdown().await;
}

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
    }

    let service: Arc<dyn DnsService> = Arc::new(ToggleErroringService {
        call_count: AtomicU64::new(0),
    });
    let server = Arc::new(MockDnsServer::new());
    let events: Arc<dyn EventPublisher> = Arc::new(BroadcastEventBus::new(16));
    let parent = tracing::Span::none();

    let server_dyn: Arc<dyn DnsServer> = Arc::clone(&server) as Arc<dyn DnsServer>;
    let runner = DnsRunner::start(service, server_dyn, events.as_ref(), &parent);

    // Wait for startup to complete.
    for _ in 0..10 {
        tokio::task::yield_now().await;
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }

    // Publish a DnsConfigChanged event — the reload will error.
    events.publish(wardnet_types::event::WardnetEvent::DnsConfigChanged {
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
    let runner = DnsRunner::start(service, server_dyn, events.as_ref(), &parent);

    // Startup should finish.
    for _ in 0..5 {
        tokio::task::yield_now().await;
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }

    // Publish an unrelated event.
    events.publish(wardnet_types::event::WardnetEvent::DhcpLeaseAssigned {
        lease_id: uuid::Uuid::new_v4(),
        mac: "aa:bb:cc:dd:ee:ff".to_owned(),
        ip: "192.168.1.50".to_owned(),
        hostname: None,
        timestamp: chrono::Utc::now(),
    });

    // Also publish a DnsServerStarted event (which hits the Ok(_) catch-all).
    events.publish(wardnet_types::event::WardnetEvent::DnsServerStarted {
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
