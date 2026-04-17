use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use tokio::sync::broadcast;
use uuid::Uuid;
use wardnet_common::api::{
    CreateAllowlistRequest, CreateBlocklistRequest, CreateFilterRuleRequest, ToggleDnsRequest,
    UpdateDnsConfigRequest, UpstreamDnsRequest,
};
use wardnet_common::auth::AuthContext;
use wardnet_common::dns::{
    AllowlistEntry, Blocklist, CustomFilterRule, DnsProtocol, DnsResolutionMode,
};
use wardnet_common::event::WardnetEvent;

use crate::auth_context;
use crate::error::AppError;
use crate::event::EventPublisher;
use crate::{DnsService, DnsServiceImpl};
use wardnetd_data::repository::{
    AllowlistRow, BlocklistRow, BlocklistUpdate, CustomRuleRow, CustomRuleUpdate, DnsRepository,
    QueryLogFilter, QueryLogRow, SystemConfigRepository,
};

// -- Mock SystemConfigRepository ----------------------------------------------

struct MockSystemConfigRepository {
    data: Mutex<HashMap<String, String>>,
}

impl MockSystemConfigRepository {
    fn new() -> Self {
        Self {
            data: Mutex::new(HashMap::new()),
        }
    }

    fn with_data(data: HashMap<String, String>) -> Self {
        Self {
            data: Mutex::new(data),
        }
    }
}

#[async_trait]
impl SystemConfigRepository for MockSystemConfigRepository {
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

// -- Mock DnsRepository -------------------------------------------------------

struct MockDnsRepository {
    blocklists: Mutex<Vec<Blocklist>>,
    allowlist: Mutex<Vec<AllowlistEntry>>,
    custom_rules: Mutex<Vec<CustomFilterRule>>,
    blocked_domains: Mutex<Vec<String>>,
}

impl MockDnsRepository {
    fn new() -> Self {
        Self {
            blocklists: Mutex::new(Vec::new()),
            allowlist: Mutex::new(Vec::new()),
            custom_rules: Mutex::new(Vec::new()),
            blocked_domains: Mutex::new(Vec::new()),
        }
    }
}

#[async_trait]
impl DnsRepository for MockDnsRepository {
    // Query log stubs
    async fn insert_query_log_batch(&self, _entries: &[QueryLogRow]) -> anyhow::Result<()> {
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

    async fn cleanup_query_log(&self, _retention_days: u32) -> anyhow::Result<u64> {
        Ok(0)
    }

    // Blocklists
    async fn list_blocklists(&self) -> anyhow::Result<Vec<Blocklist>> {
        Ok(self.blocklists.lock().unwrap().clone())
    }

    async fn get_blocklist(&self, id: Uuid) -> anyhow::Result<Option<Blocklist>> {
        Ok(self
            .blocklists
            .lock()
            .unwrap()
            .iter()
            .find(|b| b.id == id)
            .cloned())
    }

    async fn create_blocklist(&self, row: &BlocklistRow) -> anyhow::Result<()> {
        let now = chrono::Utc::now();
        self.blocklists.lock().unwrap().push(Blocklist {
            id: row.id.parse().unwrap(),
            name: row.name.clone(),
            url: row.url.clone(),
            enabled: row.enabled,
            entry_count: 0,
            last_updated: None,
            cron_schedule: row.cron_schedule.clone(),
            last_error: None,
            last_error_at: None,
            created_at: now,
            updated_at: now,
        });
        Ok(())
    }

    async fn update_blocklist(&self, id: Uuid, row: &BlocklistUpdate) -> anyhow::Result<()> {
        let mut lists = self.blocklists.lock().unwrap();
        if let Some(bl) = lists.iter_mut().find(|b| b.id == id) {
            if let Some(ref name) = row.name {
                bl.name.clone_from(name);
            }
            if let Some(ref url) = row.url {
                bl.url.clone_from(url);
            }
            if let Some(enabled) = row.enabled {
                bl.enabled = enabled;
            }
            if let Some(ref cron) = row.cron_schedule {
                bl.cron_schedule.clone_from(cron);
            }
        }
        Ok(())
    }

    async fn delete_blocklist(&self, id: Uuid) -> anyhow::Result<bool> {
        let mut lists = self.blocklists.lock().unwrap();
        let len_before = lists.len();
        lists.retain(|b| b.id != id);
        Ok(lists.len() < len_before)
    }

    async fn replace_blocklist_domains(
        &self,
        _id: Uuid,
        _domains: &[String],
    ) -> anyhow::Result<u64> {
        Ok(0)
    }

    async fn list_all_blocked_domains_for_enabled(&self) -> anyhow::Result<Vec<String>> {
        Ok(self.blocked_domains.lock().unwrap().clone())
    }

    async fn set_blocklist_error(&self, _id: Uuid, _error: Option<&str>) -> anyhow::Result<()> {
        Ok(())
    }

    // Allowlist
    async fn list_allowlist(&self) -> anyhow::Result<Vec<AllowlistEntry>> {
        Ok(self.allowlist.lock().unwrap().clone())
    }

    async fn create_allowlist_entry(&self, row: &AllowlistRow) -> anyhow::Result<()> {
        self.allowlist.lock().unwrap().push(AllowlistEntry {
            id: row.id.parse().unwrap(),
            domain: row.domain.clone(),
            reason: row.reason.clone(),
            created_at: chrono::Utc::now(),
        });
        Ok(())
    }

    async fn delete_allowlist_entry(&self, id: Uuid) -> anyhow::Result<bool> {
        let mut entries = self.allowlist.lock().unwrap();
        let len_before = entries.len();
        entries.retain(|e| e.id != id);
        Ok(entries.len() < len_before)
    }

    // Custom rules
    async fn list_custom_rules(&self) -> anyhow::Result<Vec<CustomFilterRule>> {
        Ok(self.custom_rules.lock().unwrap().clone())
    }

    async fn get_custom_rule(&self, id: Uuid) -> anyhow::Result<Option<CustomFilterRule>> {
        Ok(self
            .custom_rules
            .lock()
            .unwrap()
            .iter()
            .find(|r| r.id == id)
            .cloned())
    }

    async fn create_custom_rule(&self, row: &CustomRuleRow) -> anyhow::Result<()> {
        let now = chrono::Utc::now();
        self.custom_rules.lock().unwrap().push(CustomFilterRule {
            id: row.id.parse().unwrap(),
            rule_text: row.rule_text.clone(),
            enabled: row.enabled,
            comment: row.comment.clone(),
            created_at: now,
            updated_at: now,
        });
        Ok(())
    }

    async fn update_custom_rule(&self, id: Uuid, row: &CustomRuleUpdate) -> anyhow::Result<()> {
        let mut rules = self.custom_rules.lock().unwrap();
        if let Some(rule) = rules.iter_mut().find(|r| r.id == id) {
            if let Some(ref text) = row.rule_text {
                rule.rule_text.clone_from(text);
            }
            if let Some(enabled) = row.enabled {
                rule.enabled = enabled;
            }
            if let Some(ref comment) = row.comment {
                rule.comment = Some(comment.clone());
            }
            rule.updated_at = chrono::Utc::now();
        }
        Ok(())
    }

    async fn delete_custom_rule(&self, id: Uuid) -> anyhow::Result<bool> {
        let mut rules = self.custom_rules.lock().unwrap();
        let len_before = rules.len();
        rules.retain(|r| r.id != id);
        Ok(rules.len() < len_before)
    }
}

// -- Mock EventPublisher ------------------------------------------------------

struct MockEventPublisher {
    events: Mutex<Vec<WardnetEvent>>,
    tx: broadcast::Sender<WardnetEvent>,
}

impl MockEventPublisher {
    fn new() -> Self {
        let (tx, _) = broadcast::channel(16);
        Self {
            events: Mutex::new(Vec::new()),
            tx,
        }
    }

    fn published_events(&self) -> Vec<WardnetEvent> {
        self.events.lock().unwrap().clone()
    }
}

impl EventPublisher for MockEventPublisher {
    fn publish(&self, event: WardnetEvent) {
        self.events.lock().unwrap().push(event.clone());
        let _ = self.tx.send(event);
    }

    fn subscribe(&self) -> broadcast::Receiver<WardnetEvent> {
        self.tx.subscribe()
    }
}

// -- Helpers ------------------------------------------------------------------

fn admin_ctx() -> AuthContext {
    AuthContext::Admin {
        admin_id: Uuid::new_v4(),
    }
}

fn build_service() -> DnsServiceImpl {
    let system_config = Arc::new(MockSystemConfigRepository::new());
    let dns_repo = Arc::new(MockDnsRepository::new());
    let events = Arc::new(MockEventPublisher::new());
    DnsServiceImpl::new(system_config, dns_repo, events)
}

fn build_service_with_config(data: HashMap<String, String>) -> DnsServiceImpl {
    let system_config = Arc::new(MockSystemConfigRepository::with_data(data));
    let dns_repo = Arc::new(MockDnsRepository::new());
    let events = Arc::new(MockEventPublisher::new());
    DnsServiceImpl::new(system_config, dns_repo, events)
}

fn build_service_with_repo() -> (DnsServiceImpl, Arc<MockSystemConfigRepository>) {
    let repo = Arc::new(MockSystemConfigRepository::new());
    let dns_repo = Arc::new(MockDnsRepository::new());
    let events = Arc::new(MockEventPublisher::new());
    let svc = DnsServiceImpl::new(repo.clone(), dns_repo, events);
    (svc, repo)
}

struct FullService {
    svc: DnsServiceImpl,
    dns_repo: Arc<MockDnsRepository>,
    events: Arc<MockEventPublisher>,
}

fn build_full_service() -> FullService {
    let system_config = Arc::new(MockSystemConfigRepository::new());
    let dns_repo = Arc::new(MockDnsRepository::new());
    let events = Arc::new(MockEventPublisher::new());
    let svc = DnsServiceImpl::new(system_config, dns_repo.clone(), events.clone());
    FullService {
        svc,
        dns_repo,
        events,
    }
}

// -- Existing tests -----------------------------------------------------------

#[tokio::test]
async fn get_config_returns_defaults() {
    let svc = build_service();
    let resp = auth_context::with_context(admin_ctx(), svc.get_config())
        .await
        .unwrap();

    let c = &resp.config;
    assert!(!c.enabled);
    assert_eq!(c.resolution_mode, DnsResolutionMode::Forwarding);
    assert!(c.upstream_servers.is_empty());
    assert_eq!(c.cache_size, 10_000);
    assert_eq!(c.cache_ttl_min_secs, 0);
    assert_eq!(c.cache_ttl_max_secs, 86_400);
    assert!(!c.dnssec_enabled);
    assert!(c.rebinding_protection);
    assert_eq!(c.rate_limit_per_second, 0);
    assert!(c.ad_blocking_enabled);
    assert!(c.query_log_enabled);
    assert_eq!(c.query_log_retention_days, 7);
}

#[tokio::test]
async fn get_config_returns_stored_values() {
    let mut data = HashMap::new();
    data.insert("dns_enabled".to_owned(), "true".to_owned());
    data.insert("dns_resolution_mode".to_owned(), "recursive".to_owned());
    data.insert("dns_cache_size".to_owned(), "5000".to_owned());
    data.insert("dns_dnssec_enabled".to_owned(), "true".to_owned());
    data.insert("dns_rebinding_protection".to_owned(), "false".to_owned());
    data.insert("dns_rate_limit_per_second".to_owned(), "100".to_owned());
    data.insert("dns_ad_blocking_enabled".to_owned(), "false".to_owned());
    data.insert("dns_query_log_enabled".to_owned(), "false".to_owned());
    data.insert("dns_query_log_retention_days".to_owned(), "30".to_owned());

    let svc = build_service_with_config(data);
    let resp = auth_context::with_context(admin_ctx(), svc.get_config())
        .await
        .unwrap();

    let c = &resp.config;
    assert!(c.enabled);
    assert_eq!(c.resolution_mode, DnsResolutionMode::Recursive);
    assert_eq!(c.cache_size, 5000);
    assert!(c.dnssec_enabled);
    assert!(!c.rebinding_protection);
    assert_eq!(c.rate_limit_per_second, 100);
    assert!(!c.ad_blocking_enabled);
    assert!(!c.query_log_enabled);
    assert_eq!(c.query_log_retention_days, 30);
}

#[tokio::test]
async fn update_config_persists_upstream_servers() {
    let svc = build_service();
    let req = UpdateDnsConfigRequest {
        resolution_mode: None,
        upstream_servers: Some(vec![
            UpstreamDnsRequest {
                address: "1.1.1.1".to_owned(),
                name: "Cloudflare".to_owned(),
                protocol: DnsProtocol::Udp,
                port: None,
            },
            UpstreamDnsRequest {
                address: "8.8.8.8".to_owned(),
                name: "Google".to_owned(),
                protocol: DnsProtocol::Udp,
                port: Some(53),
            },
        ]),
        cache_size: None,
        cache_ttl_min_secs: None,
        cache_ttl_max_secs: None,
        dnssec_enabled: None,
        rebinding_protection: None,
        rate_limit_per_second: None,
        ad_blocking_enabled: None,
        query_log_enabled: None,
        query_log_retention_days: None,
    };

    let resp = auth_context::with_context(admin_ctx(), svc.update_config(req))
        .await
        .unwrap();

    assert_eq!(resp.config.upstream_servers.len(), 2);
    assert_eq!(resp.config.upstream_servers[0].address, "1.1.1.1");
    assert_eq!(resp.config.upstream_servers[0].name, "Cloudflare");
    assert_eq!(resp.config.upstream_servers[0].protocol, DnsProtocol::Udp);
    assert_eq!(resp.config.upstream_servers[0].port, None);
    assert_eq!(resp.config.upstream_servers[1].address, "8.8.8.8");
    assert_eq!(resp.config.upstream_servers[1].port, Some(53));
}

#[tokio::test]
async fn update_config_partial_update() {
    let svc = build_service();

    // Only update cache_size, everything else stays at defaults.
    let req = UpdateDnsConfigRequest {
        resolution_mode: None,
        upstream_servers: None,
        cache_size: Some(20_000),
        cache_ttl_min_secs: None,
        cache_ttl_max_secs: None,
        dnssec_enabled: None,
        rebinding_protection: None,
        rate_limit_per_second: None,
        ad_blocking_enabled: None,
        query_log_enabled: None,
        query_log_retention_days: None,
    };

    let resp = auth_context::with_context(admin_ctx(), svc.update_config(req))
        .await
        .unwrap();

    let c = &resp.config;
    assert_eq!(c.cache_size, 20_000);
    // Defaults should be preserved for untouched fields.
    assert!(!c.enabled);
    assert_eq!(c.resolution_mode, DnsResolutionMode::Forwarding);
    assert!(c.upstream_servers.is_empty());
    assert_eq!(c.cache_ttl_min_secs, 0);
    assert_eq!(c.cache_ttl_max_secs, 86_400);
    assert!(!c.dnssec_enabled);
    assert!(c.rebinding_protection);
    assert_eq!(c.rate_limit_per_second, 0);
}

#[tokio::test]
async fn toggle_enables_dns() {
    let svc = build_service();

    let resp =
        auth_context::with_context(admin_ctx(), svc.toggle(ToggleDnsRequest { enabled: true }))
            .await
            .unwrap();

    assert!(resp.config.enabled);
}

#[tokio::test]
async fn toggle_disables_dns() {
    let (svc, repo) = build_service_with_repo();
    // Pre-enable DNS so we can disable it.
    repo.set("dns_enabled", "true").await.unwrap();

    let resp =
        auth_context::with_context(admin_ctx(), svc.toggle(ToggleDnsRequest { enabled: false }))
            .await
            .unwrap();

    assert!(!resp.config.enabled);
}

#[tokio::test]
async fn status_returns_defaults() {
    let svc = build_service();

    let resp = auth_context::with_context(admin_ctx(), svc.status())
        .await
        .unwrap();

    assert!(!resp.enabled);
    assert!(!resp.running);
    assert_eq!(resp.cache_size, 0);
    assert_eq!(resp.cache_capacity, 10_000);
    assert!(resp.cache_hit_rate.abs() < f64::EPSILON);
}

#[tokio::test]
async fn flush_cache_returns_response() {
    let svc = build_service();

    let resp = auth_context::with_context(admin_ctx(), svc.flush_cache())
        .await
        .unwrap();

    assert_eq!(resp.message, "Cache flushed");
    assert_eq!(resp.entries_cleared, 0);
}

#[tokio::test]
async fn get_dns_config_loads_all_fields() {
    let mut data = HashMap::new();
    data.insert("dns_enabled".to_owned(), "true".to_owned());
    data.insert("dns_resolution_mode".to_owned(), "recursive".to_owned());
    data.insert(
        "dns_upstream_servers".to_owned(),
        r#"[{"address":"9.9.9.9","name":"Quad9","protocol":"tls","port":853}]"#.to_owned(),
    );
    data.insert("dns_cache_size".to_owned(), "50000".to_owned());
    data.insert("dns_cache_ttl_min_secs".to_owned(), "60".to_owned());
    data.insert("dns_cache_ttl_max_secs".to_owned(), "3600".to_owned());
    data.insert("dns_dnssec_enabled".to_owned(), "true".to_owned());
    data.insert("dns_rebinding_protection".to_owned(), "false".to_owned());
    data.insert("dns_rate_limit_per_second".to_owned(), "500".to_owned());
    data.insert("dns_ad_blocking_enabled".to_owned(), "false".to_owned());
    data.insert("dns_query_log_enabled".to_owned(), "false".to_owned());
    data.insert("dns_query_log_retention_days".to_owned(), "14".to_owned());

    let svc = build_service_with_config(data);

    // `get_dns_config` is the runtime method -- no auth guard.
    let c = svc.get_dns_config().await.unwrap();

    assert!(c.enabled);
    assert_eq!(c.resolution_mode, DnsResolutionMode::Recursive);
    assert_eq!(c.upstream_servers.len(), 1);
    assert_eq!(c.upstream_servers[0].address, "9.9.9.9");
    assert_eq!(c.upstream_servers[0].name, "Quad9");
    assert_eq!(c.upstream_servers[0].protocol, DnsProtocol::Tls);
    assert_eq!(c.upstream_servers[0].port, Some(853));
    assert_eq!(c.cache_size, 50_000);
    assert_eq!(c.cache_ttl_min_secs, 60);
    assert_eq!(c.cache_ttl_max_secs, 3600);
    assert!(c.dnssec_enabled);
    assert!(!c.rebinding_protection);
    assert_eq!(c.rate_limit_per_second, 500);
    assert!(!c.ad_blocking_enabled);
    assert!(!c.query_log_enabled);
    assert_eq!(c.query_log_retention_days, 14);
}

#[tokio::test]
async fn get_config_requires_admin() {
    let svc = build_service();
    let result = auth_context::with_context(AuthContext::Anonymous, svc.get_config()).await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}

#[tokio::test]
async fn update_config_updates_multiple_fields() {
    let svc = build_service();

    let req = UpdateDnsConfigRequest {
        resolution_mode: None,
        upstream_servers: None,
        cache_size: None,
        cache_ttl_min_secs: None,
        cache_ttl_max_secs: None,
        dnssec_enabled: Some(true),
        rebinding_protection: Some(false),
        rate_limit_per_second: Some(250),
        ad_blocking_enabled: None,
        query_log_enabled: None,
        query_log_retention_days: None,
    };

    let resp = auth_context::with_context(admin_ctx(), svc.update_config(req))
        .await
        .unwrap();

    let c = &resp.config;
    assert!(c.dnssec_enabled);
    assert!(!c.rebinding_protection);
    assert_eq!(c.rate_limit_per_second, 250);

    // Defaults preserved for untouched fields.
    assert!(!c.enabled);
    assert_eq!(c.cache_size, 10_000);
    assert!(c.ad_blocking_enabled);
    assert!(c.query_log_enabled);
    assert_eq!(c.query_log_retention_days, 7);
}

// -- Ad-blocking tests --------------------------------------------------------

#[tokio::test]
async fn list_blocklists_requires_admin() {
    let svc = build_service();
    let result = auth_context::with_context(AuthContext::Anonymous, svc.list_blocklists()).await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}

#[tokio::test]
async fn create_blocklist_validates_url() {
    let fs = build_full_service();
    let req = CreateBlocklistRequest {
        name: "Test".to_owned(),
        url: "ftp://bad.example.com/list.txt".to_owned(),
        cron_schedule: "0 0 3 * * *".to_owned(),
        enabled: true,
    };
    let result = auth_context::with_context(admin_ctx(), fs.svc.create_blocklist(req)).await;
    assert!(matches!(result, Err(AppError::BadRequest(_))));
    if let Err(AppError::BadRequest(msg)) = result {
        assert!(
            msg.contains("http://"),
            "error should mention http, got: {msg}"
        );
    }
}

#[tokio::test]
async fn create_blocklist_validates_cron() {
    let fs = build_full_service();
    let req = CreateBlocklistRequest {
        name: "Test".to_owned(),
        url: "https://example.com/list.txt".to_owned(),
        cron_schedule: "not a cron".to_owned(),
        enabled: true,
    };
    let result = auth_context::with_context(admin_ctx(), fs.svc.create_blocklist(req)).await;
    assert!(matches!(result, Err(AppError::BadRequest(_))));
    if let Err(AppError::BadRequest(msg)) = result {
        assert!(
            msg.contains("cron"),
            "error should mention cron, got: {msg}"
        );
    }
}

#[tokio::test]
async fn create_blocklist_success() {
    let fs = build_full_service();
    let req = CreateBlocklistRequest {
        name: "Steven Black".to_owned(),
        url: "https://raw.githubusercontent.com/StevenBlack/hosts/master/hosts".to_owned(),
        cron_schedule: "0 0 3 * * *".to_owned(),
        enabled: true,
    };
    let resp = auth_context::with_context(admin_ctx(), fs.svc.create_blocklist(req))
        .await
        .unwrap();

    assert_eq!(resp.blocklist.name, "Steven Black");
    assert!(resp.blocklist.enabled);
    assert_eq!(resp.message, "blocklist created");

    // Verify event published.
    let events = fs.events.published_events();
    assert_eq!(events.len(), 1);
    assert!(matches!(events[0], WardnetEvent::DnsFiltersChanged { .. }));

    // Verify stored in repo.
    let stored = fs.dns_repo.list_blocklists().await.unwrap();
    assert_eq!(stored.len(), 1);
}

#[tokio::test]
async fn delete_blocklist_not_found() {
    let fs = build_full_service();
    let result =
        auth_context::with_context(admin_ctx(), fs.svc.delete_blocklist(Uuid::new_v4())).await;
    assert!(matches!(result, Err(AppError::NotFound(_))));
}

#[tokio::test]
async fn create_allowlist_validates_domain() {
    let fs = build_full_service();

    // Empty domain.
    let req = CreateAllowlistRequest {
        domain: String::new(),
        reason: None,
    };
    let result = auth_context::with_context(admin_ctx(), fs.svc.create_allowlist_entry(req)).await;
    assert!(matches!(result, Err(AppError::BadRequest(_))));

    // No dot in domain.
    let req = CreateAllowlistRequest {
        domain: "nodot".to_owned(),
        reason: None,
    };
    let result = auth_context::with_context(admin_ctx(), fs.svc.create_allowlist_entry(req)).await;
    assert!(matches!(result, Err(AppError::BadRequest(_))));

    // Invalid characters.
    let req = CreateAllowlistRequest {
        domain: "bad domain!.com".to_owned(),
        reason: None,
    };
    let result = auth_context::with_context(admin_ctx(), fs.svc.create_allowlist_entry(req)).await;
    assert!(matches!(result, Err(AppError::BadRequest(_))));
}

#[tokio::test]
async fn create_filter_rule_validates_rule_text() {
    let fs = build_full_service();

    // Empty / comment line.
    let req = CreateFilterRuleRequest {
        rule_text: "# this is a comment".to_owned(),
        comment: None,
        enabled: true,
    };
    let result = auth_context::with_context(admin_ctx(), fs.svc.create_filter_rule(req)).await;
    assert!(matches!(result, Err(AppError::BadRequest(_))));
}

#[tokio::test]
async fn create_filter_rule_success() {
    let fs = build_full_service();
    let req = CreateFilterRuleRequest {
        rule_text: "||ads.example.com^".to_owned(),
        comment: Some("block ads".to_owned()),
        enabled: true,
    };
    let resp = auth_context::with_context(admin_ctx(), fs.svc.create_filter_rule(req))
        .await
        .unwrap();

    assert_eq!(resp.rule.rule_text, "||ads.example.com^");
    assert!(resp.rule.enabled);
    assert_eq!(resp.rule.comment, Some("block ads".to_owned()));
    assert_eq!(resp.message, "filter rule created");

    // Verify event published.
    let events = fs.events.published_events();
    assert_eq!(events.len(), 1);
    assert!(matches!(events[0], WardnetEvent::DnsFiltersChanged { .. }));
}

#[tokio::test]
async fn load_filter_inputs_assembles_correctly() {
    let fs = build_full_service();

    // Seed some data.
    fs.dns_repo
        .blocked_domains
        .lock()
        .unwrap()
        .extend(vec!["ads.example.com".to_owned(), "tracker.net".to_owned()]);

    fs.dns_repo.allowlist.lock().unwrap().push(AllowlistEntry {
        id: Uuid::new_v4(),
        domain: "safe.example.com".to_owned(),
        reason: None,
        created_at: chrono::Utc::now(),
    });

    fs.dns_repo
        .custom_rules
        .lock()
        .unwrap()
        .push(CustomFilterRule {
            id: Uuid::new_v4(),
            rule_text: "||custom.block^".to_owned(),
            enabled: true,
            comment: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        });

    // Add a disabled rule that should be filtered out.
    fs.dns_repo
        .custom_rules
        .lock()
        .unwrap()
        .push(CustomFilterRule {
            id: Uuid::new_v4(),
            rule_text: "||disabled.rule^".to_owned(),
            enabled: false,
            comment: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        });

    let inputs = fs.svc.load_filter_inputs().await.unwrap();

    assert_eq!(inputs.blocked_domains.len(), 2);
    assert!(
        inputs
            .blocked_domains
            .contains(&"ads.example.com".to_owned())
    );
    assert!(inputs.blocked_domains.contains(&"tracker.net".to_owned()));
    assert_eq!(inputs.allowlist.len(), 1);
    assert_eq!(inputs.allowlist[0], "safe.example.com");
    // Only enabled rules.
    assert_eq!(inputs.custom_rules.len(), 1);
    assert_eq!(inputs.custom_rules[0], "||custom.block^");
}

// -- Additional service coverage tests ----------------------------------------

#[tokio::test]
async fn list_blocklists_success() {
    let fs = build_full_service();
    let resp = auth_context::with_context(admin_ctx(), fs.svc.list_blocklists())
        .await
        .unwrap();
    assert!(resp.blocklists.is_empty());
}

#[tokio::test]
async fn list_allowlist_success() {
    let fs = build_full_service();
    let resp = auth_context::with_context(admin_ctx(), fs.svc.list_allowlist())
        .await
        .unwrap();
    assert!(resp.entries.is_empty());
}

#[tokio::test]
async fn list_filter_rules_success() {
    let fs = build_full_service();
    let resp = auth_context::with_context(admin_ctx(), fs.svc.list_filter_rules())
        .await
        .unwrap();
    assert!(resp.rules.is_empty());
}

#[tokio::test]
async fn update_blocklist_success() {
    let fs = build_full_service();

    // Create a blocklist first.
    let create_req = CreateBlocklistRequest {
        name: "Test".to_owned(),
        url: "https://example.com/list.txt".to_owned(),
        cron_schedule: "0 0 3 * * *".to_owned(),
        enabled: true,
    };
    let created = auth_context::with_context(admin_ctx(), fs.svc.create_blocklist(create_req))
        .await
        .unwrap();
    let id = created.blocklist.id;

    // Update it.
    let update_req = wardnet_common::api::UpdateBlocklistRequest {
        name: Some("Renamed".to_owned()),
        url: None,
        enabled: None,
        cron_schedule: None,
    };
    let resp = auth_context::with_context(admin_ctx(), fs.svc.update_blocklist(id, update_req))
        .await
        .unwrap();

    assert_eq!(resp.blocklist.name, "Renamed");
    assert_eq!(resp.message, "blocklist updated");

    // Verify event published (create + update).
    let events = fs.events.published_events();
    assert_eq!(events.len(), 2);
    assert!(matches!(events[1], WardnetEvent::DnsFiltersChanged { .. }));
}

#[tokio::test]
async fn update_blocklist_not_found() {
    let fs = build_full_service();
    let update_req = wardnet_common::api::UpdateBlocklistRequest {
        name: Some("Renamed".to_owned()),
        url: None,
        enabled: None,
        cron_schedule: None,
    };
    let result = auth_context::with_context(
        admin_ctx(),
        fs.svc.update_blocklist(Uuid::new_v4(), update_req),
    )
    .await;
    assert!(matches!(result, Err(AppError::NotFound(_))));
}

#[tokio::test]
async fn delete_blocklist_success() {
    let fs = build_full_service();

    // Create first.
    let req = CreateBlocklistRequest {
        name: "ToDelete".to_owned(),
        url: "https://example.com/list.txt".to_owned(),
        cron_schedule: "0 0 3 * * *".to_owned(),
        enabled: true,
    };
    let created = auth_context::with_context(admin_ctx(), fs.svc.create_blocklist(req))
        .await
        .unwrap();
    let id = created.blocklist.id;

    // Delete it.
    let resp = auth_context::with_context(admin_ctx(), fs.svc.delete_blocklist(id))
        .await
        .unwrap();

    assert!(resp.message.contains("deleted"));

    // Verify events: create + delete.
    let events = fs.events.published_events();
    assert_eq!(events.len(), 2);
}

#[tokio::test]
async fn delete_allowlist_success() {
    let fs = build_full_service();

    // Create first.
    let req = CreateAllowlistRequest {
        domain: "safe.example.com".to_owned(),
        reason: None,
    };
    let created = auth_context::with_context(admin_ctx(), fs.svc.create_allowlist_entry(req))
        .await
        .unwrap();
    let id = created.entry.id;

    // Delete it.
    let resp = auth_context::with_context(admin_ctx(), fs.svc.delete_allowlist_entry(id))
        .await
        .unwrap();

    assert!(resp.message.contains("deleted"));

    let events = fs.events.published_events();
    assert_eq!(events.len(), 2);
}

#[tokio::test]
async fn update_filter_rule_success() {
    let fs = build_full_service();

    // Create first.
    let req = CreateFilterRuleRequest {
        rule_text: "||ads.example.com^".to_owned(),
        comment: None,
        enabled: true,
    };
    let created = auth_context::with_context(admin_ctx(), fs.svc.create_filter_rule(req))
        .await
        .unwrap();
    let id = created.rule.id;

    // Update it.
    let update_req = wardnet_common::api::UpdateFilterRuleRequest {
        rule_text: Some("||updated.example.com^".to_owned()),
        enabled: None,
        comment: Some("updated comment".to_owned()),
    };
    let resp = auth_context::with_context(admin_ctx(), fs.svc.update_filter_rule(id, update_req))
        .await
        .unwrap();

    assert_eq!(resp.rule.rule_text, "||updated.example.com^");
    assert_eq!(resp.message, "filter rule updated");

    let events = fs.events.published_events();
    assert_eq!(events.len(), 2);
}

#[tokio::test]
async fn update_filter_rule_not_found() {
    let fs = build_full_service();
    let req = wardnet_common::api::UpdateFilterRuleRequest {
        rule_text: Some("||x.com^".to_owned()),
        enabled: None,
        comment: None,
    };
    let result =
        auth_context::with_context(admin_ctx(), fs.svc.update_filter_rule(Uuid::new_v4(), req))
            .await;
    assert!(matches!(result, Err(AppError::NotFound(_))));
}

#[tokio::test]
async fn delete_filter_rule_success() {
    let fs = build_full_service();

    // Create first.
    let req = CreateFilterRuleRequest {
        rule_text: "||todelete.com^".to_owned(),
        comment: None,
        enabled: true,
    };
    let created = auth_context::with_context(admin_ctx(), fs.svc.create_filter_rule(req))
        .await
        .unwrap();
    let id = created.rule.id;

    // Delete it.
    let resp = auth_context::with_context(admin_ctx(), fs.svc.delete_filter_rule(id))
        .await
        .unwrap();

    assert!(resp.message.contains("deleted"));

    let events = fs.events.published_events();
    assert_eq!(events.len(), 2);
}

#[tokio::test]
async fn delete_filter_rule_not_found() {
    let fs = build_full_service();
    let result =
        auth_context::with_context(admin_ctx(), fs.svc.delete_filter_rule(Uuid::new_v4())).await;
    assert!(matches!(result, Err(AppError::NotFound(_))));
}

#[tokio::test]
async fn update_blocklist_now_success() {
    let fs = build_full_service();

    // Create first.
    let req = CreateBlocklistRequest {
        name: "Refresh me".to_owned(),
        url: "https://example.com/list.txt".to_owned(),
        cron_schedule: "0 0 3 * * *".to_owned(),
        enabled: true,
    };
    let created = auth_context::with_context(admin_ctx(), fs.svc.create_blocklist(req))
        .await
        .unwrap();
    let id = created.blocklist.id;

    let resp = auth_context::with_context(admin_ctx(), fs.svc.update_blocklist_now(id))
        .await
        .unwrap();

    assert_eq!(resp.message, "blocklist refresh triggered");
    assert_eq!(resp.blocklist.id, id);
}

#[tokio::test]
async fn update_blocklist_now_not_found() {
    let fs = build_full_service();
    let result =
        auth_context::with_context(admin_ctx(), fs.svc.update_blocklist_now(Uuid::new_v4())).await;
    assert!(matches!(result, Err(AppError::NotFound(_))));
}

#[tokio::test]
async fn delete_allowlist_not_found() {
    let fs = build_full_service();
    let result =
        auth_context::with_context(admin_ctx(), fs.svc.delete_allowlist_entry(Uuid::new_v4()))
            .await;
    assert!(matches!(result, Err(AppError::NotFound(_))));
}

// -- update_config: cover all remaining branches ---------------------------------

#[tokio::test]
async fn update_config_sets_resolution_mode() {
    let svc = build_service();
    let req = UpdateDnsConfigRequest {
        resolution_mode: Some("recursive".to_owned()),
        upstream_servers: None,
        cache_size: None,
        cache_ttl_min_secs: None,
        cache_ttl_max_secs: None,
        dnssec_enabled: None,
        rebinding_protection: None,
        rate_limit_per_second: None,
        ad_blocking_enabled: None,
        query_log_enabled: None,
        query_log_retention_days: None,
    };
    let resp = auth_context::with_context(admin_ctx(), svc.update_config(req))
        .await
        .unwrap();
    assert_eq!(resp.config.resolution_mode, DnsResolutionMode::Recursive);
}

#[tokio::test]
async fn update_config_sets_cache_ttl_min_and_max() {
    let svc = build_service();
    let req = UpdateDnsConfigRequest {
        resolution_mode: None,
        upstream_servers: None,
        cache_size: None,
        cache_ttl_min_secs: Some(30),
        cache_ttl_max_secs: Some(7200),
        dnssec_enabled: None,
        rebinding_protection: None,
        rate_limit_per_second: None,
        ad_blocking_enabled: None,
        query_log_enabled: None,
        query_log_retention_days: None,
    };
    let resp = auth_context::with_context(admin_ctx(), svc.update_config(req))
        .await
        .unwrap();
    assert_eq!(resp.config.cache_ttl_min_secs, 30);
    assert_eq!(resp.config.cache_ttl_max_secs, 7200);
}

#[tokio::test]
async fn update_config_sets_ad_blocking_enabled() {
    let svc = build_service();
    let req = UpdateDnsConfigRequest {
        resolution_mode: None,
        upstream_servers: None,
        cache_size: None,
        cache_ttl_min_secs: None,
        cache_ttl_max_secs: None,
        dnssec_enabled: None,
        rebinding_protection: None,
        rate_limit_per_second: None,
        ad_blocking_enabled: Some(false),
        query_log_enabled: None,
        query_log_retention_days: None,
    };
    let resp = auth_context::with_context(admin_ctx(), svc.update_config(req))
        .await
        .unwrap();
    assert!(!resp.config.ad_blocking_enabled);
}

#[tokio::test]
async fn update_config_sets_query_log_fields() {
    let svc = build_service();
    let req = UpdateDnsConfigRequest {
        resolution_mode: None,
        upstream_servers: None,
        cache_size: None,
        cache_ttl_min_secs: None,
        cache_ttl_max_secs: None,
        dnssec_enabled: None,
        rebinding_protection: None,
        rate_limit_per_second: None,
        ad_blocking_enabled: None,
        query_log_enabled: Some(false),
        query_log_retention_days: Some(14),
    };
    let resp = auth_context::with_context(admin_ctx(), svc.update_config(req))
        .await
        .unwrap();
    assert!(!resp.config.query_log_enabled);
    assert_eq!(resp.config.query_log_retention_days, 14);
}

// -- create_allowlist_entry success path ----------------------------------------

#[tokio::test]
async fn create_allowlist_entry_success() {
    let fs = build_full_service();
    let req = CreateAllowlistRequest {
        domain: "safe.example.com".to_owned(),
        reason: Some("false positive".to_owned()),
    };
    let resp = auth_context::with_context(admin_ctx(), fs.svc.create_allowlist_entry(req))
        .await
        .unwrap();

    assert_eq!(resp.entry.domain, "safe.example.com");
    assert_eq!(resp.entry.reason, Some("false positive".to_owned()));
    assert_eq!(resp.message, "allowlist entry created");

    // Verify event published.
    let events = fs.events.published_events();
    assert_eq!(events.len(), 1);
    assert!(matches!(events[0], WardnetEvent::DnsFiltersChanged { .. }));
}

// -- update_blocklist with URL/cron validation ---------------------------------

#[tokio::test]
async fn update_blocklist_validates_url() {
    let fs = build_full_service();

    // Create first.
    let create_req = CreateBlocklistRequest {
        name: "Test".to_owned(),
        url: "https://example.com/list.txt".to_owned(),
        cron_schedule: "0 0 3 * * *".to_owned(),
        enabled: true,
    };
    let created = auth_context::with_context(admin_ctx(), fs.svc.create_blocklist(create_req))
        .await
        .unwrap();
    let id = created.blocklist.id;

    // Update with invalid URL.
    let update_req = wardnet_common::api::UpdateBlocklistRequest {
        name: None,
        url: Some("ftp://bad.example.com/list.txt".to_owned()),
        enabled: None,
        cron_schedule: None,
    };
    let result =
        auth_context::with_context(admin_ctx(), fs.svc.update_blocklist(id, update_req)).await;
    assert!(matches!(result, Err(AppError::BadRequest(_))));
}

#[tokio::test]
async fn update_blocklist_validates_cron() {
    let fs = build_full_service();

    // Create first.
    let create_req = CreateBlocklistRequest {
        name: "Test".to_owned(),
        url: "https://example.com/list.txt".to_owned(),
        cron_schedule: "0 0 3 * * *".to_owned(),
        enabled: true,
    };
    let created = auth_context::with_context(admin_ctx(), fs.svc.create_blocklist(create_req))
        .await
        .unwrap();
    let id = created.blocklist.id;

    // Update with invalid cron.
    let update_req = wardnet_common::api::UpdateBlocklistRequest {
        name: None,
        url: None,
        enabled: None,
        cron_schedule: Some("not a cron".to_owned()),
    };
    let result =
        auth_context::with_context(admin_ctx(), fs.svc.update_blocklist(id, update_req)).await;
    assert!(matches!(result, Err(AppError::BadRequest(_))));
}

// -- update_filter_rule with rule_text validation ------------------------------

#[tokio::test]
async fn update_filter_rule_validates_rule_text() {
    let fs = build_full_service();

    // Create first.
    let create_req = CreateFilterRuleRequest {
        rule_text: "||ads.example.com^".to_owned(),
        comment: None,
        enabled: true,
    };
    let created = auth_context::with_context(admin_ctx(), fs.svc.create_filter_rule(create_req))
        .await
        .unwrap();
    let id = created.rule.id;

    // Update with invalid rule text (comment line).
    let update_req = wardnet_common::api::UpdateFilterRuleRequest {
        rule_text: Some("# this is a comment".to_owned()),
        enabled: None,
        comment: None,
    };
    let result =
        auth_context::with_context(admin_ctx(), fs.svc.update_filter_rule(id, update_req)).await;
    assert!(matches!(result, Err(AppError::BadRequest(_))));
}

// -- auth guard coverage for remaining service methods -------------------------

#[tokio::test]
async fn list_allowlist_requires_admin() {
    let svc = build_service();
    let result = auth_context::with_context(AuthContext::Anonymous, svc.list_allowlist()).await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}

#[tokio::test]
async fn list_filter_rules_requires_admin() {
    let svc = build_service();
    let result = auth_context::with_context(AuthContext::Anonymous, svc.list_filter_rules()).await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}

#[tokio::test]
async fn create_allowlist_requires_admin() {
    let svc = build_service();
    let req = CreateAllowlistRequest {
        domain: "safe.example.com".to_owned(),
        reason: None,
    };
    let result =
        auth_context::with_context(AuthContext::Anonymous, svc.create_allowlist_entry(req)).await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}

#[tokio::test]
async fn toggle_requires_admin() {
    let svc = build_service();
    let result = auth_context::with_context(
        AuthContext::Anonymous,
        svc.toggle(ToggleDnsRequest { enabled: true }),
    )
    .await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}

#[tokio::test]
async fn status_requires_admin() {
    let svc = build_service();
    let result = auth_context::with_context(AuthContext::Anonymous, svc.status()).await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}

#[tokio::test]
async fn flush_cache_requires_admin() {
    let svc = build_service();
    let result = auth_context::with_context(AuthContext::Anonymous, svc.flush_cache()).await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}

// -- validate_domain edge case: domain > 253 chars ----------------------------

#[tokio::test]
async fn create_allowlist_domain_too_long() {
    let fs = build_full_service();
    let long_domain = format!("{}.example.com", "a".repeat(250));
    let req = CreateAllowlistRequest {
        domain: long_domain,
        reason: None,
    };
    let result = auth_context::with_context(admin_ctx(), fs.svc.create_allowlist_entry(req)).await;
    assert!(matches!(result, Err(AppError::BadRequest(_))));
}

// -- create_filter_rule with invalid (parser-error) rule text -----------------

#[tokio::test]
async fn create_filter_rule_invalid_parse_error() {
    let fs = build_full_service();
    let req = CreateFilterRuleRequest {
        rule_text: "/invalid[regex/".to_owned(),
        comment: None,
        enabled: true,
    };
    let result = auth_context::with_context(admin_ctx(), fs.svc.create_filter_rule(req)).await;
    assert!(matches!(result, Err(AppError::BadRequest(_))));
}
