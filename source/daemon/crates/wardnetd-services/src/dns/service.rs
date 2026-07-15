//! DNS server config + status + cache flush + query log.
//!
//! After issue #221 (Stage 7), every filter-source concern (blocklists,
//! allowlist, custom rules, per-device settings) lives behind
//! [`crate::dns_filter::DnsFilterService`]. This service is left with the
//! DNS server lifecycle and query log observability.
//!
//! DNS stats (totals, top domains, top clients, time series) moved to the
//! generic stats subsystem in issue #409. Use `StatsService` + the
//! `/api/stats` and `/api/stats/top` endpoints instead.

use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::broadcast;
use uuid::Uuid;
use wardnet_common::api::{
    DnsCacheFlushResponse, DnsConfigResponse, DnsStatusResponse, ListQueryLogParams,
    ListQueryLogResponse, QueryLogEvent, ToggleDnsRequest, UpdateDnsConfigRequest,
};
use wardnet_common::dns::{
    DnsConfig, DnsProtocol, DnsQueryLogEntry, DnsQueryResult, DnsResolutionMode,
    ForwarderSelectionMode, UpstreamDns,
};
use wardnet_common::event::WardnetEvent;

use crate::auth_context;
use crate::dns::log_sink::DnsLogSink;
use crate::error::AppError;
use crate::event::EventPublisher;
use wardnetd_data::repository::{
    DnsRepository, QueryLogFilter, QueryLogRow, SystemConfigRepository,
};

pub const QUERY_LOG_MAX_LIMIT: u32 = 500;
pub const QUERY_LOG_DEFAULT_LIMIT: u32 = 50;
pub const QUERY_LOG_RETENTION_MIN_DAYS: u32 = 1;
pub const QUERY_LOG_RETENTION_MAX_DAYS: u32 = 30;

/// `system_config` key for the DNS server enable flag ("true"/"false",
/// absent means off). Shared with the DHCP service, which advertises the Pi
/// as the clients' resolver only while this is set — the two readers must
/// never drift.
pub(crate) const DNS_ENABLED_KEY: &str = "dns_enabled";

#[async_trait]
pub trait DnsService: Send + Sync {
    async fn get_config(&self) -> Result<DnsConfigResponse, AppError>;
    async fn update_config(
        &self,
        req: UpdateDnsConfigRequest,
    ) -> Result<DnsConfigResponse, AppError>;
    async fn toggle(&self, req: ToggleDnsRequest) -> Result<DnsConfigResponse, AppError>;
    async fn status(&self) -> Result<DnsStatusResponse, AppError>;
    async fn flush_cache(&self) -> Result<DnsCacheFlushResponse, AppError>;
    async fn list_query_log(
        &self,
        params: ListQueryLogParams,
    ) -> Result<ListQueryLogResponse, AppError>;
    fn subscribe_query_stream(&self) -> Result<broadcast::Receiver<QueryLogEvent>, AppError>;
    async fn flush_query_log(&self) -> Result<u64, AppError>;

    /// Internal: load the DNS server runtime config (called by the runner).
    async fn get_dns_config(&self) -> Result<DnsConfig, AppError>;

    /// Internal: persist a batch of query-log rows (called by the
    /// `DnsQueryLogRunner` under an admin auth context).
    async fn insert_query_log_batch(&self, entries: &[QueryLogRow]) -> Result<(), AppError>;

    /// Internal: delete query-log rows older than `retention_days`, returning
    /// the number of rows removed (called by the `DnsQueryLogRunner` under an
    /// admin auth context).
    async fn cleanup_query_log(&self, retention_days: u32) -> Result<u64, AppError>;
}

pub struct DnsServiceImpl {
    system_config: Arc<dyn SystemConfigRepository>,
    dns_repo: Arc<dyn DnsRepository>,
    events: Arc<dyn EventPublisher>,
    log_sink: Option<Arc<DnsLogSink>>,
}

impl DnsServiceImpl {
    pub fn new(
        system_config: Arc<dyn SystemConfigRepository>,
        dns_repo: Arc<dyn DnsRepository>,
        events: Arc<dyn EventPublisher>,
        log_sink: Option<Arc<DnsLogSink>>,
    ) -> Self {
        Self {
            system_config,
            dns_repo,
            events,
            log_sink,
        }
    }

    async fn load_config(&self) -> Result<DnsConfig, AppError> {
        let get = |key: &str| {
            let sc = Arc::clone(&self.system_config);
            let key = key.to_owned();
            async move { sc.get(&key).await.map_err(AppError::Internal) }
        };

        let enabled = get(DNS_ENABLED_KEY)
            .await?
            .unwrap_or_else(|| "false".to_owned())
            == "true";

        let resolution_mode = match get("dns_resolution_mode")
            .await?
            .unwrap_or_else(|| "forwarding".to_owned())
            .as_str()
        {
            "recursive" => DnsResolutionMode::Recursive,
            _ => DnsResolutionMode::Forwarding,
        };

        let upstream_json = get("dns_upstream_servers")
            .await?
            .unwrap_or_else(|| "[]".to_owned());
        let upstream_servers: Vec<UpstreamDns> =
            serde_json::from_str(&upstream_json).map_err(|e| {
                AppError::Internal(anyhow::anyhow!("invalid dns_upstream_servers: {e}"))
            })?;

        let cache_size = Self::parse_u32(get("dns_cache_size").await?, 10_000)?;
        let cache_ttl_min_secs = Self::parse_u32(get("dns_cache_ttl_min_secs").await?, 0)?;
        let cache_ttl_max_secs = Self::parse_u32(get("dns_cache_ttl_max_secs").await?, 86_400)?;
        let dnssec_enabled = get("dns_dnssec_enabled")
            .await?
            .unwrap_or_else(|| "false".to_owned())
            == "true";
        let rebinding_protection = get("dns_rebinding_protection")
            .await?
            .unwrap_or_else(|| "true".to_owned())
            == "true";
        let rate_limit_per_second = Self::parse_u32(get("dns_rate_limit_per_second").await?, 0)?;
        let dns_filtering_enabled = get("dns_filtering_enabled")
            .await?
            .unwrap_or_else(|| "true".to_owned())
            == "true";
        let query_log_enabled = get("dns_query_log_enabled")
            .await?
            .unwrap_or_else(|| "true".to_owned())
            == "true";
        let query_log_retention_days =
            Self::parse_u32(get("dns_query_log_retention_days").await?, 7)?;

        let forwarder_selection_mode = ForwarderSelectionMode::from_wire(
            &get("dns_forwarder_selection_mode")
                .await?
                .unwrap_or_default(),
        );
        // The single-server address is only meaningful in `single` mode; drop a
        // stale value in the other modes so the loaded config stays consistent.
        let single_upstream = match forwarder_selection_mode {
            ForwarderSelectionMode::Single => get("dns_single_upstream").await?,
            ForwarderSelectionMode::Failover | ForwarderSelectionMode::Fastest => None,
        };

        Ok(DnsConfig {
            enabled,
            resolution_mode,
            upstream_servers,
            forwarder_selection_mode,
            single_upstream,
            cache_size,
            cache_ttl_min_secs,
            cache_ttl_max_secs,
            dnssec_enabled,
            rebinding_protection,
            rate_limit_per_second,
            dns_filtering_enabled,
            query_log_enabled,
            query_log_retention_days,
        })
    }

    fn parse_u32(val: Option<String>, default: u32) -> Result<u32, AppError> {
        val.unwrap_or_else(|| default.to_string())
            .parse()
            .map_err(|e| AppError::Internal(anyhow::anyhow!("invalid u32 config value: {e}")))
    }

    fn publish_config_changed(&self) {
        self.events.publish(WardnetEvent::DnsConfigChanged {
            timestamp: chrono::Utc::now(),
        });
    }
}

/// Resolve the forwarder selection (mode + single-server address) an update
/// results in, and validate the selection against the upstream list the update
/// produces.
///
/// Pure so it can be unit-tested without the full service. In `Failover` and
/// `Fastest` the single-server address is always cleared. In `Single` the
/// effective address is the request's (or the current one, if the request left
/// it unchanged) and MUST be present in `effective_upstreams` — this is what
/// rejects both selecting an unknown server and removing the currently-selected
/// server. `Err` carries a human message for a `400` response.
fn resolve_forwarder_selection(
    current_mode: ForwarderSelectionMode,
    current_single: Option<&str>,
    req_mode: Option<ForwarderSelectionMode>,
    req_single: Option<&str>,
    effective_upstreams: &[String],
) -> Result<(ForwarderSelectionMode, Option<String>), String> {
    let mode = req_mode.unwrap_or(current_mode);
    match mode {
        ForwarderSelectionMode::Failover | ForwarderSelectionMode::Fastest => Ok((mode, None)),
        ForwarderSelectionMode::Single => {
            let addr = req_single.or(current_single).map(str::to_owned);
            match addr {
                Some(a) if effective_upstreams.iter().any(|u| u == &a) => Ok((mode, Some(a))),
                Some(a) => Err(format!(
                    "selected upstream '{a}' is not one of the configured upstream servers"
                )),
                None => Err("single-server mode requires a single_upstream address".to_owned()),
            }
        }
    }
}

#[async_trait]
impl DnsService for DnsServiceImpl {
    async fn get_config(&self) -> Result<DnsConfigResponse, AppError> {
        auth_context::require_admin()?;
        let config = self.load_config().await?;
        Ok(DnsConfigResponse { config })
    }

    #[allow(clippy::too_many_lines)]
    async fn update_config(
        &self,
        req: UpdateDnsConfigRequest,
    ) -> Result<DnsConfigResponse, AppError> {
        auth_context::require_admin()?;

        if let Some(days) = req.query_log_retention_days
            && !(QUERY_LOG_RETENTION_MIN_DAYS..=QUERY_LOG_RETENTION_MAX_DAYS).contains(&days)
        {
            return Err(AppError::BadRequest(format!(
                "query_log_retention_days must be between {QUERY_LOG_RETENTION_MIN_DAYS} and {QUERY_LOG_RETENTION_MAX_DAYS}"
            )));
        }

        // ---- Validation phase ----
        // All rejections happen here, BEFORE any `system_config` write, so a
        // rejected request never leaves the persisted config half-mutated
        // (each `set`/`delete` is an independent, non-transactional write).

        // DoT/DoH need a hostname SNI for certificate validation; reject
        // encrypted upstreams that omit it or give a value that can't be a cert
        // hostname (URL, IP literal, whitespace) — those would only surface as
        // opaque per-query handshake failures, not a clear config error.
        if let Some(ref servers) = req.upstream_servers {
            for s in servers {
                if matches!(s.protocol, DnsProtocol::Tls | DnsProtocol::Https) {
                    let sni = s.tls_server_name.as_deref().map_or("", str::trim);
                    let is_hostname = !sni.is_empty()
                        && !sni.contains(|c: char| c.is_whitespace() || c == '/')
                        && sni.parse::<std::net::IpAddr>().is_err();
                    if !is_hostname {
                        return Err(AppError::BadRequest(format!(
                            "upstream '{}' uses {:?} and requires a valid TLS server name (a hostname)",
                            s.name, s.protocol
                        )));
                    }
                }
            }
        }

        // Forwarder selection (pin/auto). Resolve the effective mode + pinned
        // address this update results in and validate it against the resulting
        // upstream list. Computed here — before any write — so that pinning an
        // unlisted server, or removing the currently-pinned one, is rejected
        // without having already persisted the new upstream list.
        let forwarder_decision = if req.forwarder_selection_mode.is_some()
            || req.single_upstream.is_some()
            || req.upstream_servers.is_some()
        {
            let current = self.load_config().await?;
            // Addresses the config will have after this update.
            let effective_upstreams: Vec<String> = req.upstream_servers.as_ref().map_or_else(
                || {
                    current
                        .upstream_servers
                        .iter()
                        .map(|u| u.address.clone())
                        .collect()
                },
                |servers| servers.iter().map(|s| s.address.clone()).collect(),
            );
            Some(
                resolve_forwarder_selection(
                    current.forwarder_selection_mode,
                    current.single_upstream.as_deref(),
                    req.forwarder_selection_mode,
                    req.single_upstream.as_deref(),
                    &effective_upstreams,
                )
                .map_err(AppError::BadRequest)?,
            )
        } else {
            None
        };

        // ---- Write phase ---- (every value below is already validated)
        if let Some(mode) = req.resolution_mode {
            self.system_config
                .set("dns_resolution_mode", mode.as_str())
                .await
                .map_err(AppError::Internal)?;
        }
        if let Some(ref servers) = req.upstream_servers {
            let upstream: Vec<UpstreamDns> = servers
                .iter()
                .map(|s| UpstreamDns {
                    address: s.address.clone(),
                    name: s.name.clone(),
                    protocol: s.protocol,
                    port: s.port,
                    tls_server_name: s.tls_server_name.clone(),
                })
                .collect();
            let json = serde_json::to_string(&upstream)
                .map_err(|e| AppError::Internal(anyhow::anyhow!("serialize upstreams: {e}")))?;
            self.system_config
                .set("dns_upstream_servers", &json)
                .await
                .map_err(AppError::Internal)?;
        }
        if let Some((mode, single)) = forwarder_decision {
            self.system_config
                .set("dns_forwarder_selection_mode", mode.as_str())
                .await
                .map_err(AppError::Internal)?;
            match single {
                Some(a) => self
                    .system_config
                    .set("dns_single_upstream", &a)
                    .await
                    .map_err(AppError::Internal)?,
                None => self
                    .system_config
                    .delete("dns_single_upstream")
                    .await
                    .map_err(AppError::Internal)?,
            }
        }
        if let Some(v) = req.cache_size {
            self.system_config
                .set("dns_cache_size", &v.to_string())
                .await
                .map_err(AppError::Internal)?;
        }
        if let Some(v) = req.cache_ttl_min_secs {
            self.system_config
                .set("dns_cache_ttl_min_secs", &v.to_string())
                .await
                .map_err(AppError::Internal)?;
        }
        if let Some(v) = req.cache_ttl_max_secs {
            self.system_config
                .set("dns_cache_ttl_max_secs", &v.to_string())
                .await
                .map_err(AppError::Internal)?;
        }
        if let Some(v) = req.dnssec_enabled {
            self.system_config
                .set("dns_dnssec_enabled", if v { "true" } else { "false" })
                .await
                .map_err(AppError::Internal)?;
        }
        if let Some(v) = req.rebinding_protection {
            self.system_config
                .set("dns_rebinding_protection", if v { "true" } else { "false" })
                .await
                .map_err(AppError::Internal)?;
        }
        if let Some(v) = req.rate_limit_per_second {
            self.system_config
                .set("dns_rate_limit_per_second", &v.to_string())
                .await
                .map_err(AppError::Internal)?;
        }
        if let Some(v) = req.dns_filtering_enabled {
            self.system_config
                .set("dns_filtering_enabled", if v { "true" } else { "false" })
                .await
                .map_err(AppError::Internal)?;
            self.events.publish(WardnetEvent::DnsFilterChanged {
                change: wardnet_common::event::DnsFilterChange::GlobalToggle,
                timestamp: chrono::Utc::now(),
            });
        }
        if let Some(v) = req.query_log_enabled {
            self.system_config
                .set("dns_query_log_enabled", if v { "true" } else { "false" })
                .await
                .map_err(AppError::Internal)?;
        }
        if let Some(v) = req.query_log_retention_days {
            self.system_config
                .set("dns_query_log_retention_days", &v.to_string())
                .await
                .map_err(AppError::Internal)?;
        }

        self.publish_config_changed();
        let config = self.load_config().await?;
        Ok(DnsConfigResponse { config })
    }

    async fn toggle(&self, req: ToggleDnsRequest) -> Result<DnsConfigResponse, AppError> {
        auth_context::require_admin()?;
        self.system_config
            .set(DNS_ENABLED_KEY, if req.enabled { "true" } else { "false" })
            .await
            .map_err(AppError::Internal)?;
        self.publish_config_changed();
        let config = self.load_config().await?;
        Ok(DnsConfigResponse { config })
    }

    async fn status(&self) -> Result<DnsStatusResponse, AppError> {
        auth_context::require_admin()?;
        let config = self.load_config().await?;
        Ok(DnsStatusResponse {
            enabled: config.enabled,
            running: false,
            cache_size: 0,
            cache_capacity: config.cache_size,
            cache_hit_rate: 0.0,
            // Live per-upstream latency comes from the server/prober; the API
            // status handler populates it. This service-level path is a
            // placeholder that never carries telemetry.
            upstream_latencies: Vec::new(),
        })
    }

    async fn flush_cache(&self) -> Result<DnsCacheFlushResponse, AppError> {
        auth_context::require_admin()?;
        Ok(DnsCacheFlushResponse {
            message: "Cache flushed".to_owned(),
            entries_cleared: 0,
        })
    }

    async fn get_dns_config(&self) -> Result<DnsConfig, AppError> {
        self.load_config().await
    }

    async fn insert_query_log_batch(&self, entries: &[QueryLogRow]) -> Result<(), AppError> {
        auth_context::require_admin()?;
        self.dns_repo
            .insert_query_log_batch(entries)
            .await
            .map_err(AppError::Internal)
    }

    async fn cleanup_query_log(&self, retention_days: u32) -> Result<u64, AppError> {
        auth_context::require_admin()?;
        self.dns_repo
            .cleanup_query_log(retention_days)
            .await
            .map_err(AppError::Internal)
    }

    async fn list_query_log(
        &self,
        params: ListQueryLogParams,
    ) -> Result<ListQueryLogResponse, AppError> {
        auth_context::require_admin()?;

        let limit = params
            .limit
            .unwrap_or(QUERY_LOG_DEFAULT_LIMIT)
            .clamp(1, QUERY_LOG_MAX_LIMIT);
        let offset = params.offset.unwrap_or(0);

        let filter = QueryLogFilter {
            client_ip: params.client_ip,
            device_id: params.device_id.map(|id| id.to_string()),
            domain: params.domain,
            result: params.result, // already Option<DnsQueryResult>
        };

        let rows = self
            .dns_repo
            .query_log_paginated(limit, offset, &filter)
            .await
            .map_err(AppError::Internal)?;
        let total = self
            .dns_repo
            .query_log_count(&filter)
            .await
            .map_err(AppError::Internal)?;

        let entries: Vec<DnsQueryLogEntry> = rows
            .into_iter()
            .filter_map(|row| {
                let timestamp = parse_iso_timestamp(&row.timestamp).ok()?;
                let device_id = row
                    .device_id
                    .as_deref()
                    .and_then(|s| Uuid::parse_str(s).ok());
                Some(DnsQueryLogEntry {
                    id: 0,
                    timestamp,
                    client_ip: row.client_ip,
                    domain: row.domain,
                    query_type: row.query_type,
                    result: DnsQueryResult::parse(&row.result),
                    upstream: row.upstream,
                    latency_ms: row.latency_ms,
                    device_id,
                })
            })
            .collect();

        Ok(ListQueryLogResponse { entries, total })
    }

    fn subscribe_query_stream(&self) -> Result<broadcast::Receiver<QueryLogEvent>, AppError> {
        auth_context::require_admin()?;
        match &self.log_sink {
            Some(sink) => Ok(sink.subscribe()),
            None => Err(AppError::Internal(anyhow::anyhow!(
                "DNS log sink not initialized"
            ))),
        }
    }

    async fn flush_query_log(&self) -> Result<u64, AppError> {
        auth_context::require_admin()?;
        Ok(0)
    }
}

fn parse_iso_timestamp(s: &str) -> anyhow::Result<chrono::DateTime<chrono::Utc>> {
    use chrono::NaiveDateTime;
    let naive = NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%SZ")?;
    Ok(chrono::TimeZone::from_utc_datetime(&chrono::Utc, &naive))
}

#[cfg(test)]
mod forwarder_selection_tests {
    use super::resolve_forwarder_selection;
    use wardnet_common::dns::ForwarderSelectionMode::{Failover, Fastest, Single};

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
            let (mode, single) = resolve_forwarder_selection(
                Single,
                Some("8.8.8.8"),
                Some(target),
                None,
                &upstreams(),
            )
            .unwrap();
            assert_eq!(mode, target);
            assert_eq!(single, None);
        }
    }

    #[test]
    fn selecting_a_listed_server_is_accepted() {
        let (mode, single) = resolve_forwarder_selection(
            Failover,
            None,
            Some(Single),
            Some("9.9.9.9"),
            &upstreams(),
        )
        .unwrap();
        assert_eq!(mode, Single);
        assert_eq!(single.as_deref(), Some("9.9.9.9"));
    }

    #[test]
    fn selecting_an_unlisted_server_is_rejected() {
        let err = resolve_forwarder_selection(
            Failover,
            None,
            Some(Single),
            Some("4.4.4.4"),
            &upstreams(),
        )
        .unwrap_err();
        assert!(
            err.contains("4.4.4.4"),
            "message names the bad address: {err}"
        );
    }

    #[test]
    fn single_mode_without_any_address_is_rejected() {
        let err = resolve_forwarder_selection(Failover, None, Some(Single), None, &upstreams())
            .unwrap_err();
        assert!(err.contains("requires a single_upstream"));
    }

    #[test]
    fn removing_the_selected_server_is_rejected() {
        // Mode unchanged (stays Single via current), request only changes the
        // upstream list to one that no longer contains the selected address.
        let remaining = vec!["1.1.1.1".to_owned(), "9.9.9.9".to_owned()];
        let err = resolve_forwarder_selection(Single, Some("8.8.8.8"), None, None, &remaining)
            .unwrap_err();
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
}

#[cfg(test)]
mod forwarder_service_tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;
    use wardnet_common::api::UpstreamDnsRequest;
    use wardnet_common::auth::AuthContext;

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
}
