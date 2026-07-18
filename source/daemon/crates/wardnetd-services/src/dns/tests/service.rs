//! Unit tests for the DNS service — forwarder-selection validation and the
//! config persistence path around it.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use uuid::Uuid;
use wardnet_common::api::{UpdateDnsConfigRequest, UpstreamDnsRequest};
use wardnet_common::auth::AuthContext;
use wardnet_common::dns::DnsProtocol;
use wardnet_common::dns::ForwarderSelectionMode::{self, Failover, Fastest, Single};

use crate::dns::service::{DnsService, DnsServiceImpl, resolve_forwarder_selection};
use crate::error::AppError;
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
    async fn query_log_count(&self, _f: &QueryLogFilter) -> anyhow::Result<u64> {
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

fn admin() -> AuthContext {
    AuthContext::Admin {
        admin_id: Uuid::nil(),
    }
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
