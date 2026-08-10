//! Tests for [`DomainRouteRunner`].
//!
//! The runner is thin glue: at startup it bootstraps the compiled view
//! ([`RoutingProfileService::refresh_view`]), then drains the enforcement
//! channel (calling [`RoutingService::route_resolved_domain`] per request) and
//! sweeps expired leases on a timer ([`RoutingService::gc_domain_routes`]).
//! Both service traits are mocked so the loop can be driven without a kernel or
//! a database — mirrors `private_dns/tests/runner.rs`.

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use arc_swap::ArcSwap;
use async_trait::async_trait;
use uuid::Uuid;
use wardnet_common::dns::UpstreamId;
use wardnet_common::routing::RoutingTarget;
use wardnet_common::routing_profile::{DomainRoutingRule, DomainRoutingTarget, RoutingProfile};

use crate::error::AppError;
use crate::routing::RoutingService;
use crate::routing_profile::runner::DomainRouteRunner;
use crate::routing_profile::service::{DomainRouteRequest, RoutingProfileService};

// -- Mock RoutingProfileService (only `refresh_view` is exercised) ---------

#[derive(Default)]
struct MockProfiles {
    refreshes: AtomicU32,
}

#[async_trait]
impl RoutingProfileService for MockProfiles {
    async fn refresh_view(&self) -> Result<(), AppError> {
        self.refreshes.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    fn note_resolution(
        &self,
        _device_id: Uuid,
        _device_ip: IpAddr,
        _name: &str,
        _answer_ips: &[IpAddr],
        _ttl_secs: u32,
    ) {
    }

    async fn list_profiles(&self) -> Result<Vec<RoutingProfile>, AppError> {
        unimplemented!("not exercised by runner tests")
    }
    async fn get_profile(&self, _id: Uuid) -> Result<RoutingProfile, AppError> {
        unimplemented!("not exercised by runner tests")
    }
    async fn create_profile(&self, _name: &str) -> Result<RoutingProfile, AppError> {
        unimplemented!("not exercised by runner tests")
    }
    async fn rename_profile(&self, _id: Uuid, _name: &str) -> Result<RoutingProfile, AppError> {
        unimplemented!("not exercised by runner tests")
    }
    async fn delete_profile(&self, _id: Uuid) -> Result<(), AppError> {
        unimplemented!("not exercised by runner tests")
    }
    async fn list_rules(&self, _profile_id: Uuid) -> Result<Vec<DomainRoutingRule>, AppError> {
        unimplemented!("not exercised by runner tests")
    }
    async fn create_rule(
        &self,
        _profile_id: Uuid,
        _pattern: &str,
        _target: DomainRoutingTarget,
        _enabled: bool,
    ) -> Result<DomainRoutingRule, AppError> {
        unimplemented!("not exercised by runner tests")
    }
    async fn update_rule(
        &self,
        _id: Uuid,
        _pattern: Option<String>,
        _target: Option<DomainRoutingTarget>,
        _enabled: Option<bool>,
    ) -> Result<DomainRoutingRule, AppError> {
        unimplemented!("not exercised by runner tests")
    }
    async fn delete_rule(&self, _id: Uuid) -> Result<(), AppError> {
        unimplemented!("not exercised by runner tests")
    }
    async fn get_device_profiles(&self, _device_id: Uuid) -> Result<Vec<Uuid>, AppError> {
        unimplemented!("not exercised by runner tests")
    }
    async fn set_device_profiles(
        &self,
        _device_id: Uuid,
        _profile_ids: &[Uuid],
    ) -> Result<(), AppError> {
        unimplemented!("not exercised by runner tests")
    }
    async fn list_profile_devices(&self, _profile_id: Uuid) -> Result<Vec<Uuid>, AppError> {
        unimplemented!("not exercised by runner tests")
    }
}

/// A `RoutingProfileService` whose `refresh_view` always errors — models a
/// transient DB read failure so the runner's bootstrap error branch runs.
struct ErroringProfiles;

#[async_trait]
impl RoutingProfileService for ErroringProfiles {
    async fn refresh_view(&self) -> Result<(), AppError> {
        Err(AppError::Internal(anyhow::anyhow!("view bootstrap failed")))
    }
    fn note_resolution(
        &self,
        _device_id: Uuid,
        _device_ip: IpAddr,
        _name: &str,
        _answer_ips: &[IpAddr],
        _ttl_secs: u32,
    ) {
    }
    async fn list_profiles(&self) -> Result<Vec<RoutingProfile>, AppError> {
        unimplemented!()
    }
    async fn get_profile(&self, _id: Uuid) -> Result<RoutingProfile, AppError> {
        unimplemented!()
    }
    async fn create_profile(&self, _name: &str) -> Result<RoutingProfile, AppError> {
        unimplemented!()
    }
    async fn rename_profile(&self, _id: Uuid, _name: &str) -> Result<RoutingProfile, AppError> {
        unimplemented!()
    }
    async fn delete_profile(&self, _id: Uuid) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn list_rules(&self, _profile_id: Uuid) -> Result<Vec<DomainRoutingRule>, AppError> {
        unimplemented!()
    }
    async fn create_rule(
        &self,
        _profile_id: Uuid,
        _pattern: &str,
        _target: DomainRoutingTarget,
        _enabled: bool,
    ) -> Result<DomainRoutingRule, AppError> {
        unimplemented!()
    }
    async fn update_rule(
        &self,
        _id: Uuid,
        _pattern: Option<String>,
        _target: Option<DomainRoutingTarget>,
        _enabled: Option<bool>,
    ) -> Result<DomainRoutingRule, AppError> {
        unimplemented!()
    }
    async fn delete_rule(&self, _id: Uuid) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn get_device_profiles(&self, _device_id: Uuid) -> Result<Vec<Uuid>, AppError> {
        unimplemented!()
    }
    async fn set_device_profiles(
        &self,
        _device_id: Uuid,
        _profile_ids: &[Uuid],
    ) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn list_profile_devices(&self, _profile_id: Uuid) -> Result<Vec<Uuid>, AppError> {
        unimplemented!()
    }
}

// -- Mock RoutingService (records the two domain-route calls) --------------

#[derive(Default)]
struct MockRouting {
    /// `(device_ip, ttl)` per `route_resolved_domain` call.
    enforced: Mutex<Vec<(String, u32)>>,
    gc_sweeps: AtomicU32,
}

#[async_trait]
impl RoutingService for MockRouting {
    async fn route_resolved_domain(
        &self,
        device_ip: &str,
        _resolved_ips: &[IpAddr],
        _target: &DomainRoutingTarget,
        ttl_secs: u32,
    ) -> Result<(), AppError> {
        self.enforced
            .lock()
            .unwrap()
            .push((device_ip.to_owned(), ttl_secs));
        Ok(())
    }
    async fn gc_domain_routes(&self) -> Result<(), AppError> {
        self.gc_sweeps.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    async fn apply_rule(
        &self,
        _device_id: Uuid,
        _device_ip: &str,
        _target: &RoutingTarget,
    ) -> Result<(), AppError> {
        unimplemented!("not exercised by runner tests")
    }
    async fn remove_device_routes(
        &self,
        _device_id: Uuid,
        _device_ip: &str,
    ) -> Result<(), AppError> {
        unimplemented!("not exercised by runner tests")
    }
    async fn handle_ip_change(
        &self,
        _device_id: Uuid,
        _old_ip: &str,
        _new_ip: &str,
    ) -> Result<(), AppError> {
        unimplemented!("not exercised by runner tests")
    }
    async fn handle_tunnel_down(&self, _tunnel_id: Uuid) -> Result<(), AppError> {
        unimplemented!("not exercised by runner tests")
    }
    async fn handle_tunnel_up(&self, _tunnel_id: Uuid) -> Result<(), AppError> {
        unimplemented!("not exercised by runner tests")
    }
    async fn reconcile(&self) -> Result<(), AppError> {
        unimplemented!("not exercised by runner tests")
    }
    async fn handle_route_table_lost(&self, _table: u32) -> Result<(), AppError> {
        unimplemented!("not exercised by runner tests")
    }
    async fn devices_using_tunnel(&self, _tunnel_id: Uuid) -> Result<Vec<Uuid>, AppError> {
        unimplemented!("not exercised by runner tests")
    }
    async fn apply_rule_for_device(
        &self,
        _device_id: Uuid,
        _target: &RoutingTarget,
    ) -> Result<(), AppError> {
        unimplemented!("not exercised by runner tests")
    }
    async fn apply_rule_for_discovered_device(
        &self,
        _device_id: Uuid,
        _ip: &str,
    ) -> Result<(), AppError> {
        unimplemented!("not exercised by runner tests")
    }
    async fn set_switchback_targets(
        &self,
        _device_id: Uuid,
        _device_ip: String,
        _target_cidrs: Vec<String>,
    ) -> Result<(), AppError> {
        unimplemented!("not exercised by runner tests")
    }
    async fn set_default_policy(&self, _policy: &str) -> Result<(), AppError> {
        unimplemented!("not exercised by runner tests")
    }
    async fn default_policy(&self) -> Result<String, AppError> {
        unimplemented!("not exercised by runner tests")
    }
    async fn handle_default_policy_changed(&self) -> Result<(), AppError> {
        unimplemented!("not exercised by runner tests")
    }
    fn dns_upstream_snapshot(&self) -> Arc<ArcSwap<HashMap<IpAddr, UpstreamId>>> {
        unimplemented!("not exercised by runner tests")
    }
    async fn rebuild_dns_upstream_snapshot(&self) -> Result<(), AppError> {
        unimplemented!("not exercised by runner tests")
    }
    fn dns_device_upstream_snapshot(&self) -> Arc<ArcSwap<HashMap<Uuid, UpstreamId>>> {
        unimplemented!("not exercised by runner tests")
    }
    async fn rebuild_dns_device_upstream_snapshot(&self) -> Result<(), AppError> {
        unimplemented!("not exercised by runner tests")
    }
}

/// Poll until `predicate` holds or ~2s elapse.
async fn wait_for(predicate: impl Fn() -> bool) {
    for _ in 0..200 {
        if predicate() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(predicate(), "condition not reached within 2s");
}

fn request(device_ip: &str, ttl: u32) -> DomainRouteRequest {
    DomainRouteRequest {
        device_ip: device_ip.parse().unwrap(),
        resolved_ips: vec![IpAddr::V4(Ipv4Addr::new(93, 184, 216, 34))],
        target: DomainRoutingTarget::Direct,
        ttl_secs: ttl,
    }
}

#[tokio::test]
async fn bootstraps_view_gc_sweeps_and_enforces_queued_requests() {
    let profiles = Arc::new(MockProfiles::default());
    let routing = Arc::new(MockRouting::default());
    let (tx, rx) = tokio::sync::mpsc::channel(16);

    let runner = DomainRouteRunner::start(
        profiles.clone(),
        routing.clone(),
        rx,
        &tracing::Span::none(),
    );

    // Startup bootstraps the compiled view, and the GC interval's first tick
    // fires immediately so a sweep runs without waiting out the 30s interval.
    wait_for(|| {
        profiles.refreshes.load(Ordering::SeqCst) >= 1
            && routing.gc_sweeps.load(Ordering::SeqCst) >= 1
    })
    .await;

    // A queued resolution is drained and enforced with its device IP + TTL.
    tx.send(request("10.0.0.5", 300)).await.unwrap();
    wait_for(|| !routing.enforced.lock().unwrap().is_empty()).await;
    assert_eq!(
        routing.enforced.lock().unwrap().as_slice(),
        &[("10.0.0.5".to_owned(), 300)]
    );

    runner.shutdown().await;
}

#[tokio::test]
async fn closed_channel_ends_the_loop() {
    let profiles = Arc::new(MockProfiles::default());
    let routing = Arc::new(MockRouting::default());
    let (tx, rx) = tokio::sync::mpsc::channel(4);

    let runner = DomainRouteRunner::start(
        profiles.clone(),
        routing.clone(),
        rx,
        &tracing::Span::none(),
    );
    wait_for(|| profiles.refreshes.load(Ordering::SeqCst) >= 1).await;

    // Dropping every sender closes the channel; the loop exits on `recv() ->
    // None`. shutdown() then joins cleanly even though the loop already ended.
    drop(tx);
    runner.shutdown().await;
}

#[tokio::test]
async fn bootstrap_error_does_not_abort_the_runner() {
    // A failed view bootstrap is logged and swallowed — the runner still
    // services the enforcement channel afterwards.
    let routing = Arc::new(MockRouting::default());
    let (tx, rx) = tokio::sync::mpsc::channel(4);

    let runner = DomainRouteRunner::start(
        Arc::new(ErroringProfiles),
        routing.clone(),
        rx,
        &tracing::Span::none(),
    );

    tx.send(request("10.0.0.9", 60)).await.unwrap();
    wait_for(|| !routing.enforced.lock().unwrap().is_empty()).await;

    runner.shutdown().await;
}
