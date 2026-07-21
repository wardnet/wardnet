//! Service-level tests for [`RoutingProfileServiceImpl`] over an in-memory DB.
//!
//! Covers the DNS-facing surface the API tests don't reach: `note_resolution`
//! (each early-return branch and the happy path that queues an enforcement
//! request), the compiled-view refresh that feeds it, and `validate_target`'s
//! unknown-tunnel rejection. CRUD happy paths and conflicts are covered by the
//! API-layer tests, which drive the same service.

use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;

use async_trait::async_trait;
use sqlx::SqlitePool;
use sqlx::sqlite::SqlitePoolOptions;
use tokio::sync::mpsc;
use uuid::Uuid;
use wardnet_common::api::CreateTunnelRequest;
use wardnet_common::auth::AuthContext;
use wardnet_common::routing_profile::DomainRoutingTarget;
use wardnet_common::tunnel::Tunnel;
use wardnetd_data::repository::SqliteRoutingProfileRepository;

use crate::TunnelService;
use crate::auth_context;
use crate::error::AppError;
use crate::routing_profile::service::{
    DomainRouteRequest, RoutingProfileService, RoutingProfileServiceImpl,
};

fn admin_ctx() -> AuthContext {
    AuthContext::Admin {
        admin_id: Uuid::new_v4(),
    }
}

async fn test_pool() -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("../wardnetd-data/migrations")
        .run(&pool)
        .await
        .unwrap();
    pool
}

async fn insert_device(pool: &SqlitePool, id: Uuid, last_ip: &str) {
    sqlx::query(
        "INSERT INTO devices (id, mac, last_ip, device_type, first_seen, last_seen, zone_id) \
         VALUES (?, ?, ?, 'unknown', ?, ?, '00000000-0000-0000-0000-000000000201')",
    )
    .bind(id.to_string())
    .bind(format!("aa:bb:cc:dd:ee:{:02x}", (id.as_u128() & 0xff) as u8))
    .bind(last_ip)
    .bind("2026-07-21T00:00:00Z")
    .bind("2026-07-21T00:00:00Z")
    .execute(pool)
    .await
    .unwrap();
}

/// A `TunnelService` whose `get_tunnel` always reports "not found" (so
/// `validate_target`'s unknown-tunnel branch runs) and whose other methods are
/// never called by these tests.
struct MissingTunnelService;

#[async_trait]
impl TunnelService for MissingTunnelService {
    async fn get_tunnel(&self, _id: Uuid) -> Result<Tunnel, AppError> {
        Err(AppError::NotFound("tunnel not found".to_owned()))
    }
    async fn import_tunnel(
        &self,
        _req: CreateTunnelRequest,
    ) -> Result<wardnet_common::api::CreateTunnelResponse, AppError> {
        unimplemented!("not exercised by routing-profile service tests")
    }
    async fn list_tunnels(&self) -> Result<wardnet_common::api::ListTunnelsResponse, AppError> {
        unimplemented!("not exercised by routing-profile service tests")
    }
    async fn test_tunnel(
        &self,
        _id: Uuid,
    ) -> Result<wardnet_common::api::TunnelTestResult, AppError> {
        unimplemented!("not exercised by routing-profile service tests")
    }
    async fn list_tunnel_devices(
        &self,
        _id: Uuid,
    ) -> Result<wardnet_common::api::TunnelDevicesResponse, AppError> {
        unimplemented!("not exercised by routing-profile service tests")
    }
    async fn set_dns_override(&self, _id: Uuid, _value: bool) -> Result<Tunnel, AppError> {
        unimplemented!("not exercised by routing-profile service tests")
    }
    async fn rebuild(&self, _id: Uuid) -> Result<(), AppError> {
        unimplemented!("not exercised by routing-profile service tests")
    }
    async fn bring_up(&self, _id: Uuid) -> Result<(), AppError> {
        unimplemented!("not exercised by routing-profile service tests")
    }
    async fn tear_down(&self, _id: Uuid, _reason: &str) -> Result<(), AppError> {
        unimplemented!("not exercised by routing-profile service tests")
    }
    async fn delete_tunnel(
        &self,
        _id: Uuid,
    ) -> Result<wardnet_common::api::DeleteTunnelResponse, AppError> {
        unimplemented!("not exercised by routing-profile service tests")
    }
    async fn bring_up_internal(&self, _id: Uuid) -> Result<(), AppError> {
        unimplemented!("not exercised by routing-profile service tests")
    }
    async fn tear_down_internal(&self, _id: Uuid, _reason: &str) -> Result<(), AppError> {
        unimplemented!("not exercised by routing-profile service tests")
    }
    async fn restore_tunnels(&self) -> Result<(), AppError> {
        unimplemented!("not exercised by routing-profile service tests")
    }
    async fn collect_stats(&self) -> Result<(), AppError> {
        unimplemented!("not exercised by routing-profile service tests")
    }
    async fn run_health_check(&self) -> Result<(), AppError> {
        unimplemented!("not exercised by routing-profile service tests")
    }
    async fn probe_latencies(&self) -> Result<(), AppError> {
        unimplemented!("not exercised by routing-profile service tests")
    }
    async fn start_speed_test(
        self: Arc<Self>,
        _id: Uuid,
    ) -> Result<wardnet_common::jobs::JobDispatchedResponse, AppError> {
        unimplemented!("not exercised by routing-profile service tests")
    }
    async fn list_speed_tests(
        &self,
        _id: Uuid,
    ) -> Result<wardnet_common::speed_test::TunnelSpeedTestHistoryResponse, AppError> {
        unimplemented!("not exercised by routing-profile service tests")
    }
}

/// Build a service over an in-memory DB with one device that has a `Direct`
/// rule matching `*.netflix.com` assigned, and return the service, the
/// enforcement receiver, and the device id.
async fn setup() -> (
    Arc<RoutingProfileServiceImpl>,
    mpsc::Receiver<DomainRouteRequest>,
    Uuid,
) {
    let pool = test_pool().await;
    let device_id = Uuid::new_v4();
    insert_device(&pool, device_id, "10.0.0.5").await;
    let repo = Arc::new(SqliteRoutingProfileRepository::new(pool));
    let (tx, rx) = mpsc::channel(16);
    let svc = Arc::new(RoutingProfileServiceImpl::new(
        repo,
        Arc::new(MissingTunnelService),
        tx,
    ));

    auth_context::with_context(admin_ctx(), async {
        let profile = svc.create_profile("streaming").await.unwrap();
        svc.create_rule(profile.id, "*.netflix.com", DomainRoutingTarget::Direct, true)
            .await
            .unwrap();
        svc.set_device_profiles(device_id, &[profile.id])
            .await
            .unwrap();
        // Mutations refresh the view inline; call again to pin the contract.
        svc.refresh_view().await.unwrap();
    })
    .await;

    (svc, rx, device_id)
}

const LAN_IP: &str = "10.0.0.5";

fn v4(a: u8, b: u8, c: u8, d: u8) -> IpAddr {
    IpAddr::V4(Ipv4Addr::new(a, b, c, d))
}

#[tokio::test]
async fn note_resolution_queues_a_matching_domain() {
    let (svc, mut rx, device_id) = setup().await;
    let ip: IpAddr = LAN_IP.parse().unwrap();

    svc.note_resolution(device_id, ip, "www.netflix.com", &[v4(93, 184, 216, 34)], 300);

    let req = rx.try_recv().expect("a matching resolution must be queued");
    assert_eq!(req.device_ip, ip);
    assert_eq!(req.resolved_ips, vec![v4(93, 184, 216, 34)]);
    assert_eq!(req.target, DomainRoutingTarget::Direct);
    assert_eq!(req.ttl_secs, 300);
}

#[tokio::test]
async fn note_resolution_ignores_non_matching_name() {
    let (svc, mut rx, device_id) = setup().await;
    let ip: IpAddr = LAN_IP.parse().unwrap();

    svc.note_resolution(device_id, ip, "example.com", &[v4(1, 1, 1, 1)], 300);
    assert!(rx.try_recv().is_err(), "an unmatched name enqueues nothing");
}

#[tokio::test]
async fn note_resolution_ignores_unknown_device() {
    let (svc, mut rx, _device_id) = setup().await;
    let ip: IpAddr = LAN_IP.parse().unwrap();

    svc.note_resolution(Uuid::new_v4(), ip, "www.netflix.com", &[v4(1, 1, 1, 1)], 300);
    assert!(
        rx.try_recv().is_err(),
        "a device with no compiled context enqueues nothing"
    );
}

#[tokio::test]
async fn note_resolution_ignores_ipv6_only_answers() {
    let (svc, mut rx, device_id) = setup().await;
    let ip: IpAddr = LAN_IP.parse().unwrap();
    let v6: IpAddr = "2606:2800:220:1:248:1893:25c8:1946".parse().unwrap();

    // v1 enforcement is IPv4-only: an AAAA-only answer filters to empty and is
    // dropped before it reaches the channel.
    svc.note_resolution(device_id, ip, "www.netflix.com", &[v6], 300);
    assert!(rx.try_recv().is_err(), "an IPv6-only answer enqueues nothing");
}

#[tokio::test]
async fn create_rule_rejects_an_unknown_tunnel() {
    let (svc, _rx, _device_id) = setup().await;

    auth_context::with_context(admin_ctx(), async {
        let profile = svc.create_profile("vpn").await.unwrap();
        let err = svc
            .create_rule(
                profile.id,
                "secure.example.com",
                DomainRoutingTarget::Tunnel {
                    tunnel_id: Uuid::new_v4(),
                },
                true,
            )
            .await
            .unwrap_err();
        assert!(
            matches!(err, AppError::BadRequest(_)),
            "a rule targeting a nonexistent tunnel is a bad request, got {err:?}"
        );
    })
    .await;
}
