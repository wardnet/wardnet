//! Tests for `DnsQueryLogRunner` — drives the runner with a synthetic
//! mpsc and a recording mock service to assert batch flush and
//! "disabled mode drains without inserting" semantics.
//!
//! After C4 the runner goes through the auth-gated [`DnsService`] (not a
//! repository) for both `get_dns_config` and the query-log writes, so the
//! mock service records inserts/cleanups and can be configured to fail.

use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use async_trait::async_trait;
use uuid::Uuid;
use wardnet_common::api::{
    DnsCacheFlushResponse, DnsConfigResponse, DnsStatusResponse, ListQueryLogParams,
    ListQueryLogResponse, QueryLogEvent, ToggleDnsRequest, UpdateDnsConfigRequest,
};
use wardnet_common::auth::AuthContext;
use wardnet_common::dns::{DnsConfig, DnsResolutionMode};
use wardnetd_data::repository::QueryLogRow;

use crate::DnsService;
use crate::dns::log_sink::DnsLogSink;
use crate::dns::query_log_runner::DnsQueryLogRunner;
use crate::error::AppError;

/// Recording mock service. Returns a configurable `DnsConfig` (controls
/// whether the runner inserts or drains-into-void) and captures every batch
/// insert + cleanup call so tests can assert on what the runner did. Optional
/// flags exercise the error branches.
#[derive(Default)]
#[allow(clippy::struct_excessive_bools)] // test toggles, not a domain state machine
struct MockService {
    query_log_enabled: bool,
    /// `get_dns_config` returns `Err` when set.
    config_errors: bool,
    /// `insert_query_log_batch` returns `Err` when set.
    insert_fails: bool,
    /// `cleanup_query_log` returns `Err` when set.
    cleanup_fails: bool,
    inserts: Mutex<Vec<Vec<QueryLogRow>>>,
    cleanups: Mutex<Vec<u32>>,
}

impl MockService {
    fn enabled() -> Self {
        Self {
            query_log_enabled: true,
            ..Self::default()
        }
    }

    fn disabled() -> Self {
        Self {
            query_log_enabled: false,
            ..Self::default()
        }
    }

    fn total_inserts(&self) -> usize {
        self.inserts.lock().unwrap().iter().map(Vec::len).sum()
    }
}

#[async_trait]
impl DnsService for MockService {
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
        if self.config_errors {
            return Err(AppError::Internal(anyhow::anyhow!("config load failed")));
        }
        Ok(DnsConfig {
            enabled: false,
            resolution_mode: DnsResolutionMode::Forwarding,
            upstream_servers: vec![],
            cache_size: 0,
            cache_ttl_min_secs: 0,
            cache_ttl_max_secs: 0,
            dnssec_enabled: false,
            rebinding_protection: false,
            rate_limit_per_second: 0,
            dns_filtering_enabled: false,
            query_log_enabled: self.query_log_enabled,
            query_log_retention_days: 7,
        })
    }
    async fn insert_query_log_batch(&self, entries: &[QueryLogRow]) -> Result<(), AppError> {
        if self.insert_fails {
            return Err(AppError::Internal(anyhow::anyhow!(
                "synthetic insert failure"
            )));
        }
        self.inserts.lock().unwrap().push(entries.to_vec());
        Ok(())
    }
    async fn cleanup_query_log(&self, retention_days: u32) -> Result<u64, AppError> {
        if self.cleanup_fails {
            return Err(AppError::Internal(anyhow::anyhow!(
                "synthetic cleanup failure"
            )));
        }
        self.cleanups.lock().unwrap().push(retention_days);
        Ok(0)
    }
    async fn list_query_log(
        &self,
        _params: ListQueryLogParams,
    ) -> Result<ListQueryLogResponse, AppError> {
        unimplemented!()
    }
    fn subscribe_query_stream(
        &self,
    ) -> Result<tokio::sync::broadcast::Receiver<QueryLogEvent>, AppError> {
        unimplemented!()
    }
    async fn flush_query_log(&self) -> Result<u64, AppError> {
        Ok(0)
    }
}

fn sample_row() -> QueryLogRow {
    QueryLogRow {
        timestamp: "2026-05-05T00:00:00Z".to_owned(),
        client_ip: "10.0.0.1".to_owned(),
        domain: "example.com".to_owned(),
        query_type: "A".to_owned(),
        result: "forwarded".to_owned(),
        upstream: None,
        latency_ms: 1.0,
        device_id: None,
    }
}

fn admin_ctx() -> AuthContext {
    AuthContext::Admin {
        admin_id: Uuid::nil(),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn runner_flushes_buffered_rows_when_enabled() {
    let service = Arc::new(MockService::enabled());
    let service_dyn: Arc<dyn DnsService> = service.clone();
    let (sink, rx) = DnsLogSink::new();

    let runner = DnsQueryLogRunner::start_with_intervals(
        service_dyn,
        sink.clone(),
        rx,
        Duration::from_millis(50),
        Duration::from_mins(1),
        &tracing::Span::none(),
    );

    sink.record(sample_row());
    sink.record(sample_row());
    tokio::time::sleep(Duration::from_millis(150)).await;

    let total = service.total_inserts();
    assert!(total >= 2, "expected at least 2 inserted rows, got {total}");
    runner.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn runner_drains_into_void_when_disabled() {
    let service = Arc::new(MockService::disabled());
    let service_dyn: Arc<dyn DnsService> = service.clone();
    let (sink, rx) = DnsLogSink::new();

    let runner = DnsQueryLogRunner::start_with_intervals(
        service_dyn,
        sink.clone(),
        rx,
        Duration::from_millis(50),
        Duration::from_mins(1),
        &tracing::Span::none(),
    );

    sink.record(sample_row());
    sink.record(sample_row());
    sink.record(sample_row());
    tokio::time::sleep(Duration::from_millis(150)).await;

    let total = service.total_inserts();
    assert_eq!(total, 0, "disabled runner must not insert rows");
    runner.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn runner_drains_remaining_on_shutdown() {
    let service = Arc::new(MockService::enabled());
    let service_dyn: Arc<dyn DnsService> = service.clone();
    let (sink, rx) = DnsLogSink::new();

    let runner = DnsQueryLogRunner::start_with_intervals(
        service_dyn,
        sink.clone(),
        rx,
        Duration::from_mins(1),
        Duration::from_mins(1),
        &tracing::Span::none(),
    );

    sink.record(sample_row());
    tokio::time::sleep(Duration::from_millis(50)).await;
    runner.shutdown().await;

    let total = service.total_inserts();
    assert_eq!(total, 1, "shutdown must flush remaining buffer");
}

#[tokio::test]
async fn flush_no_op_on_empty_buffer() {
    let service: Arc<dyn DnsService> = Arc::new(MockService::enabled());
    let mut buf: Vec<QueryLogRow> = Vec::new();
    crate::dns::query_log_runner::flush(&service, &admin_ctx(), &mut buf).await;
}

#[tokio::test]
async fn flush_logs_error_on_repo_failure() {
    let service: Arc<dyn DnsService> = Arc::new(MockService {
        query_log_enabled: true,
        insert_fails: true,
        ..MockService::default()
    });
    let mut buf = vec![sample_row()];
    crate::dns::query_log_runner::flush(&service, &admin_ctx(), &mut buf).await;
    assert!(buf.is_empty());
}

#[tokio::test]
async fn flush_treats_config_error_as_enabled() {
    let service = Arc::new(MockService {
        config_errors: true,
        ..MockService::default()
    });
    let service_dyn: Arc<dyn DnsService> = service.clone();
    let mut buf = vec![sample_row()];
    crate::dns::query_log_runner::flush(&service_dyn, &admin_ctx(), &mut buf).await;
    assert_eq!(service.total_inserts(), 1);
}

#[tokio::test]
async fn cleanup_runs_when_enabled() {
    let service = Arc::new(MockService::enabled());
    let service_dyn: Arc<dyn DnsService> = service.clone();
    crate::dns::query_log_runner::cleanup(&service_dyn, &admin_ctx()).await;
    let calls = service.cleanups.lock().unwrap().clone();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0], 7);
}

#[tokio::test]
async fn cleanup_skips_when_disabled() {
    let service = Arc::new(MockService::disabled());
    let service_dyn: Arc<dyn DnsService> = service.clone();
    crate::dns::query_log_runner::cleanup(&service_dyn, &admin_ctx()).await;
    assert!(service.cleanups.lock().unwrap().is_empty());
}

#[tokio::test]
async fn cleanup_logs_error_on_repo_failure() {
    let service: Arc<dyn DnsService> = Arc::new(MockService {
        query_log_enabled: true,
        cleanup_fails: true,
        ..MockService::default()
    });
    // Must not panic — the error is logged and swallowed.
    crate::dns::query_log_runner::cleanup(&service, &admin_ctx()).await;
}
