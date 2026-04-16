use std::str::FromStr;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use uuid::Uuid;
use wardnet_types::api::{
    CreateAllowlistRequest, CreateAllowlistResponse, CreateBlocklistRequest,
    CreateBlocklistResponse, CreateFilterRuleRequest, CreateFilterRuleResponse,
    DeleteAllowlistResponse, DeleteBlocklistResponse, DeleteFilterRuleResponse,
    DnsCacheFlushResponse, DnsConfigResponse, DnsStatusResponse, ListAllowlistResponse,
    ListBlocklistsResponse, ListFilterRulesResponse, ToggleDnsRequest, UpdateBlocklistNowResponse,
    UpdateBlocklistRequest, UpdateBlocklistResponse, UpdateDnsConfigRequest,
    UpdateFilterRuleRequest, UpdateFilterRuleResponse,
};
use wardnet_types::dns::{DnsConfig, DnsResolutionMode, UpstreamDns};
use wardnet_types::event::WardnetEvent;

use crate::auth_context;
use crate::dns::filter::FilterInputs;
use crate::error::AppError;
use crate::event::EventPublisher;
use crate::repository::{
    AllowlistRow, BlocklistRow, BlocklistUpdate, CustomRuleRow, CustomRuleUpdate, DnsRepository,
    SystemConfigRepository,
};

/// DNS server configuration, status management, and ad-blocking CRUD.
#[async_trait]
pub trait DnsService: Send + Sync {
    /// Get the current DNS configuration.
    async fn get_config(&self) -> Result<DnsConfigResponse, AppError>;

    /// Update DNS configuration fields (partial update).
    async fn update_config(
        &self,
        req: UpdateDnsConfigRequest,
    ) -> Result<DnsConfigResponse, AppError>;

    /// Enable or disable the DNS server.
    async fn toggle(&self, req: ToggleDnsRequest) -> Result<DnsConfigResponse, AppError>;

    /// Get DNS server runtime status.
    async fn status(&self) -> Result<DnsStatusResponse, AppError>;

    /// Flush the DNS cache.
    async fn flush_cache(&self) -> Result<DnsCacheFlushResponse, AppError>;

    // ── Blocklists ──────────────────────────────────────────────────────

    /// List all blocklists.
    async fn list_blocklists(&self) -> Result<ListBlocklistsResponse, AppError>;

    /// Create a new blocklist.
    async fn create_blocklist(
        &self,
        req: CreateBlocklistRequest,
    ) -> Result<CreateBlocklistResponse, AppError>;

    /// Update an existing blocklist (partial).
    async fn update_blocklist(
        &self,
        id: Uuid,
        req: UpdateBlocklistRequest,
    ) -> Result<UpdateBlocklistResponse, AppError>;

    /// Delete a blocklist.
    async fn delete_blocklist(&self, id: Uuid) -> Result<DeleteBlocklistResponse, AppError>;

    /// Trigger a blocklist refresh (placeholder).
    async fn update_blocklist_now(&self, id: Uuid) -> Result<UpdateBlocklistNowResponse, AppError>;

    // ── Allowlist ───────────────────────────────────────────────────────

    /// List all allowlist entries.
    async fn list_allowlist(&self) -> Result<ListAllowlistResponse, AppError>;

    /// Create a new allowlist entry.
    async fn create_allowlist_entry(
        &self,
        req: CreateAllowlistRequest,
    ) -> Result<CreateAllowlistResponse, AppError>;

    /// Delete an allowlist entry.
    async fn delete_allowlist_entry(&self, id: Uuid) -> Result<DeleteAllowlistResponse, AppError>;

    // ── Custom filter rules ─────────────────────────────────────────────

    /// List all custom filter rules.
    async fn list_filter_rules(&self) -> Result<ListFilterRulesResponse, AppError>;

    /// Create a new custom filter rule.
    async fn create_filter_rule(
        &self,
        req: CreateFilterRuleRequest,
    ) -> Result<CreateFilterRuleResponse, AppError>;

    /// Update an existing custom filter rule (partial).
    async fn update_filter_rule(
        &self,
        id: Uuid,
        req: UpdateFilterRuleRequest,
    ) -> Result<UpdateFilterRuleResponse, AppError>;

    /// Delete a custom filter rule.
    async fn delete_filter_rule(&self, id: Uuid) -> Result<DeleteFilterRuleResponse, AppError>;

    // ── Internal ────────────────────────────────────────────────────────

    /// Load filter inputs for building the `DnsFilter` engine.
    async fn load_filter_inputs(&self) -> Result<FilterInputs, AppError>;

    // ── Runtime methods (called by DNS server, not HTTP handlers) ──

    /// Load the full DNS config for the server runtime.
    async fn get_dns_config(&self) -> Result<DnsConfig, AppError>;
}

/// Default implementation of [`DnsService`].
pub struct DnsServiceImpl {
    system_config: Arc<dyn SystemConfigRepository>,
    dns_repo: Arc<dyn DnsRepository>,
    events: Arc<dyn EventPublisher>,
}

impl DnsServiceImpl {
    pub fn new(
        system_config: Arc<dyn SystemConfigRepository>,
        dns_repo: Arc<dyn DnsRepository>,
        events: Arc<dyn EventPublisher>,
    ) -> Self {
        Self {
            system_config,
            dns_repo,
            events,
        }
    }

    /// Load DNS configuration from `system_config` KV store.
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
        let ad_blocking_enabled = get("dns_ad_blocking_enabled")
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
            ad_blocking_enabled,
            query_log_enabled,
            query_log_retention_days,
        })
    }

    fn parse_u32(val: Option<String>, default: u32) -> Result<u32, AppError> {
        val.unwrap_or_else(|| default.to_string())
            .parse()
            .map_err(|e| AppError::Internal(anyhow::anyhow!("invalid u32 config value: {e}")))
    }

    /// Validate a domain name for allowlist entries.
    fn validate_domain(domain: &str) -> Result<(), AppError> {
        if domain.is_empty() {
            return Err(AppError::BadRequest("domain must not be empty".to_owned()));
        }
        if domain.len() > 253 {
            return Err(AppError::BadRequest(
                "domain must be <= 253 characters".to_owned(),
            ));
        }
        if !domain.contains('.') {
            return Err(AppError::BadRequest(
                "domain must contain at least one '.'".to_owned(),
            ));
        }
        if !domain
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_')
        {
            return Err(AppError::BadRequest(
                "domain contains invalid characters (allowed: alphanumeric, '.', '-', '_')"
                    .to_owned(),
            ));
        }
        Ok(())
    }

    /// Validate a URL starts with http:// or https://.
    fn validate_url(url: &str) -> Result<(), AppError> {
        if !url.starts_with("http://") && !url.starts_with("https://") {
            return Err(AppError::BadRequest(
                "URL must start with http:// or https://".to_owned(),
            ));
        }
        Ok(())
    }

    /// Validate a cron expression.
    fn validate_cron(schedule: &str) -> Result<(), AppError> {
        cron::Schedule::from_str(schedule)
            .map_err(|e| AppError::BadRequest(format!("Invalid cron expression: {e}")))?;
        Ok(())
    }

    /// Validate rule text parses as a valid filter rule (not a comment/blank).
    fn validate_rule_text(rule_text: &str) -> Result<(), AppError> {
        match crate::dns::filter_parser::parse_line(rule_text) {
            Err(e) => Err(AppError::BadRequest(format!("Invalid filter rule: {e}"))),
            Ok(None) => Err(AppError::BadRequest(
                "Rule parses as a comment or blank line".to_owned(),
            )),
            Ok(Some(_)) => Ok(()),
        }
    }

    /// Publish a `DnsFiltersChanged` event.
    fn publish_filters_changed(&self) {
        self.events.publish(WardnetEvent::DnsFiltersChanged {
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
        if let Some(v) = req.ad_blocking_enabled {
            self.system_config
                .set("dns_ad_blocking_enabled", if v { "true" } else { "false" })
                .await
                .map_err(AppError::Internal)?;
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

        let config = self.load_config().await?;
        Ok(DnsConfigResponse { config })
    }

    async fn toggle(&self, req: ToggleDnsRequest) -> Result<DnsConfigResponse, AppError> {
        auth_context::require_admin()?;
        self.system_config
            .set("dns_enabled", if req.enabled { "true" } else { "false" })
            .await
            .map_err(AppError::Internal)?;
        let config = self.load_config().await?;
        Ok(DnsConfigResponse { config })
    }

    async fn status(&self) -> Result<DnsStatusResponse, AppError> {
        auth_context::require_admin()?;
        let config = self.load_config().await?;
        // TODO(stage-1): wire actual runtime stats from DnsServer once available.
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
        // TODO(stage-1): wire cache flush via DnsServer handle.
        Ok(DnsCacheFlushResponse {
            message: "Cache flushed".to_owned(),
            entries_cleared: 0,
        })
    }

    // ── Blocklists ──────────────────────────────────────────────────────

    async fn list_blocklists(&self) -> Result<ListBlocklistsResponse, AppError> {
        auth_context::require_admin()?;
        let blocklists = self
            .dns_repo
            .list_blocklists()
            .await
            .map_err(AppError::Internal)?;
        Ok(ListBlocklistsResponse { blocklists })
    }

    async fn create_blocklist(
        &self,
        req: CreateBlocklistRequest,
    ) -> Result<CreateBlocklistResponse, AppError> {
        auth_context::require_admin()?;

        Self::validate_url(&req.url)?;
        Self::validate_cron(&req.cron_schedule)?;

        let id = Uuid::new_v4();
        let row = BlocklistRow {
            id: id.to_string(),
            name: req.name,
            url: req.url,
            enabled: req.enabled,
            cron_schedule: req.cron_schedule,
        };

        self.dns_repo
            .create_blocklist(&row)
            .await
            .map_err(AppError::Internal)?;

        let blocklist = self
            .dns_repo
            .get_blocklist(id)
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| {
                AppError::Internal(anyhow::anyhow!("blocklist not found after insert"))
            })?;

        self.publish_filters_changed();

        Ok(CreateBlocklistResponse {
            blocklist,
            message: "blocklist created".to_owned(),
        })
    }

    async fn update_blocklist(
        &self,
        id: Uuid,
        req: UpdateBlocklistRequest,
    ) -> Result<UpdateBlocklistResponse, AppError> {
        auth_context::require_admin()?;

        // Ensure blocklist exists.
        self.dns_repo
            .get_blocklist(id)
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::NotFound(format!("blocklist {id} not found")))?;

        // Validate optional fields.
        if let Some(ref url) = req.url {
            Self::validate_url(url)?;
        }
        if let Some(ref cron) = req.cron_schedule {
            Self::validate_cron(cron)?;
        }

        let update = BlocklistUpdate {
            name: req.name,
            url: req.url,
            enabled: req.enabled,
            cron_schedule: req.cron_schedule,
        };

        self.dns_repo
            .update_blocklist(id, &update)
            .await
            .map_err(AppError::Internal)?;

        let blocklist = self
            .dns_repo
            .get_blocklist(id)
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| {
                AppError::Internal(anyhow::anyhow!("blocklist not found after update"))
            })?;

        self.publish_filters_changed();

        Ok(UpdateBlocklistResponse {
            blocklist,
            message: "blocklist updated".to_owned(),
        })
    }

    async fn delete_blocklist(&self, id: Uuid) -> Result<DeleteBlocklistResponse, AppError> {
        auth_context::require_admin()?;

        let deleted = self
            .dns_repo
            .delete_blocklist(id)
            .await
            .map_err(AppError::Internal)?;

        if !deleted {
            return Err(AppError::NotFound(format!("blocklist {id} not found")));
        }

        self.publish_filters_changed();

        Ok(DeleteBlocklistResponse {
            message: format!("blocklist {id} deleted"),
        })
    }

    async fn update_blocklist_now(&self, id: Uuid) -> Result<UpdateBlocklistNowResponse, AppError> {
        auth_context::require_admin()?;

        let blocklist = self
            .dns_repo
            .get_blocklist(id)
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::NotFound(format!("blocklist {id} not found")))?;

        self.publish_filters_changed();

        Ok(UpdateBlocklistNowResponse {
            entry_count: blocklist.entry_count,
            blocklist,
            message: "blocklist refresh triggered".to_owned(),
        })
    }

    // ── Allowlist ───────────────────────────────────────────────────────

    async fn list_allowlist(&self) -> Result<ListAllowlistResponse, AppError> {
        auth_context::require_admin()?;
        let entries = self
            .dns_repo
            .list_allowlist()
            .await
            .map_err(AppError::Internal)?;
        Ok(ListAllowlistResponse { entries })
    }

    async fn create_allowlist_entry(
        &self,
        req: CreateAllowlistRequest,
    ) -> Result<CreateAllowlistResponse, AppError> {
        auth_context::require_admin()?;

        Self::validate_domain(&req.domain)?;

        let id = Uuid::new_v4();
        let row = AllowlistRow {
            id: id.to_string(),
            domain: req.domain,
            reason: req.reason,
        };

        self.dns_repo
            .create_allowlist_entry(&row)
            .await
            .map_err(AppError::Internal)?;

        // Re-fetch to get the created_at timestamp from the DB.
        let entries = self
            .dns_repo
            .list_allowlist()
            .await
            .map_err(AppError::Internal)?;
        let entry = entries.into_iter().find(|e| e.id == id).ok_or_else(|| {
            AppError::Internal(anyhow::anyhow!("allowlist entry not found after insert"))
        })?;

        self.publish_filters_changed();

        Ok(CreateAllowlistResponse {
            entry,
            message: "allowlist entry created".to_owned(),
        })
    }

    async fn delete_allowlist_entry(&self, id: Uuid) -> Result<DeleteAllowlistResponse, AppError> {
        auth_context::require_admin()?;

        let deleted = self
            .dns_repo
            .delete_allowlist_entry(id)
            .await
            .map_err(AppError::Internal)?;

        if !deleted {
            return Err(AppError::NotFound(format!(
                "allowlist entry {id} not found"
            )));
        }

        self.publish_filters_changed();

        Ok(DeleteAllowlistResponse {
            message: format!("allowlist entry {id} deleted"),
        })
    }

    // ── Custom filter rules ─────────────────────────────────────────────

    async fn list_filter_rules(&self) -> Result<ListFilterRulesResponse, AppError> {
        auth_context::require_admin()?;
        let rules = self
            .dns_repo
            .list_custom_rules()
            .await
            .map_err(AppError::Internal)?;
        Ok(ListFilterRulesResponse { rules })
    }

    async fn create_filter_rule(
        &self,
        req: CreateFilterRuleRequest,
    ) -> Result<CreateFilterRuleResponse, AppError> {
        auth_context::require_admin()?;

        Self::validate_rule_text(&req.rule_text)?;

        let id = Uuid::new_v4();
        let row = CustomRuleRow {
            id: id.to_string(),
            rule_text: req.rule_text,
            enabled: req.enabled,
            comment: req.comment,
        };

        self.dns_repo
            .create_custom_rule(&row)
            .await
            .map_err(AppError::Internal)?;

        let rule = self
            .dns_repo
            .get_custom_rule(id)
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| {
                AppError::Internal(anyhow::anyhow!("custom rule not found after insert"))
            })?;

        self.publish_filters_changed();

        Ok(CreateFilterRuleResponse {
            rule,
            message: "filter rule created".to_owned(),
        })
    }

    async fn update_filter_rule(
        &self,
        id: Uuid,
        req: UpdateFilterRuleRequest,
    ) -> Result<UpdateFilterRuleResponse, AppError> {
        auth_context::require_admin()?;

        self.dns_repo
            .get_custom_rule(id)
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::NotFound(format!("filter rule {id} not found")))?;

        if let Some(ref rule_text) = req.rule_text {
            Self::validate_rule_text(rule_text)?;
        }

        let update = CustomRuleUpdate {
            rule_text: req.rule_text,
            enabled: req.enabled,
            comment: req.comment,
        };

        self.dns_repo
            .update_custom_rule(id, &update)
            .await
            .map_err(AppError::Internal)?;

        let rule = self
            .dns_repo
            .get_custom_rule(id)
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| {
                AppError::Internal(anyhow::anyhow!("custom rule not found after update"))
            })?;

        self.publish_filters_changed();

        Ok(UpdateFilterRuleResponse {
            rule,
            message: "filter rule updated".to_owned(),
        })
    }

    async fn delete_filter_rule(&self, id: Uuid) -> Result<DeleteFilterRuleResponse, AppError> {
        auth_context::require_admin()?;

        let deleted = self
            .dns_repo
            .delete_custom_rule(id)
            .await
            .map_err(AppError::Internal)?;

        if !deleted {
            return Err(AppError::NotFound(format!("filter rule {id} not found")));
        }

        self.publish_filters_changed();

        Ok(DeleteFilterRuleResponse {
            message: format!("filter rule {id} deleted"),
        })
    }

    // ── Internal ────────────────────────────────────────────────────────

    async fn load_filter_inputs(&self) -> Result<FilterInputs, AppError> {
        let blocked_domains = self
            .dns_repo
            .list_all_blocked_domains_for_enabled()
            .await
            .map_err(AppError::Internal)?;

        let allowlist_entries = self
            .dns_repo
            .list_allowlist()
            .await
            .map_err(AppError::Internal)?;
        let allowlist = allowlist_entries.into_iter().map(|e| e.domain).collect();

        let custom_rules_entries = self
            .dns_repo
            .list_custom_rules()
            .await
            .map_err(AppError::Internal)?;
        let custom_rules = custom_rules_entries
            .into_iter()
            .filter(|r| r.enabled)
            .map(|r| r.rule_text)
            .collect();

        Ok(FilterInputs {
            blocked_domains,
            allowlist,
            custom_rules,
        })
    }

    async fn get_dns_config(&self) -> Result<DnsConfig, AppError> {
        self.load_config().await
    }
}
