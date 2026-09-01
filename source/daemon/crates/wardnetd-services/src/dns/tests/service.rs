//! Unit tests for the DNS service — forwarder-selection validation and the
//! config persistence path around it.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use wardnet_common::api::{ListQueryLogParams, UpdateDnsConfigRequest, UpstreamDnsRequest};
use wardnet_common::dns::DnsProtocol;
use wardnet_common::dns::ForwarderSelectionMode::{self, Failover, Fastest, Single};

use super::{admin, ts};
use crate::dns::service::{
    DnsService, DnsServiceImpl, FORWARD_DEADLINE_MAX_MS, FORWARD_DEADLINE_MIN_MS,
    QUERY_LOG_MAX_LIMIT, UPSTREAM_TIMEOUT_MAX_MS, UPSTREAM_TIMEOUT_MIN_MS,
    resolve_forwarder_selection, validate_forward_timings,
};
use crate::error::AppError;
use wardnet_common::auth::AuthContext;
use wardnetd_data::repository::{
    DnsRepository, QueryLogFilter, QueryLogRow, SystemConfigRepository,
};

fn upstreams() -> Vec<String> {
    vec![
        "1.1.1.1".to_owned(),
        "8.8.8.8".to_owned(),
        "9.9.9.9".to_owned(),
    ]
}

#[test]
fn non_single_modes_clear_the_selection() {
    // Switching away from Single drops the address, even a stale one.
    for target in [Failover, Fastest] {
        let (mode, single) =
            resolve_forwarder_selection(Single, Some("8.8.8.8"), Some(target), None, &upstreams())
                .unwrap();
        assert_eq!(mode, target);
        assert_eq!(single, None);
    }
}

#[test]
fn selecting_a_listed_server_is_accepted() {
    let (mode, single) =
        resolve_forwarder_selection(Failover, None, Some(Single), Some("9.9.9.9"), &upstreams())
            .unwrap();
    assert_eq!(mode, Single);
    assert_eq!(single.as_deref(), Some("9.9.9.9"));
}

#[test]
fn selecting_an_unlisted_server_is_rejected() {
    let err =
        resolve_forwarder_selection(Failover, None, Some(Single), Some("4.4.4.4"), &upstreams())
            .unwrap_err();
    assert!(
        err.contains("4.4.4.4"),
        "message names the bad address: {err}"
    );
}

#[test]
fn single_mode_without_any_address_is_rejected() {
    let err =
        resolve_forwarder_selection(Failover, None, Some(Single), None, &upstreams()).unwrap_err();
    assert!(err.contains("requires a single_upstream"));
}

#[test]
fn removing_the_selected_server_is_rejected() {
    // Mode unchanged (stays Single via current), request only changes the
    // upstream list to one that no longer contains the selected address.
    let remaining = vec!["1.1.1.1".to_owned(), "9.9.9.9".to_owned()];
    let err =
        resolve_forwarder_selection(Single, Some("8.8.8.8"), None, None, &remaining).unwrap_err();
    assert!(
        err.contains("8.8.8.8"),
        "rejects orphaning the selection: {err}"
    );
}

#[test]
fn keeping_the_selected_server_while_editing_list_is_accepted() {
    let remaining = vec!["8.8.8.8".to_owned(), "9.9.9.9".to_owned()];
    let (mode, single) =
        resolve_forwarder_selection(Single, Some("8.8.8.8"), None, None, &remaining).unwrap();
    assert_eq!(mode, Single);
    assert_eq!(single.as_deref(), Some("8.8.8.8"));
}

/// In-memory `SystemConfigRepository` (KV) — starts empty so `load_config`
/// falls back to defaults.
#[derive(Default)]
struct MemConfig {
    data: Mutex<HashMap<String, String>>,
}
#[async_trait]
impl SystemConfigRepository for MemConfig {
    async fn get(&self, key: &str) -> anyhow::Result<Option<String>> {
        Ok(self.data.lock().unwrap().get(key).cloned())
    }
    async fn set(&self, key: &str, value: &str) -> anyhow::Result<()> {
        self.data
            .lock()
            .unwrap()
            .insert(key.to_owned(), value.to_owned());
        Ok(())
    }
    async fn delete(&self, key: &str) -> anyhow::Result<()> {
        self.data.lock().unwrap().remove(key);
        Ok(())
    }
    async fn device_count(&self) -> anyhow::Result<i64> {
        Ok(0)
    }
    async fn tunnel_count(&self) -> anyhow::Result<i64> {
        Ok(0)
    }
    async fn db_size_bytes(&self) -> anyhow::Result<u64> {
        Ok(0)
    }
}

/// The config path never touches the query log, so the repo is a stub.
struct StubDnsRepo;
#[async_trait]
impl DnsRepository for StubDnsRepo {
    async fn insert_query_log_batch(&self, _e: &[QueryLogRow]) -> anyhow::Result<()> {
        unimplemented!()
    }
    async fn query_log_paginated(
        &self,
        _l: u32,
        _o: u32,
        _f: &QueryLogFilter,
    ) -> anyhow::Result<Vec<QueryLogRow>> {
        unimplemented!()
    }
    async fn cleanup_query_log(&self, _d: u32) -> anyhow::Result<u64> {
        unimplemented!()
    }
}

fn service() -> DnsServiceImpl {
    DnsServiceImpl::new(
        Arc::new(MemConfig::default()),
        Arc::new(StubDnsRepo),
        Arc::new(crate::event::BroadcastEventBus::new(16)),
        None,
    )
}

fn udp(address: &str, name: &str) -> UpstreamDnsRequest {
    UpstreamDnsRequest {
        address: address.to_owned(),
        name: name.to_owned(),
        protocol: DnsProtocol::Udp,
        port: None,
        tls_server_name: None,
    }
}

fn two_servers() -> Vec<UpstreamDnsRequest> {
    vec![udp("1.1.1.1", "CF"), udp("8.8.8.8", "G")]
}

#[tokio::test]
async fn single_mode_persists_and_reloads() {
    let svc = service();
    crate::auth_context::with_context(
        admin(),
        svc.update_config(UpdateDnsConfigRequest {
            upstream_servers: Some(two_servers()),
            forwarder_selection_mode: Some(ForwarderSelectionMode::Single),
            single_upstream: Some("8.8.8.8".to_owned()),
            ..Default::default()
        }),
    )
    .await
    .unwrap();

    let cfg = crate::auth_context::with_context(admin(), svc.get_config())
        .await
        .unwrap()
        .config;
    assert_eq!(cfg.forwarder_selection_mode, ForwarderSelectionMode::Single);
    assert_eq!(cfg.single_upstream.as_deref(), Some("8.8.8.8"));
}

#[tokio::test]
async fn switching_to_failover_clears_single_upstream() {
    let svc = service();
    crate::auth_context::with_context(
        admin(),
        svc.update_config(UpdateDnsConfigRequest {
            upstream_servers: Some(two_servers()),
            forwarder_selection_mode: Some(ForwarderSelectionMode::Single),
            single_upstream: Some("1.1.1.1".to_owned()),
            ..Default::default()
        }),
    )
    .await
    .unwrap();
    crate::auth_context::with_context(
        admin(),
        svc.update_config(UpdateDnsConfigRequest {
            forwarder_selection_mode: Some(ForwarderSelectionMode::Failover),
            ..Default::default()
        }),
    )
    .await
    .unwrap();

    let cfg = crate::auth_context::with_context(admin(), svc.get_config())
        .await
        .unwrap()
        .config;
    assert_eq!(
        cfg.forwarder_selection_mode,
        ForwarderSelectionMode::Failover
    );
    assert_eq!(cfg.single_upstream, None);
}

#[tokio::test]
async fn single_mode_with_unlisted_server_is_rejected() {
    let svc = service();
    crate::auth_context::with_context(
        admin(),
        svc.update_config(UpdateDnsConfigRequest {
            upstream_servers: Some(two_servers()),
            ..Default::default()
        }),
    )
    .await
    .unwrap();
    let err = crate::auth_context::with_context(
        admin(),
        svc.update_config(UpdateDnsConfigRequest {
            forwarder_selection_mode: Some(ForwarderSelectionMode::Single),
            single_upstream: Some("9.9.9.9".to_owned()),
            ..Default::default()
        }),
    )
    .await
    .unwrap_err();
    assert!(matches!(err, AppError::BadRequest(_)));
}

#[tokio::test]
async fn get_dns_config_rejects_anonymous_caller() {
    let svc = service();
    let err = crate::auth_context::with_context(AuthContext::Anonymous, svc.get_dns_config())
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::Forbidden(_)));
}

#[tokio::test]
async fn get_dns_config_allows_admin_caller() {
    let svc = service();
    crate::auth_context::with_context(admin(), svc.get_dns_config())
        .await
        .expect("admin caller can read the DNS config");
}

// ---------------------------------------------------------------------------
// validate_forward_timings (#1199) — the two knobs bounding a forwarded query.
// ---------------------------------------------------------------------------

#[test]
fn the_default_timings_validate() {
    let cfg = wardnet_common::dns::DnsConfig::default();
    assert!(validate_forward_timings(cfg.upstream_timeout_ms, cfg.forward_deadline_ms).is_ok());
}

#[test]
fn each_knob_is_range_checked() {
    assert!(validate_forward_timings(UPSTREAM_TIMEOUT_MIN_MS - 1, 3_500).is_err());
    assert!(validate_forward_timings(UPSTREAM_TIMEOUT_MAX_MS + 1, 14_000).is_err());
    assert!(validate_forward_timings(1_500, FORWARD_DEADLINE_MIN_MS - 1).is_err());
    assert!(validate_forward_timings(1_500, FORWARD_DEADLINE_MAX_MS + 1).is_err());

    assert!(validate_forward_timings(UPSTREAM_TIMEOUT_MIN_MS, FORWARD_DEADLINE_MAX_MS).is_ok());
}

#[test]
fn a_per_upstream_timeout_may_not_exceed_the_whole_query_deadline() {
    // Otherwise the first upstream can consume the entire budget and the
    // ladder never reaches a second one — failover that exists only on paper.
    let err = validate_forward_timings(3_000, 2_000).expect_err("ordering must be enforced");
    assert!(err.contains("must not exceed"), "got: {err}");

    // Equal is fine: one full attempt, and no time for a second, which is a
    // coherent thing for an admin with a single upstream to ask for.
    assert!(validate_forward_timings(2_000, 2_000).is_ok());
}

#[tokio::test]
async fn updating_one_timing_is_validated_against_the_persisted_other() {
    // A partial update must not be able to produce an incoherent pair by
    // moving only one side of it.
    let svc = service();

    let result = crate::auth_context::with_context(
        admin(),
        svc.update_config(UpdateDnsConfigRequest {
            // In range on its own; against the default 3500ms deadline it is not.
            upstream_timeout_ms: Some(5_000),
            ..Default::default()
        }),
    )
    .await;

    assert!(
        matches!(result, Err(AppError::BadRequest(_))),
        "raising one knob past the other must be rejected"
    );

    // And the rejection left nothing behind: validation runs before any write.
    let cfg = crate::auth_context::with_context(admin(), svc.get_config())
        .await
        .unwrap()
        .config;
    assert_eq!(
        cfg.upstream_timeout_ms,
        wardnet_common::dns::DEFAULT_UPSTREAM_TIMEOUT_MS
    );
}

#[tokio::test]
async fn valid_timings_persist_and_reload() {
    let svc = service();

    crate::auth_context::with_context(
        admin(),
        svc.update_config(UpdateDnsConfigRequest {
            upstream_timeout_ms: Some(800),
            forward_deadline_ms: Some(2_400),
            ..Default::default()
        }),
    )
    .await
    .expect("a coherent pair is accepted");

    let cfg = crate::auth_context::with_context(admin(), svc.get_config())
        .await
        .unwrap()
        .config;
    assert_eq!(cfg.upstream_timeout_ms, 800);
    assert_eq!(cfg.forward_deadline_ms, 2_400);
}

/// Serves `available` synthetic rows and records the limit it was asked for,
/// so a test can assert on the over-fetch the service performs.
struct PagingDnsRepo {
    available: usize,
    seen_limit: Mutex<Option<u32>>,
}

impl PagingDnsRepo {
    fn new(available: usize) -> Arc<Self> {
        Arc::new(Self {
            available,
            seen_limit: Mutex::new(None),
        })
    }
}

#[async_trait]
impl DnsRepository for PagingDnsRepo {
    async fn insert_query_log_batch(&self, _e: &[QueryLogRow]) -> anyhow::Result<()> {
        unimplemented!()
    }
    async fn query_log_paginated(
        &self,
        limit: u32,
        _o: u32,
        _f: &QueryLogFilter,
    ) -> anyhow::Result<Vec<QueryLogRow>> {
        *self.seen_limit.lock().unwrap() = Some(limit);
        let n = self.available.min(limit as usize);
        Ok((0..n)
            .map(|i| QueryLogRow {
                timestamp: ts("2026-09-01T00:00:00Z"),
                client_ip: "10.0.0.1".to_owned(),
                domain: format!("d{i}.com"),
                query_type: "A".to_owned(),
                result: "allowed".to_owned(),
                upstream: None,
                latency_ms: 1.0,
                device_id: None,
                protocol: "udp".to_owned(),
            })
            .collect())
    }
    async fn cleanup_query_log(&self, _d: u32) -> anyhow::Result<u64> {
        unimplemented!()
    }
}

fn paging_service(repo: Arc<PagingDnsRepo>) -> DnsServiceImpl {
    DnsServiceImpl::new(
        Arc::new(MemConfig::default()),
        repo,
        Arc::new(crate::event::BroadcastEventBus::new(16)),
        None,
    )
}

async fn page(repo: Arc<PagingDnsRepo>, limit: u32) -> wardnet_common::api::ListQueryLogResponse {
    let svc = paging_service(repo);
    crate::auth_context::with_context(
        admin(),
        svc.list_query_log(ListQueryLogParams {
            limit: Some(limit),
            ..Default::default()
        }),
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn a_full_page_with_nothing_beyond_it_reports_no_more() {
    // The boundary that a naive `entries.len() == limit` check gets wrong.
    let repo = PagingDnsRepo::new(50);
    let res = page(repo, 50).await;
    assert_eq!(res.entries.len(), 50);
    assert!(!res.has_more);
}

#[tokio::test]
async fn one_row_beyond_the_page_reports_more_and_is_not_returned() {
    let repo = PagingDnsRepo::new(51);
    let res = page(repo, 50).await;
    assert_eq!(res.entries.len(), 50, "the over-fetched row is trimmed");
    assert!(res.has_more);
}

#[tokio::test]
async fn a_partial_page_reports_no_more() {
    let repo = PagingDnsRepo::new(7);
    let res = page(repo, 50).await;
    assert_eq!(res.entries.len(), 7);
    assert!(!res.has_more);
}

#[tokio::test]
async fn the_over_fetch_is_applied_after_the_limit_clamp() {
    // Over-fetching before the clamp would let a caller asking for 1000 walk
    // away with 501 rows, quietly raising the cap.
    let repo = PagingDnsRepo::new(10_000);
    let res = page(Arc::clone(&repo), 10_000).await;
    assert_eq!(res.entries.len(), QUERY_LOG_MAX_LIMIT as usize);
    assert!(res.has_more);
    assert_eq!(
        *repo.seen_limit.lock().unwrap(),
        Some(QUERY_LOG_MAX_LIMIT + 1)
    );
}
