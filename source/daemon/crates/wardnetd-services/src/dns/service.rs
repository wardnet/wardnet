//! DNS server config + status + cache flush + query log + stats.
//!
//! After issue #221 (Stage 7), every filter-source concern (blocklists,
//! allowlist, custom rules, per-device settings) lives behind
//! [`crate::dns_filter::DnsFilterService`]. This service is left with the
//! DNS server lifecycle and observability.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{Duration, Utc};
use tokio::sync::broadcast;
use uuid::Uuid;
use wardnet_common::api::{
    DnsCacheFlushResponse, DnsConfigResponse, DnsSeriesBucket, DnsSeriesPoint, DnsStatsResponse,
    DnsStatsTotals, DnsStatusResponse, ListQueryLogParams, ListQueryLogResponse, QueryLogEvent,
    ToggleDnsRequest, TopClient, TopDomain, UpdateDnsConfigRequest,
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
    BucketSize, DeviceRepository, DnsRepository, QueryLogFilter, SystemConfigRepository,
};

pub const QUERY_LOG_MAX_LIMIT: u32 = 500;
pub const QUERY_LOG_DEFAULT_LIMIT: u32 = 50;
pub const DNS_STATS_DEFAULT_HOURS: u32 = 24;
pub const DNS_STATS_MAX_HOURS: u32 = 168;
pub const QUERY_LOG_RETENTION_MIN_DAYS: u32 = 1;
pub const QUERY_LOG_RETENTION_MAX_DAYS: u32 = 30;
const STATS_TOP_N: u32 = 10;

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
    async fn dns_stats(&self, hours: u32) -> Result<DnsStatsResponse, AppError>;
    fn subscribe_query_stream(&self) -> Result<broadcast::Receiver<QueryLogEvent>, AppError>;
    async fn flush_query_log(&self) -> Result<u64, AppError>;

    /// Internal: load the DNS server runtime config (called by the runner).
    async fn get_dns_config(&self) -> Result<DnsConfig, AppError>;
}

pub struct DnsServiceImpl {
    system_config: Arc<dyn SystemConfigRepository>,
    dns_repo: Arc<dyn DnsRepository>,
    device_repo: Arc<dyn DeviceRepository>,
    events: Arc<dyn EventPublisher>,
    log_sink: Option<Arc<DnsLogSink>>,
}

impl DnsServiceImpl {
    pub fn new(
        system_config: Arc<dyn SystemConfigRepository>,
        dns_repo: Arc<dyn DnsRepository>,
        device_repo: Arc<dyn DeviceRepository>,
        events: Arc<dyn EventPublisher>,
        log_sink: Option<Arc<DnsLogSink>>,
    ) -> Self {
        Self {
            system_config,
            dns_repo,
            device_repo,
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
            timestamp: Utc::now(),
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
                timestamp: Utc::now(),
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
            result: params.result,
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
                    result: parse_dns_query_result(&row.result),
                    upstream: row.upstream,
                    latency_ms: row.latency_ms,
                    device_id,
                })
            })
            .collect();

        Ok(ListQueryLogResponse { entries, total })
    }

    async fn dns_stats(&self, hours: u32) -> Result<DnsStatsResponse, AppError> {
        auth_context::require_admin()?;

        let hours = hours.clamp(1, DNS_STATS_MAX_HOURS);
        let since = Utc::now() - Duration::hours(i64::from(hours));

        let stats = self
            .dns_repo
            .query_stats(since)
            .await
            .map_err(AppError::Internal)?;

        let blocked_percent = if stats.total_queries == 0 {
            0.0
        } else {
            #[allow(clippy::cast_precision_loss)]
            let pct = (stats.blocked_queries as f64) / (stats.total_queries as f64) * 100.0;
            pct
        };

        let totals = DnsStatsTotals {
            total_queries: stats.total_queries,
            blocked_queries: stats.blocked_queries,
            blocked_percent,
            avg_latency_ms: stats.avg_latency_ms,
            unique_clients: stats.unique_clients,
            unique_domains: stats.unique_domains,
        };

        let top_domains = self
            .dns_repo
            .top_domains(since, STATS_TOP_N, false)
            .await
            .map_err(AppError::Internal)?
            .into_iter()
            .map(|r| TopDomain {
                domain: r.domain,
                count: r.count,
            })
            .collect();

        let top_blocked = self
            .dns_repo
            .top_domains(since, STATS_TOP_N, true)
            .await
            .map_err(AppError::Internal)?
            .into_iter()
            .map(|r| TopDomain {
                domain: r.domain,
                count: r.count,
            })
            .collect();

        let top_client_rows = self
            .dns_repo
            .top_clients(since, STATS_TOP_N)
            .await
            .map_err(AppError::Internal)?;

        let top_clients = self.enrich_top_clients(top_client_rows).await;

        let bucket = if hours <= 1 {
            BucketSize::Minute
        } else {
            BucketSize::Hour
        };
        let series_rows = self
            .dns_repo
            .series_buckets(since, bucket)
            .await
            .map_err(AppError::Internal)?;
        let series = series_rows
            .into_iter()
            .map(|r| DnsSeriesPoint {
                bucket: r.bucket,
                total: r.total,
                blocked: r.blocked,
            })
            .collect();
        let series_bucket = match bucket {
            BucketSize::Minute => DnsSeriesBucket::Minute,
            BucketSize::Hour => DnsSeriesBucket::Hour,
        };

        Ok(DnsStatsResponse {
            hours,
            totals,
            top_domains,
            top_blocked,
            top_clients,
            series_bucket,
            series,
        })
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

fn parse_iso_timestamp(s: &str) -> anyhow::Result<chrono::DateTime<Utc>> {
    use chrono::NaiveDateTime;
    let naive = NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%SZ")?;
    Ok(chrono::TimeZone::from_utc_datetime(&Utc, &naive))
}

fn parse_dns_query_result(s: &str) -> DnsQueryResult {
    match s {
        "forwarded" => DnsQueryResult::Forwarded,
        "cache_hit" | "cached" => DnsQueryResult::Cached,
        "blocked" => DnsQueryResult::Blocked,
        "blocked_skipped" => DnsQueryResult::BlockedSkipped,
        "rewritten" | "local" => DnsQueryResult::Local,
        "recursive" => DnsQueryResult::Recursive,
        _ => DnsQueryResult::Error,
    }
}

impl DnsServiceImpl {
    async fn enrich_top_clients(
        &self,
        rows: Vec<wardnetd_data::repository::TopClientRow>,
    ) -> Vec<TopClient> {
        type DeviceMeta = (Option<String>, Option<String>, Option<String>);
        let mut by_ip: HashMap<String, DeviceMeta> = HashMap::new();

        for row in &rows {
            if by_ip.contains_key(&row.client_ip) {
                continue;
            }
            match self.device_repo.find_by_ip(&row.client_ip).await {
                Ok(Some(device)) => {
                    let label = device.name.clone().or_else(|| device.hostname.clone());
                    by_ip.insert(
                        row.client_ip.clone(),
                        (Some(device.id.to_string()), label, Some(device.mac.clone())),
                    );
                }
                Ok(None) => {
                    by_ip.insert(row.client_ip.clone(), (None, None, None));
                }
                Err(e) => {
                    tracing::warn!(client_ip = %row.client_ip, error = %e,
                        "device lookup failed for top-client");
                    by_ip.insert(row.client_ip.clone(), (None, None, None));
                }
            }
        }

        rows.into_iter()
            .map(|row| {
                let (device_id, device_label, device_mac) =
                    by_ip.remove(&row.client_ip).unwrap_or((None, None, None));
                TopClient {
                    client_ip: row.client_ip,
                    count: row.count,
                    device_id,
                    device_label,
                    device_mac,
                }
            })
            .collect()
    }
}
