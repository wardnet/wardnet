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
    DEFAULT_FORWARD_DEADLINE_MS, DEFAULT_UPSTREAM_TIMEOUT_MS, DnsConfig, DnsProtocol,
    DnsQueryLogEntry, DnsQueryResult, DnsResolutionMode, ForwarderSelectionMode, UpstreamDns,
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
/// Bounds for [`DnsConfig::upstream_timeout_ms`]. The floor keeps an admin
/// from setting a deadline no real upstream can meet (which would SERVFAIL
/// every query); the ceiling keeps one rung of the ladder from outlasting a
/// client stub's patience on its own.
pub const UPSTREAM_TIMEOUT_MIN_MS: u32 = 100;
pub const UPSTREAM_TIMEOUT_MAX_MS: u32 = 10_000;
/// Bounds for [`DnsConfig::forward_deadline_ms`]. The ceiling is deliberately
/// above a stub resolver's ~5s patience: an admin debugging a slow link may
/// want to see the answer arrive even though no client is still waiting for
/// it, and the query log records it either way.
pub const FORWARD_DEADLINE_MIN_MS: u32 = 200;
pub const FORWARD_DEADLINE_MAX_MS: u32 = 15_000;
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
        let upstream_timeout_ms = Self::parse_u32(
            get("dns_upstream_timeout_ms").await?,
            DEFAULT_UPSTREAM_TIMEOUT_MS,
        )?;
        let forward_deadline_ms = Self::parse_u32(
            get("dns_forward_deadline_ms").await?,
            DEFAULT_FORWARD_DEADLINE_MS,
        )?;
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
            upstream_timeout_ms,
            forward_deadline_ms,
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

/// Validate the forwarding timings an update results in.
///
/// Pure so the rules can be unit-tested without a repository. Beyond the
/// independent ranges, the pair has to be ordered: a per-upstream timeout
/// larger than the whole-query deadline means the first upstream can consume
/// the entire budget, so the ladder would never reach a second one and
/// failover would exist only on paper.
pub(crate) fn validate_forward_timings(
    upstream_timeout_ms: u32,
    forward_deadline_ms: u32,
) -> Result<(), String> {
    if !(UPSTREAM_TIMEOUT_MIN_MS..=UPSTREAM_TIMEOUT_MAX_MS).contains(&upstream_timeout_ms) {
        return Err(format!(
            "upstream_timeout_ms must be between {UPSTREAM_TIMEOUT_MIN_MS} and {UPSTREAM_TIMEOUT_MAX_MS}"
        ));
    }
    if !(FORWARD_DEADLINE_MIN_MS..=FORWARD_DEADLINE_MAX_MS).contains(&forward_deadline_ms) {
        return Err(format!(
            "forward_deadline_ms must be between {FORWARD_DEADLINE_MIN_MS} and {FORWARD_DEADLINE_MAX_MS}"
        ));
    }
    if upstream_timeout_ms > forward_deadline_ms {
        return Err(format!(
            "upstream_timeout_ms ({upstream_timeout_ms}) must not exceed forward_deadline_ms ({forward_deadline_ms})"
        ));
    }
    Ok(())
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
pub(crate) fn resolve_forwarder_selection(
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

        // Forwarding timings. Both fields are independently optional, so the
        // pair has to be validated as it will end up — a request that raises
        // only the per-upstream timeout must still be checked against the
        // persisted whole-query deadline.
        if req.upstream_timeout_ms.is_some() || req.forward_deadline_ms.is_some() {
            let current = self.load_config().await?;
            validate_forward_timings(
                req.upstream_timeout_ms
                    .unwrap_or(current.upstream_timeout_ms),
                req.forward_deadline_ms
                    .unwrap_or(current.forward_deadline_ms),
            )
            .map_err(AppError::BadRequest)?;
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
        if let Some(v) = req.upstream_timeout_ms {
            self.system_config
                .set("dns_upstream_timeout_ms", &v.to_string())
                .await
                .map_err(AppError::Internal)?;
        }
        if let Some(v) = req.forward_deadline_ms {
            self.system_config
                .set("dns_forward_deadline_ms", &v.to_string())
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
        auth_context::require_admin()?;
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

        // Over-fetch a single row past the page to learn whether another page
        // exists. This runs after the clamp above, so the cap still governs
        // what is returned. `has_more` is taken from the raw row count rather
        // than from `entries`, because the mapping below drops rows whose
        // timestamp will not parse.
        let mut rows = self
            .dns_repo
            .query_log_paginated(limit + 1, offset, &filter)
            .await
            .map_err(AppError::Internal)?;
        let has_more = rows.len() > limit as usize;
        rows.truncate(limit as usize);

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

        Ok(ListQueryLogResponse { entries, has_more })
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
