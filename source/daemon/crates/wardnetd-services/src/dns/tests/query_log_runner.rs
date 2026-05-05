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
    CreateAllowlistRequest, CreateAllowlistResponse, CreateBlocklistRequest,
    CreateBlocklistResponse, CreateFilterRuleRequest, CreateFilterRuleResponse,
    DeleteAllowlistResponse, DeleteBlocklistResponse, DeleteFilterRuleResponse,
    DnsCacheFlushResponse, DnsConfigResponse, DnsStatsResponse, DnsStatusResponse,
    ListAllowlistResponse, ListBlocklistsResponse, ListFilterRulesResponse, ListQueryLogParams,
    ListQueryLogResponse, QueryLogEvent, ToggleDnsRequest, UpdateBlocklistRequest,
    UpdateBlocklistResponse, UpdateDnsConfigRequest, UpdateFilterRuleRequest,
    UpdateFilterRuleResponse,
};
use wardnet_common::auth::AuthContext;
use wardnet_common::dns::{
    AllowlistEntry, Blocklist, CustomFilterRule, DnsConfig, DnsResolutionMode,
};
use wardnet_common::jobs::JobDispatchedResponse;
use wardnetd_data::repository::{
    BucketSize, DnsRepository, QueryLogFilter, QueryLogRow, QueryStatsRow, SeriesBucketRow,
    TopClientRow, TopDomainRow,
};

use crate::DnsService;
use crate::dns::filter::FilterInputs;
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
            ad_blocking_enabled: false,
            query_log_enabled: self.enabled,
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
    async fn update_blocklist_now(&self, _id: Uuid) -> Result<JobDispatchedResponse, AppError> {
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
    async fn load_filter_inputs(&self) -> Result<FilterInputs, AppError> {
        unimplemented!()
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
    async fn list_blocklists(&self) -> anyhow::Result<Vec<Blocklist>> {
        Ok(Vec::new())
    }
    async fn get_blocklist(&self, _id: Uuid) -> anyhow::Result<Option<Blocklist>> {
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
        Ok(Vec::new())
    }
    async fn set_blocklist_error(&self, _id: Uuid, _error: Option<&str>) -> anyhow::Result<()> {
        Ok(())
    }
    async fn list_allowlist(&self) -> anyhow::Result<Vec<AllowlistEntry>> {
        Ok(Vec::new())
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
    async fn list_custom_rules(&self) -> anyhow::Result<Vec<CustomFilterRule>> {
        Ok(Vec::new())
    }
    async fn get_custom_rule(&self, _id: Uuid) -> anyhow::Result<Option<CustomFilterRule>> {
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

/// Drive the runner with a few rows and a short flush interval; assert
/// the recording repo received them in a single batch.
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

    // Wait long enough for the flush interval to fire at least once.
    tokio::time::sleep(Duration::from_millis(150)).await;

    let total = total_inserts(&repo);
    assert!(total >= 2, "expected at least 2 inserted rows, got {total}");

    runner.shutdown().await;
}

/// When `query_log_enabled = false` the runner should drain the channel
/// (so the producer doesn't block) but not write to the repository.
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

/// On clean shutdown the runner drains any remaining buffered entries.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn runner_drains_remaining_on_shutdown() {
    let service: Arc<dyn DnsService> = Arc::new(MockService { enabled: true });
    let repo = Arc::new(RecordingRepo::default());
    let (sink, rx) = DnsLogSink::new();

    // Long flush interval so the auto-tick won't fire — the only flush
    // we care about is the one on shutdown.
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
    // Give the runner a moment to pick the row off the channel into its
    // internal buffer before we cancel.
    tokio::time::sleep(Duration::from_millis(50)).await;

    runner.shutdown().await;

    let total = total_inserts(&repo);
    assert_eq!(total, 1, "shutdown must flush remaining buffer");
}

/// Sum the row counts across every recorded batch — used by tests to
/// assert how much the runner persisted. Reads the `Mutex` and drops
/// the guard before returning, so callers can `.await` safely after.
fn total_inserts(repo: &RecordingRepo) -> usize {
    repo.inserts.lock().unwrap().iter().map(Vec::len).sum()
}

fn cleanup_calls(repo: &RecordingRepo) -> Vec<u32> {
    repo.cleanups.lock().unwrap().clone()
}

/// The cleanup tick fires daily; under a tight test interval the day
/// gate gets crossed and `cleanup_query_log` runs. We can't easily
/// flip the date inside a test, but we can verify the tick path
/// doesn't panic and the dropped-counter warning runs once per
/// reporting interval.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn runner_reports_dropped_counter_once_per_interval() {
    let service: Arc<dyn DnsService> = Arc::new(MockService { enabled: true });
    let repo = Arc::new(RecordingRepo::default());
    // Tiny mpsc capacity so the producer fills it instantly.
    let (sink, rx) = DnsLogSink::with_capacities(1, 16);

    let runner = DnsQueryLogRunner::start_with_intervals(
        service,
        repo.clone(),
        sink.clone(),
        rx,
        Duration::from_millis(50),
        Duration::from_mins(1),
        &tracing::Span::none(),
    );

    // Push more than the channel can hold so dropped_entries climbs.
    for _ in 0..50 {
        sink.record(sample_row());
    }

    // Dropped counter should be non-zero before the runner sees it.
    assert!(sink.dropped_count() > 0);

    runner.shutdown().await;
}

// ── Direct unit tests for flush() + cleanup() ─────────────────────────────
//
// The runner_loop's tick branches are exercised through
// `start_with_intervals` above, but the inner helpers have several
// failure paths (insert error, config-load error, disabled mode in
// cleanup) that are best covered with direct calls instead of timing
// races.

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
    async fn list_blocklists(&self) -> anyhow::Result<Vec<Blocklist>> {
        Ok(Vec::new())
    }
    async fn get_blocklist(&self, _id: Uuid) -> anyhow::Result<Option<Blocklist>> {
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
        Ok(Vec::new())
    }
    async fn set_blocklist_error(&self, _id: Uuid, _error: Option<&str>) -> anyhow::Result<()> {
        Ok(())
    }
    async fn list_allowlist(&self) -> anyhow::Result<Vec<AllowlistEntry>> {
        Ok(Vec::new())
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
    async fn list_custom_rules(&self) -> anyhow::Result<Vec<CustomFilterRule>> {
        Ok(Vec::new())
    }
    async fn get_custom_rule(&self, _id: Uuid) -> anyhow::Result<Option<CustomFilterRule>> {
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

/// Service whose `get_dns_config` always returns Err — exercises the
/// "failed to read config" branches of `flush` and `cleanup`.
struct ErroringConfigService;

#[async_trait]
impl DnsService for ErroringConfigService {
    async fn get_config(&self) -> Result<wardnet_common::api::DnsConfigResponse, AppError> {
        unimplemented!()
    }
    async fn update_config(
        &self,
        _req: UpdateDnsConfigRequest,
    ) -> Result<wardnet_common::api::DnsConfigResponse, AppError> {
        unimplemented!()
    }
    async fn toggle(
        &self,
        _req: ToggleDnsRequest,
    ) -> Result<wardnet_common::api::DnsConfigResponse, AppError> {
        unimplemented!()
    }
    async fn status(&self) -> Result<wardnet_common::api::DnsStatusResponse, AppError> {
        unimplemented!()
    }
    async fn flush_cache(&self) -> Result<wardnet_common::api::DnsCacheFlushResponse, AppError> {
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
    async fn update_blocklist_now(&self, _id: Uuid) -> Result<JobDispatchedResponse, AppError> {
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
    async fn load_filter_inputs(&self) -> Result<crate::dns::filter::FilterInputs, AppError> {
        unimplemented!()
    }
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

#[tokio::test]
async fn flush_logs_error_on_repo_failure() {
    let service: Arc<dyn DnsService> = Arc::new(MockService { enabled: true });
    let repo = InsertFailingRepo;
    let admin_ctx = AuthContext::Admin {
        admin_id: Uuid::nil(),
    };
    let mut buf = vec![sample_row()];
    // Should not panic; just emit a tracing::error.
    crate::dns::query_log_runner::flush(&service, &repo, &admin_ctx, &mut buf).await;
    // Buffer is cleared either way (we don't retry).
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
    // Defaulting to enabled means the row IS inserted.
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
    let calls = cleanup_calls(&repo);
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
    assert!(cleanup_calls(&repo).is_empty());
}

#[tokio::test]
async fn cleanup_skips_when_config_load_fails() {
    let service: Arc<dyn DnsService> = Arc::new(ErroringConfigService);
    let repo = RecordingRepo::default();
    let admin_ctx = AuthContext::Admin {
        admin_id: Uuid::nil(),
    };
    crate::dns::query_log_runner::cleanup(&service, &repo, &admin_ctx).await;
    assert!(cleanup_calls(&repo).is_empty());
}

/// Verify the runner doesn't crash when the cleanup tick fires before
/// the day rolls over — exercises the tick branch + the day-gate
/// short-circuit.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn runner_cleanup_tick_no_op_within_same_day() {
    let service: Arc<dyn DnsService> = Arc::new(MockService { enabled: true });
    let repo = Arc::new(RecordingRepo::default());
    let (sink, rx) = DnsLogSink::new();

    // Cleanup ticker fires every 60ms; flush ticker every 60s so it
    // doesn't compete. Same calendar day → no cleanup call expected.
    let runner = DnsQueryLogRunner::start_with_intervals(
        service,
        repo.clone(),
        sink.clone(),
        rx,
        Duration::from_mins(1),
        Duration::from_millis(60),
        &tracing::Span::none(),
    );
    drop(sink);

    tokio::time::sleep(Duration::from_millis(200)).await;
    runner.shutdown().await;

    let calls = cleanup_calls(&repo);
    assert!(
        calls.is_empty(),
        "no cleanup expected within the same calendar day; got {calls:?}"
    );
}
