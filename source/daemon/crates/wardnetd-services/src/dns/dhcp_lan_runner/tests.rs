//! Tests for the DHCP `.lan` runner: the pure `lan_label` helper and the
//! `register_lease` upsert logic (against a real SQLite-backed
//! `DnsLocalServiceImpl` whose `.lan` zone is migration-seeded) plus a minimal
//! `DhcpService` mock that only answers `get_dhcp_config`.

use std::net::Ipv4Addr;
use std::sync::Arc;

use async_trait::async_trait;
use sqlx::SqlitePool;
use sqlx::sqlite::SqlitePoolOptions;
use uuid::Uuid;
use wardnet_common::auth::AuthContext;
use wardnet_common::dhcp::{DhcpConfig, DhcpLease};
use wardnetd_data::repository::SqliteDnsLocalRepository;

use super::{LAN_ZONE_ID, lan_label, register_lease};
use crate::auth_context;
use crate::dhcp::DhcpService;
use crate::dns_local::service::{DnsLocalService, DnsLocalServiceImpl};
use crate::error::AppError;

// ── lan_label (pure) ──────────────────────────────────────────────────────

#[test]
fn lan_label_keeps_first_label_lowercased() {
    assert_eq!(lan_label("MyPC.home.arpa").as_deref(), Some("mypc"));
    assert_eq!(lan_label("nas").as_deref(), Some("nas"));
    assert_eq!(lan_label("  Trimmed  ").as_deref(), Some("trimmed"));
}

#[test]
fn lan_label_rejects_empty() {
    assert_eq!(lan_label(""), None);
    assert_eq!(lan_label("   "), None);
    assert_eq!(lan_label("."), None);
}

// ── register_lease (real service + mock dhcp) ─────────────────────────────

/// Minimal `DhcpService` mock — only `get_dhcp_config` is exercised.
/// `lease_secs == None` makes it error so the fallback-TTL path is hit.
struct MockDhcp {
    lease_secs: Option<u32>,
}

fn dhcp_config(lease_duration_secs: u32) -> DhcpConfig {
    DhcpConfig {
        enabled: true,
        gateway_ip: Ipv4Addr::new(10, 0, 0, 1),
        pool_start: Ipv4Addr::new(10, 0, 0, 100),
        pool_end: Ipv4Addr::new(10, 0, 0, 200),
        subnet_mask: Ipv4Addr::new(255, 255, 255, 0),
        upstream_dns: vec![Ipv4Addr::new(9, 9, 9, 9)],
        lease_duration_secs,
        router_ip: None,
    }
}

#[async_trait]
impl DhcpService for MockDhcp {
    async fn get_dhcp_config(&self) -> Result<DhcpConfig, AppError> {
        match self.lease_secs {
            Some(n) => Ok(dhcp_config(n)),
            None => Err(AppError::Internal(anyhow::anyhow!(
                "mock dhcp config error"
            ))),
        }
    }

    async fn get_config(&self) -> Result<wardnet_common::api::DhcpConfigResponse, AppError> {
        unimplemented!()
    }
    async fn update_config(
        &self,
        _req: wardnet_common::api::UpdateDhcpConfigRequest,
    ) -> Result<wardnet_common::api::DhcpConfigResponse, AppError> {
        unimplemented!()
    }
    async fn preview_config(
        &self,
        _req: wardnet_common::api::PreviewDhcpConfigRequest,
    ) -> Result<wardnet_common::api::PreviewDhcpConfigResponse, AppError> {
        Ok(wardnet_common::api::PreviewDhcpConfigResponse {
            affected: Vec::new(),
        })
    }
    async fn toggle(
        &self,
        _req: wardnet_common::api::ToggleDhcpRequest,
    ) -> Result<wardnet_common::api::DhcpConfigResponse, AppError> {
        unimplemented!()
    }
    async fn list_leases(&self) -> Result<wardnet_common::api::ListDhcpLeasesResponse, AppError> {
        unimplemented!()
    }
    async fn revoke_lease(
        &self,
        _id: Uuid,
    ) -> Result<wardnet_common::api::RevokeDhcpLeaseResponse, AppError> {
        unimplemented!()
    }
    async fn list_reservations(
        &self,
    ) -> Result<wardnet_common::api::ListDhcpReservationsResponse, AppError> {
        unimplemented!()
    }
    async fn create_reservation(
        &self,
        _req: wardnet_common::api::CreateDhcpReservationRequest,
    ) -> Result<wardnet_common::api::CreateDhcpReservationResponse, AppError> {
        unimplemented!()
    }
    async fn delete_reservation(
        &self,
        _id: Uuid,
    ) -> Result<wardnet_common::api::DeleteDhcpReservationResponse, AppError> {
        unimplemented!()
    }
    async fn status(&self) -> Result<wardnet_common::api::DhcpStatusResponse, AppError> {
        unimplemented!()
    }
    async fn assign_lease(
        &self,
        _mac: &str,
        _hostname: Option<&str>,
    ) -> Result<DhcpLease, AppError> {
        unimplemented!()
    }
    async fn renew_lease(
        &self,
        _mac: &str,
        _hostname: Option<&str>,
    ) -> Result<DhcpLease, AppError> {
        unimplemented!()
    }
    async fn release_lease(&self, _mac: &str) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn cleanup_expired(&self) -> Result<u64, AppError> {
        unimplemented!()
    }
}

async fn build_service() -> (DnsLocalServiceImpl, SqlitePool) {
    let pool: SqlitePool = SqlitePoolOptions::new()
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("../wardnetd-data/migrations")
        .run(&pool)
        .await
        .unwrap();
    let repo = Arc::new(SqliteDnsLocalRepository::new(pool.clone()));
    let events: Arc<dyn crate::event::EventPublisher> =
        Arc::new(crate::event::BroadcastEventBus::new(16));
    (DnsLocalServiceImpl::new(repo, events), pool)
}

fn admin() -> AuthContext {
    AuthContext::Admin {
        admin_id: Uuid::nil(),
    }
}

async fn lan_records(svc: &DnsLocalServiceImpl) -> Vec<wardnet_common::dns::CustomDnsRecord> {
    auth_context::with_context(admin(), svc.list_records())
        .await
        .unwrap()
        .records
}

#[tokio::test]
async fn register_lease_upserts_lan_record_with_half_lease_ttl() {
    let (svc, _pool) = build_service().await;
    let dhcp = MockDhcp {
        lease_secs: Some(600),
    };
    register_lease(&svc, &dhcp, &admin(), "MyPC.home.arpa", "10.0.0.42").await;

    let records = lan_records(&svc).await;
    let rec = records
        .iter()
        .find(|r| r.domain == "mypc.lan")
        .expect("mypc.lan record registered");
    assert_eq!(rec.value, "10.0.0.42");
    assert_eq!(rec.ttl, 300); // 600 / 2
    assert_eq!(rec.source, wardnet_common::dns::DnsRecordSource::Dhcp);
}

#[tokio::test]
async fn register_lease_uses_fallback_ttl_when_config_unreadable() {
    let (svc, _pool) = build_service().await;
    let dhcp = MockDhcp { lease_secs: None }; // get_dhcp_config errors
    register_lease(&svc, &dhcp, &admin(), "router", "10.0.0.1").await;

    let records = lan_records(&svc).await;
    let rec = records
        .iter()
        .find(|r| r.domain == "router.lan")
        .expect("router.lan record registered");
    assert_eq!(rec.ttl, super::FALLBACK_TTL_SECS); // 300
}

#[tokio::test]
async fn register_lease_skips_empty_hostname() {
    let (svc, _pool) = build_service().await;
    let dhcp = MockDhcp {
        lease_secs: Some(600),
    };
    register_lease(&svc, &dhcp, &admin(), "   ", "10.0.0.7").await;
    assert!(lan_records(&svc).await.is_empty());
}

#[tokio::test]
async fn register_lease_skips_when_lan_zone_missing() {
    let (svc, pool) = build_service().await;
    // Remove the seeded `.lan` zone via raw SQL (the service guards the
    // system-sourced zone against deletion) so the NotFound soft-skip runs.
    sqlx::query("DELETE FROM dns_zones WHERE id = ?")
        .bind(LAN_ZONE_ID.to_string())
        .execute(&pool)
        .await
        .unwrap();
    let dhcp = MockDhcp {
        lease_secs: Some(600),
    };
    register_lease(&svc, &dhcp, &admin(), "ghost", "10.0.0.9").await;
    assert!(lan_records(&svc).await.is_empty());
}

// ── Runner lifecycle (start → event → shutdown) ───────────────────────────

#[tokio::test]
async fn runner_registers_lease_from_dhcp_event() {
    use std::time::Duration;

    use wardnet_common::event::WardnetEvent;

    use crate::event::{BroadcastEventBus, EventPublisher};

    let (svc_impl, _pool) = build_service().await;
    let svc: Arc<dyn DnsLocalService> = Arc::new(svc_impl);
    let dhcp: Arc<dyn DhcpService> = Arc::new(MockDhcp {
        lease_secs: Some(600),
    });
    let events = BroadcastEventBus::new(16);
    let span = tracing::Span::none();

    let runner = super::DhcpLanRunner::start(svc.clone(), dhcp, &events, &span);

    // The runner subscribed before spawning, so this is buffered and delivered.
    events.publish(WardnetEvent::DhcpLeaseAssigned {
        lease_id: Uuid::nil(),
        mac: "aa:bb:cc:dd:ee:ff".to_owned(),
        ip: "10.0.0.50".to_owned(),
        hostname: Some("Laptop".to_owned()),
        timestamp: chrono::Utc::now(),
    });

    // Poll until the runner has processed the event (or time out).
    let mut found = false;
    for _ in 0..100 {
        let recs = auth_context::with_context(admin(), svc.list_records())
            .await
            .unwrap()
            .records;
        if recs.iter().any(|r| r.domain == "laptop.lan") {
            found = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    runner.shutdown().await;
    assert!(
        found,
        "runner should register laptop.lan from the lease event"
    );
}
