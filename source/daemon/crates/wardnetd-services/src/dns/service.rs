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
    DnsConfig, DnsQueryLogEntry, DnsQueryResult, DnsResolutionMode, UpstreamDns,
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

        let enabled = get("dns_enabled")
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

        Ok(DnsConfig {
            enabled,
            resolution_mode,
            upstream_servers,
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

#[async_trait]
impl DnsService for DnsServiceImpl {
    async fn get_config(&self) -> Result<DnsConfigResponse, AppError> {
        auth_context::require_admin()?;
        let config = self.load_config().await?;
        Ok(DnsConfigResponse { config })
    }

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

        if let Some(ref mode) = req.resolution_mode {
            self.system_config
                .set("dns_resolution_mode", mode)
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
                })
                .collect();
            let json = serde_json::to_string(&upstream)
                .map_err(|e| AppError::Internal(anyhow::anyhow!("serialize upstreams: {e}")))?;
            self.system_config
                .set("dns_upstream_servers", &json)
                .await
                .map_err(AppError::Internal)?;
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
            .set("dns_enabled", if req.enabled { "true" } else { "false" })
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
