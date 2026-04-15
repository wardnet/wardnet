use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use uuid::Uuid;
use wardnet_types::api::{ToggleDnsRequest, UpdateDnsConfigRequest, UpstreamDnsRequest};
use wardnet_types::auth::AuthContext;
use wardnet_types::dns::{DnsProtocol, DnsResolutionMode};

use crate::auth_context;
use crate::error::AppError;
use crate::repository::SystemConfigRepository;
use crate::service::{DnsService, DnsServiceImpl};

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

// -- Helpers ------------------------------------------------------------------

fn admin_ctx() -> AuthContext {
    AuthContext::Admin {
        admin_id: Uuid::new_v4(),
    }
}

fn build_service() -> DnsServiceImpl {
    let system_config = Arc::new(MockSystemConfigRepository::new());
    DnsServiceImpl::new(system_config)
}

fn build_service_with_config(data: HashMap<String, String>) -> DnsServiceImpl {
    let system_config = Arc::new(MockSystemConfigRepository::with_data(data));
    DnsServiceImpl::new(system_config)
}

fn build_service_with_repo() -> (DnsServiceImpl, Arc<MockSystemConfigRepository>) {
    let repo = Arc::new(MockSystemConfigRepository::new());
    let svc = DnsServiceImpl::new(repo.clone());
    (svc, repo)
}

// -- Tests --------------------------------------------------------------------

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
