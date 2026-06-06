//! TLS service-layer tests. Auth gating, the inert-when-unconfigured
//! short-circuit, and the renewal decision use hand-written mocks; the live
//! instant-acme order dance is deferred to the bridge-live integration test, so
//! nothing here makes a real ACME call. Certificate parsing is exercised against
//! an rcgen-generated cert.

use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::sync::Arc;
use std::sync::Mutex;

use async_trait::async_trait;
use chrono::{Duration, Utc};
use sqlx::SqlitePool;
use sqlx::sqlite::SqlitePoolOptions;
use uuid::Uuid;
use wardnet_common::api::CreateRecordRequest;
use wardnet_common::auth::AuthContext;
use wardnet_common::dns::{DnsRecordSource, DnsRecordType};
use wardnetd_data::repository::{SqliteDnsLocalRepository, SystemConfigRepository};

use crate::auth_context;
use crate::ddns::{DdnsRegistration, DdnsService, DdnsStatus};
use crate::dns_local::{DnsLocalService, DnsLocalServiceImpl};
use crate::error::AppError;
use crate::event::BroadcastEventBus;
use crate::secret_store::SecretStore;
use crate::tls::{
    CertActivator, KEY_CERT_DOMAIN, KEY_CERT_NOT_AFTER, TlsService, TlsServiceImpl, TlsStatus,
    within_renewal_window,
};

/// A fixed LAN IP for the split-horizon record under test.
const TEST_LAN_IP: Ipv4Addr = Ipv4Addr::new(192, 168, 1, 1);

/// Build a real in-memory [`DnsLocalService`] (same migration set as the rest of
/// the workspace) so the split-horizon reconcile can be asserted end-to-end.
async fn new_dns_local() -> Arc<dyn DnsLocalService> {
    let pool: SqlitePool = SqlitePoolOptions::new()
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("../wardnetd-data/migrations")
        .run(&pool)
        .await
        .unwrap();
    let repo = Arc::new(SqliteDnsLocalRepository::new(pool));
    let events: Arc<dyn crate::event::EventPublisher> = Arc::new(BroadcastEventBus::new(16));
    Arc::new(DnsLocalServiceImpl::new(repo, events))
}

// ── Mocks ────────────────────────────────────────────────────────────────────────

#[derive(Default)]
struct MockSystemConfig {
    store: Mutex<HashMap<String, String>>,
}

#[async_trait]
impl SystemConfigRepository for MockSystemConfig {
    async fn get(&self, key: &str) -> anyhow::Result<Option<String>> {
        Ok(self.store.lock().unwrap().get(key).cloned())
    }
    async fn set(&self, key: &str, value: &str) -> anyhow::Result<()> {
        self.store
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

#[derive(Default)]
struct MockSecretStore {
    store: Mutex<HashMap<String, Vec<u8>>>,
}

#[async_trait]
impl SecretStore for MockSecretStore {
    async fn put(&self, path: &str, value: &[u8]) -> anyhow::Result<()> {
        self.store
            .lock()
            .unwrap()
            .insert(path.to_owned(), value.to_vec());
        Ok(())
    }
    async fn get(&self, path: &str) -> anyhow::Result<Option<Vec<u8>>> {
        Ok(self.store.lock().unwrap().get(path).cloned())
    }
    async fn delete(&self, path: &str) -> anyhow::Result<()> {
        self.store.lock().unwrap().remove(path);
        Ok(())
    }
    async fn list(&self, prefix: &str) -> anyhow::Result<Vec<String>> {
        Ok(self
            .store
            .lock()
            .unwrap()
            .keys()
            .filter(|k| k.starts_with(prefix))
            .cloned()
            .collect())
    }
}

/// A DDNS mock whose only behaviour that matters here is `status().fqdn`. ACME
/// challenge methods would only be reached on a live issuance, which these tests
/// never trigger.
struct MockDdns {
    fqdn: Option<String>,
}

#[async_trait]
impl DdnsService for MockDdns {
    async fn register_with_bridge(&self, _name: String) -> Result<DdnsRegistration, AppError> {
        unreachable!("not used in TLS tests")
    }
    async fn check_name_available(&self, _name: String) -> Result<bool, AppError> {
        unreachable!("not used in TLS tests")
    }
    async fn refresh_public_ip(&self) -> Result<Option<Ipv4Addr>, AppError> {
        unreachable!("not used in TLS tests")
    }
    async fn status(&self) -> Result<DdnsStatus, AppError> {
        Ok(DdnsStatus {
            provider: self.fqdn.as_ref().map(|_| "bridge".to_owned()),
            fqdn: self.fqdn.clone(),
            last_public_ip: None,
        })
    }
    async fn set_acme_challenge(&self, _value: &str) -> Result<(), AppError> {
        unreachable!("no live issuance in TLS tests")
    }
    async fn clear_acme_challenge(&self) -> Result<(), AppError> {
        unreachable!("no live issuance in TLS tests")
    }
}

/// Records whether `activate` was called (and with which fqdn), so tests can
/// assert the cert is only swapped on an actual issuance and carries the domain.
#[derive(Default)]
struct MockActivator {
    activated: Mutex<bool>,
    fqdn: Mutex<Option<String>>,
}

#[async_trait]
impl CertActivator for MockActivator {
    async fn activate(
        &self,
        _chain_pem: Vec<u8>,
        _key_pem: Vec<u8>,
        fqdn: String,
    ) -> anyhow::Result<()> {
        *self.activated.lock().unwrap() = true;
        *self.fqdn.lock().unwrap() = Some(fqdn);
        Ok(())
    }
}

// ── Helpers ────────────────────────────────────────────────────────────────────

fn admin_ctx() -> AuthContext {
    AuthContext::Admin {
        admin_id: Uuid::nil(),
    }
}

fn build_service(
    config: Arc<MockSystemConfig>,
    secrets: Arc<MockSecretStore>,
    fqdn: Option<&str>,
    activator: Arc<MockActivator>,
    dns_local: Arc<dyn DnsLocalService>,
) -> TlsServiceImpl {
    TlsServiceImpl::new(
        config,
        secrets,
        Arc::new(MockDdns {
            fqdn: fqdn.map(str::to_owned),
        }),
        activator,
        dns_local,
        TEST_LAN_IP,
    )
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn ensure_certificate_requires_admin() {
    let svc = build_service(
        Arc::new(MockSystemConfig::default()),
        Arc::new(MockSecretStore::default()),
        Some("home.example.net"),
        Arc::new(MockActivator::default()),
        new_dns_local().await,
    );
    assert!(matches!(
        svc.ensure_certificate().await.unwrap_err(),
        AppError::Forbidden(_)
    ));
}

#[tokio::test]
async fn ensure_certificate_inert_when_no_fqdn() {
    let activator = Arc::new(MockActivator::default());
    let svc = build_service(
        Arc::new(MockSystemConfig::default()),
        Arc::new(MockSecretStore::default()),
        None,
        activator.clone(),
        new_dns_local().await,
    );
    let status = auth_context::with_context(admin_ctx(), svc.ensure_certificate())
        .await
        .unwrap();
    assert_eq!(status, TlsStatus::NotConfigured);
    assert!(!*activator.activated.lock().unwrap(), "must not swap cert");
}

#[tokio::test]
async fn ensure_certificate_up_to_date_when_cert_fresh() {
    let config = Arc::new(MockSystemConfig::default());
    config
        .set(KEY_CERT_DOMAIN, "home.example.net")
        .await
        .unwrap();
    let not_after = Utc::now() + Duration::days(60);
    config
        .set(KEY_CERT_NOT_AFTER, &not_after.to_rfc3339())
        .await
        .unwrap();

    let activator = Arc::new(MockActivator::default());
    let svc = build_service(
        config,
        Arc::new(MockSecretStore::default()),
        Some("home.example.net"),
        activator.clone(),
        new_dns_local().await,
    );
    let status = auth_context::with_context(admin_ctx(), svc.ensure_certificate())
        .await
        .unwrap();
    assert!(matches!(status, TlsStatus::UpToDate { .. }));
    assert!(
        !*activator.activated.lock().unwrap(),
        "fresh cert must not be re-issued"
    );
}

/// Even on the steady-state `UpToDate` path (no re-issuance), the split-horizon
/// `<fqdn> -> lan_ip` system record is reconciled so it self-heals.
#[tokio::test]
async fn ensure_certificate_reconciles_split_horizon_when_fresh() {
    let config = Arc::new(MockSystemConfig::default());
    config
        .set(KEY_CERT_DOMAIN, "home.example.net")
        .await
        .unwrap();
    config
        .set(
            KEY_CERT_NOT_AFTER,
            &(Utc::now() + Duration::days(60)).to_rfc3339(),
        )
        .await
        .unwrap();

    let dns_local = new_dns_local().await;
    let svc = build_service(
        config,
        Arc::new(MockSecretStore::default()),
        Some("home.example.net"),
        Arc::new(MockActivator::default()),
        dns_local.clone(),
    );

    auth_context::with_context(admin_ctx(), svc.ensure_certificate())
        .await
        .unwrap();

    let records = auth_context::with_context(admin_ctx(), dns_local.list_records())
        .await
        .unwrap()
        .records;
    let split = records
        .iter()
        .find(|r| r.domain == "home.example.net")
        .expect("split-horizon record reconciled");
    assert_eq!(split.value, TEST_LAN_IP.to_string());
    assert_eq!(split.record_type, wardnet_common::dns::DnsRecordType::A);
    assert_eq!(split.source, wardnet_common::dns::DnsRecordSource::System);
    assert_eq!(split.zone_id, None);
}

/// On a canonical-FQDN change the old domain's `system` record is removed and the
/// new one created. Exercises the private `reconcile_split_horizon` directly
/// (the live-issuance path that triggers it in production needs a real ACME call).
#[tokio::test]
async fn reconcile_split_horizon_swaps_domain_on_change() {
    let dns_local = new_dns_local().await;
    let svc = build_service(
        Arc::new(MockSystemConfig::default()),
        Arc::new(MockSecretStore::default()),
        Some("b.example.net"),
        Arc::new(MockActivator::default()),
        dns_local.clone(),
    );

    auth_context::with_context(
        admin_ctx(),
        svc.reconcile_split_horizon("a.example.net", None),
    )
    .await;
    auth_context::with_context(
        admin_ctx(),
        svc.reconcile_split_horizon("b.example.net", Some("a.example.net")),
    )
    .await;

    let records = auth_context::with_context(admin_ctx(), dns_local.list_records())
        .await
        .unwrap()
        .records;
    assert!(
        records
            .iter()
            .any(|r| r.domain == "b.example.net" && r.source == DnsRecordSource::System),
        "new system record present"
    );
    assert!(
        !records.iter().any(|r| r.domain == "a.example.net"),
        "old system record pruned"
    );
}

/// The stale-record prune must only touch `system` records — a user's `manual`
/// record for the same name is sacrosanct. This is the one path here that could
/// destroy user data, so it is pinned explicitly.
#[tokio::test]
async fn reconcile_split_horizon_spares_user_records() {
    let dns_local = new_dns_local().await;
    // Seed a *manual* A record for the soon-to-be-old domain.
    auth_context::with_context(
        admin_ctx(),
        dns_local.create_record(CreateRecordRequest {
            zone_id: None,
            domain: "old.example.net".to_owned(),
            record_type: DnsRecordType::A,
            value: "10.9.9.9".to_owned(),
            ttl: 300,
            enabled: true,
        }),
    )
    .await
    .unwrap();

    let svc = build_service(
        Arc::new(MockSystemConfig::default()),
        Arc::new(MockSecretStore::default()),
        Some("new.example.net"),
        Arc::new(MockActivator::default()),
        dns_local.clone(),
    );
    // Reconcile to a new domain, claiming `old.example.net` was the previous one.
    auth_context::with_context(
        admin_ctx(),
        svc.reconcile_split_horizon("new.example.net", Some("old.example.net")),
    )
    .await;

    let records = auth_context::with_context(admin_ctx(), dns_local.list_records())
        .await
        .unwrap()
        .records;
    let manual = records
        .iter()
        .find(|r| r.domain == "old.example.net")
        .expect("manual record must survive the prune");
    assert_eq!(manual.source, DnsRecordSource::Manual);
    assert_eq!(manual.value, "10.9.9.9");
}

#[tokio::test]
async fn status_reflects_stored_state() {
    // No FQDN → NotConfigured.
    let svc = build_service(
        Arc::new(MockSystemConfig::default()),
        Arc::new(MockSecretStore::default()),
        None,
        Arc::new(MockActivator::default()),
        new_dns_local().await,
    );
    assert_eq!(
        auth_context::with_context(admin_ctx(), svc.status())
            .await
            .unwrap(),
        TlsStatus::NotConfigured
    );

    // FQDN but no stored cert → Pending.
    let svc = build_service(
        Arc::new(MockSystemConfig::default()),
        Arc::new(MockSecretStore::default()),
        Some("home.example.net"),
        Arc::new(MockActivator::default()),
        new_dns_local().await,
    );
    assert!(matches!(
        auth_context::with_context(admin_ctx(), svc.status())
            .await
            .unwrap(),
        TlsStatus::Pending { .. }
    ));

    // Fresh stored cert for the active domain → UpToDate.
    let config = Arc::new(MockSystemConfig::default());
    config
        .set(KEY_CERT_DOMAIN, "home.example.net")
        .await
        .unwrap();
    config
        .set(
            KEY_CERT_NOT_AFTER,
            &(Utc::now() + Duration::days(60)).to_rfc3339(),
        )
        .await
        .unwrap();
    let svc = build_service(
        config,
        Arc::new(MockSecretStore::default()),
        Some("home.example.net"),
        Arc::new(MockActivator::default()),
        new_dns_local().await,
    );
    assert!(matches!(
        auth_context::with_context(admin_ctx(), svc.status())
            .await
            .unwrap(),
        TlsStatus::UpToDate { .. }
    ));

    // Stored cert within the renewal window → NeedsRenewal.
    let config = Arc::new(MockSystemConfig::default());
    config
        .set(KEY_CERT_DOMAIN, "home.example.net")
        .await
        .unwrap();
    config
        .set(
            KEY_CERT_NOT_AFTER,
            &(Utc::now() + Duration::days(5)).to_rfc3339(),
        )
        .await
        .unwrap();
    let svc = build_service(
        config,
        Arc::new(MockSecretStore::default()),
        Some("home.example.net"),
        Arc::new(MockActivator::default()),
        new_dns_local().await,
    );
    assert!(matches!(
        auth_context::with_context(admin_ctx(), svc.status())
            .await
            .unwrap(),
        TlsStatus::NeedsRenewal { .. }
    ));
}

#[tokio::test]
async fn status_pending_when_stored_cert_is_for_a_different_domain() {
    // Stored cert is for a *different* domain than the active FQDN → Pending.
    let config = Arc::new(MockSystemConfig::default());
    config
        .set(KEY_CERT_DOMAIN, "old.example.net")
        .await
        .unwrap();
    config
        .set(
            KEY_CERT_NOT_AFTER,
            &(Utc::now() + Duration::days(60)).to_rfc3339(),
        )
        .await
        .unwrap();
    let svc = build_service(
        config,
        Arc::new(MockSecretStore::default()),
        Some("home.example.net"),
        Arc::new(MockActivator::default()),
        new_dns_local().await,
    );
    assert!(matches!(
        auth_context::with_context(admin_ctx(), svc.status())
            .await
            .unwrap(),
        TlsStatus::Pending { .. }
    ));
}

#[test]
fn renewal_window_threshold() {
    let now = Utc::now();
    // Outside the window → no renewal.
    assert!(!within_renewal_window(now + Duration::days(60), now));
    assert!(!within_renewal_window(now + Duration::days(31), now));
    // Inside / at / past the window → renew.
    assert!(within_renewal_window(now + Duration::days(29), now));
    assert!(within_renewal_window(now + Duration::days(30), now));
    assert!(within_renewal_window(now - Duration::days(1), now));
}

#[test]
fn parse_not_after_reads_certificate_validity() {
    // rcgen's default validity runs to the year 4096 — a stable, far-future
    // anchor we can assert on without depending on the `time` crate.
    let key_pair = rcgen::KeyPair::generate().unwrap();
    let params = rcgen::CertificateParams::new(vec!["home.example.net".to_owned()]).unwrap();
    let cert = params.self_signed(&key_pair).unwrap();

    let not_after = crate::tls::acme::parse_not_after(cert.pem().as_bytes()).unwrap();
    assert!(
        not_after > Utc::now() + Duration::days(3650),
        "expected far-future not_after, got {not_after}"
    );
}
