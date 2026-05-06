//! Tests for `DnsQueryLogRunner` — drives the runner with a synthetic
//! mpsc and a recording mock repository to assert batch flush and
//! "disabled mode drains without inserting" semantics.

use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;
use wardnet_common::api::{
    DnsCacheFlushResponse, DnsConfigResponse, DnsStatsResponse, DnsStatusResponse,
    ListQueryLogParams, ListQueryLogResponse, QueryLogEvent, ToggleDnsRequest,
    UpdateDnsConfigRequest,
};
use wardnet_common::auth::AuthContext;
use wardnet_common::dns::{DnsConfig, DnsResolutionMode};
use wardnetd_data::repository::{
    BucketSize, DnsRepository, QueryLogFilter, QueryLogRow, QueryStatsRow, SeriesBucketRow,
    TopClientRow, TopDomainRow,
};

use crate::DnsService;
use crate::dns::log_sink::DnsLogSink;
use crate::dns::query_log_runner::DnsQueryLogRunner;
use crate::error::AppError;

/// Mock service that returns a configurable `DnsConfig` (controls whether
/// the runner inserts or drains-into-void).
struct MockService {
    enabled: bool,
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
            query_log_enabled: self.enabled,
            query_log_retention_days: 7,
        })
    }
    async fn list_query_log(
        &self,
        _params: ListQueryLogParams,
    ) -> Result<ListQueryLogResponse, AppError> {
        unimplemented!()
    }
    async fn dns_stats(&self, _hours: u32) -> Result<DnsStatsResponse, AppError> {
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

/// Recording `DnsRepository` — captures every batch insert + cleanup
/// call so tests can assert on what the runner did.
#[derive(Default)]
struct RecordingRepo {
    inserts: Mutex<Vec<Vec<QueryLogRow>>>,
    cleanups: Mutex<Vec<u32>>,
}

#[async_trait]
impl DnsRepository for RecordingRepo {
    async fn insert_query_log_batch(&self, entries: &[QueryLogRow]) -> anyhow::Result<()> {
        self.inserts.lock().unwrap().push(entries.to_vec());
        Ok(())
    }
    async fn query_log_paginated(
        &self,
        _limit: u32,
        _offset: u32,
        _filter: &QueryLogFilter,
    ) -> anyhow::Result<Vec<QueryLogRow>> {
        Ok(Vec::new())
    }
    async fn query_log_count(&self, _filter: &QueryLogFilter) -> anyhow::Result<u64> {
        Ok(0)
    }
    async fn cleanup_query_log(&self, retention_days: u32) -> anyhow::Result<u64> {
        self.cleanups.lock().unwrap().push(retention_days);
        Ok(0)
    }
    async fn query_stats(&self, _since: DateTime<Utc>) -> anyhow::Result<QueryStatsRow> {
        Ok(QueryStatsRow::default())
    }
    async fn top_domains(
        &self,
        _since: DateTime<Utc>,
        _limit: u32,
        _blocked_only: bool,
    ) -> anyhow::Result<Vec<TopDomainRow>> {
        Ok(Vec::new())
    }
    async fn top_clients(
        &self,
        _since: DateTime<Utc>,
        _limit: u32,
    ) -> anyhow::Result<Vec<TopClientRow>> {
        Ok(Vec::new())
    }
    async fn series_buckets(
        &self,
        _since: DateTime<Utc>,
        _bucket: BucketSize,
    ) -> anyhow::Result<Vec<SeriesBucketRow>> {
        Ok(Vec::new())
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn runner_flushes_buffered_rows_when_enabled() {
    let service: Arc<dyn DnsService> = Arc::new(MockService { enabled: true });
    let repo = Arc::new(RecordingRepo::default());
    let (sink, rx) = DnsLogSink::new();

    let runner = DnsQueryLogRunner::start_with_intervals(
        service,
        repo.clone(),
        sink.clone(),
        rx,
        Duration::from_millis(50),
        Duration::from_mins(1),
        &tracing::Span::none(),
    );

    sink.record(sample_row());
    sink.record(sample_row());
    tokio::time::sleep(Duration::from_millis(150)).await;

    let total = total_inserts(&repo);
    assert!(total >= 2, "expected at least 2 inserted rows, got {total}");
    runner.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn runner_drains_into_void_when_disabled() {
    let service: Arc<dyn DnsService> = Arc::new(MockService { enabled: false });
    let repo = Arc::new(RecordingRepo::default());
    let (sink, rx) = DnsLogSink::new();

    let runner = DnsQueryLogRunner::start_with_intervals(
        service,
        repo.clone(),
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

    let total = total_inserts(&repo);
    assert_eq!(total, 0, "disabled runner must not insert rows");
    runner.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn runner_drains_remaining_on_shutdown() {
    let service: Arc<dyn DnsService> = Arc::new(MockService { enabled: true });
    let repo = Arc::new(RecordingRepo::default());
    let (sink, rx) = DnsLogSink::new();

    let runner = DnsQueryLogRunner::start_with_intervals(
        service,
        repo.clone(),
        sink.clone(),
        rx,
        Duration::from_mins(1),
        Duration::from_mins(1),
        &tracing::Span::none(),
    );

    sink.record(sample_row());
    tokio::time::sleep(Duration::from_millis(50)).await;
    runner.shutdown().await;

    let total = total_inserts(&repo);
    assert_eq!(total, 1, "shutdown must flush remaining buffer");
}

fn total_inserts(repo: &RecordingRepo) -> usize {
    repo.inserts.lock().unwrap().iter().map(Vec::len).sum()
}

#[tokio::test]
async fn flush_no_op_on_empty_buffer() {
    let service: Arc<dyn DnsService> = Arc::new(MockService { enabled: true });
    let repo = RecordingRepo::default();
    let admin_ctx = AuthContext::Admin {
        admin_id: Uuid::nil(),
    };
    let mut buf: Vec<QueryLogRow> = Vec::new();
    crate::dns::query_log_runner::flush(&service, &repo, &admin_ctx, &mut buf).await;
    assert!(repo.inserts.lock().unwrap().is_empty());
}

/// Repo whose `insert_query_log_batch` always returns Err.
struct InsertFailingRepo;

#[async_trait]
impl DnsRepository for InsertFailingRepo {
    async fn insert_query_log_batch(&self, _entries: &[QueryLogRow]) -> anyhow::Result<()> {
        Err(anyhow::anyhow!("synthetic insert failure"))
    }
    async fn query_log_paginated(
        &self,
        _limit: u32,
        _offset: u32,
        _filter: &QueryLogFilter,
    ) -> anyhow::Result<Vec<QueryLogRow>> {
        Ok(Vec::new())
    }
    async fn query_log_count(&self, _filter: &QueryLogFilter) -> anyhow::Result<u64> {
        Ok(0)
    }
    async fn cleanup_query_log(&self, _retention_days: u32) -> anyhow::Result<u64> {
        Ok(0)
    }
    async fn query_stats(&self, _since: DateTime<Utc>) -> anyhow::Result<QueryStatsRow> {
        Ok(QueryStatsRow::default())
    }
    async fn top_domains(
        &self,
        _since: DateTime<Utc>,
        _limit: u32,
        _blocked_only: bool,
    ) -> anyhow::Result<Vec<TopDomainRow>> {
        Ok(Vec::new())
    }
    async fn top_clients(
        &self,
        _since: DateTime<Utc>,
        _limit: u32,
    ) -> anyhow::Result<Vec<TopClientRow>> {
        Ok(Vec::new())
    }
    async fn series_buckets(
        &self,
        _since: DateTime<Utc>,
        _bucket: BucketSize,
    ) -> anyhow::Result<Vec<SeriesBucketRow>> {
        Ok(Vec::new())
    }
}

/// Service whose `get_dns_config` always returns Err — exercises the
/// "failed to read config" branches of `flush` and `cleanup`.
struct ErroringConfigService;

#[async_trait]
impl DnsService for ErroringConfigService {
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
    async fn list_query_log(
        &self,
        _params: ListQueryLogParams,
    ) -> Result<ListQueryLogResponse, AppError> {
        unimplemented!()
    }
    async fn dns_stats(&self, _hours: u32) -> Result<DnsStatsResponse, AppError> {
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

#[tokio::test]
async fn flush_logs_error_on_repo_failure() {
    let service: Arc<dyn DnsService> = Arc::new(MockService { enabled: true });
    let repo = InsertFailingRepo;
    let admin_ctx = AuthContext::Admin {
        admin_id: Uuid::nil(),
    };
    let mut buf = vec![sample_row()];
    crate::dns::query_log_runner::flush(&service, &repo, &admin_ctx, &mut buf).await;
    assert!(buf.is_empty());
}

#[tokio::test]
async fn flush_treats_config_error_as_enabled() {
    let service: Arc<dyn DnsService> = Arc::new(ErroringConfigService);
    let repo = RecordingRepo::default();
    let admin_ctx = AuthContext::Admin {
        admin_id: Uuid::nil(),
    };
    let mut buf = vec![sample_row()];
    crate::dns::query_log_runner::flush(&service, &repo, &admin_ctx, &mut buf).await;
    let total = total_inserts(&repo);
    assert_eq!(total, 1);
}

#[tokio::test]
async fn cleanup_runs_when_enabled() {
    let service: Arc<dyn DnsService> = Arc::new(MockService { enabled: true });
    let repo = RecordingRepo::default();
    let admin_ctx = AuthContext::Admin {
        admin_id: Uuid::nil(),
    };
    crate::dns::query_log_runner::cleanup(&service, &repo, &admin_ctx).await;
    let calls = repo.cleanups.lock().unwrap().clone();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0], 7);
}

#[tokio::test]
async fn cleanup_skips_when_disabled() {
    let service: Arc<dyn DnsService> = Arc::new(MockService { enabled: false });
    let repo = RecordingRepo::default();
    let admin_ctx = AuthContext::Admin {
        admin_id: Uuid::nil(),
    };
    crate::dns::query_log_runner::cleanup(&service, &repo, &admin_ctx).await;
    assert!(repo.cleanups.lock().unwrap().is_empty());
}
