//! Tests for the desired-vs-actual DNS/DHCP health probes (issue #214).
//!
//! The critical invariant: a *deliberately disabled* service must report UP, so
//! the soft watchdog never restart-loops a healthy daemon. DOWN is reported
//! only when the service is configured-enabled yet its server isn't running.

use std::net::Ipv4Addr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;
use wardnet_common::dhcp::DhcpConfig;
use wardnet_common::dns::DnsConfig;

use crate::dhcp::server::DhcpServer;
use crate::dhcp::service::DhcpService;
use crate::dns::server::DnsServer;
use crate::dns::service::DnsService;
use crate::error::AppError;
use crate::health::checks::{DhcpServerHealthCheck, DnsServerHealthCheck};
use crate::health::{CheckOutcome, HealthCheck};

// ── DNS mocks ─────────────────────────────────────────────────────────────

/// Mock `DnsService` whose `get_dns_config` returns a config with the given
/// `enabled` flag, or an error when `fail` is set.
struct MockDnsService {
    enabled: bool,
    fail: bool,
}

#[async_trait]
impl DnsService for MockDnsService {
    async fn get_dns_config(&self) -> Result<DnsConfig, AppError> {
        if self.fail {
            return Err(AppError::Internal(anyhow::anyhow!(
                "simulated config error"
            )));
        }
        Ok(DnsConfig {
            enabled: self.enabled,
            ..Default::default()
        })
    }

    async fn get_config(&self) -> Result<wardnet_common::api::DnsConfigResponse, AppError> {
        unimplemented!()
    }
    async fn update_config(
        &self,
        _req: wardnet_common::api::UpdateDnsConfigRequest,
    ) -> Result<wardnet_common::api::DnsConfigResponse, AppError> {
        unimplemented!()
    }
    async fn toggle(
        &self,
        _req: wardnet_common::api::ToggleDnsRequest,
    ) -> Result<wardnet_common::api::DnsConfigResponse, AppError> {
        unimplemented!()
    }
    async fn status(&self) -> Result<wardnet_common::api::DnsStatusResponse, AppError> {
        unimplemented!()
    }
    async fn flush_cache(&self) -> Result<wardnet_common::api::DnsCacheFlushResponse, AppError> {
        unimplemented!()
    }
    async fn list_query_log(
        &self,
        _params: wardnet_common::api::ListQueryLogParams,
    ) -> Result<wardnet_common::api::ListQueryLogResponse, AppError> {
        unimplemented!()
    }
    fn subscribe_query_stream(
        &self,
    ) -> Result<tokio::sync::broadcast::Receiver<wardnet_common::api::QueryLogEvent>, AppError>
    {
        unimplemented!()
    }
    async fn flush_query_log(&self) -> Result<u64, AppError> {
        unimplemented!()
    }
    async fn insert_query_log_batch(
        &self,
        _entries: &[wardnetd_data::repository::QueryLogRow],
    ) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn cleanup_query_log(&self, _retention_days: u32) -> Result<u64, AppError> {
        unimplemented!()
    }
}

/// Mock `DnsServer` whose `is_running` returns a fixed flag.
struct MockDnsServer {
    running: AtomicBool,
}

#[async_trait]
impl DnsServer for MockDnsServer {
    async fn start(&self) -> anyhow::Result<()> {
        Ok(())
    }
    async fn stop(&self) -> anyhow::Result<()> {
        Ok(())
    }
    fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }
    async fn flush_cache(&self) -> u64 {
        0
    }
    async fn cache_size(&self) -> u64 {
        0
    }
    async fn cache_hit_rate(&self) -> f64 {
        0.0
    }
    async fn update_config(&self, _config: DnsConfig) {}
    async fn update_authoritative_view(&self, _view: crate::dns::authoritative::AuthoritativeView) {
    }
    async fn invalidate_subtree(&self, _domain: &str) {}
}

fn dns_check(enabled: bool, running: bool, fail: bool) -> DnsServerHealthCheck {
    DnsServerHealthCheck::new(
        Arc::new(MockDnsService { enabled, fail }),
        Arc::new(MockDnsServer {
            running: AtomicBool::new(running),
        }),
    )
}

#[tokio::test]
async fn dns_disabled_reports_up_even_when_not_running() {
    // The whole point: a toggled-off DNS server must NOT be DOWN.
    let outcome = dns_check(false, false, false).check().await;
    assert_eq!(outcome, CheckOutcome::Up);
}

#[tokio::test]
async fn dns_enabled_and_running_reports_up() {
    let outcome = dns_check(true, true, false).check().await;
    assert_eq!(outcome, CheckOutcome::Up);
}

#[tokio::test]
async fn dns_enabled_but_not_running_reports_down() {
    let outcome = dns_check(true, false, false).check().await;
    assert_eq!(
        outcome,
        CheckOutcome::down("dns enabled but server not running")
    );
}

#[tokio::test]
async fn dns_config_read_error_reports_down_without_leaking_detail() {
    let outcome = dns_check(true, true, true).check().await;
    // Static detail — the raw error goes to the log, not the public body.
    assert_eq!(outcome, CheckOutcome::down("dns status unavailable"));
}

// ── DHCP mocks ────────────────────────────────────────────────────────────

/// Mock `DhcpService` whose `get_dhcp_config` returns a config with the given
/// `enabled` flag, or an error when `fail` is set.
struct MockDhcpService {
    enabled: bool,
    fail: bool,
}

#[async_trait]
impl DhcpService for MockDhcpService {
    async fn get_dhcp_config(&self) -> Result<DhcpConfig, AppError> {
        if self.fail {
            return Err(AppError::Internal(anyhow::anyhow!(
                "simulated config error"
            )));
        }
        Ok(DhcpConfig {
            enabled: self.enabled,
            gateway_ip: Ipv4Addr::new(192, 168, 1, 1),
            pool_start: Ipv4Addr::new(192, 168, 1, 100),
            pool_end: Ipv4Addr::new(192, 168, 1, 200),
            subnet_mask: Ipv4Addr::new(255, 255, 255, 0),
            upstream_dns: vec![Ipv4Addr::new(1, 1, 1, 1)],
            lease_duration_secs: 86_400,
            router_ip: Some(Ipv4Addr::new(192, 168, 1, 1)),
        })
    }

    async fn scope_for_mac(&self, _mac: &str) -> Result<wardnet_common::dhcp::DhcpScope, AppError> {
        unimplemented!()
    }

    async fn get_config(&self) -> Result<wardnet_common::api::DhcpConfigResponse, AppError> {
        unimplemented!()
    }
    async fn update_config(
        &self,
        _r: wardnet_common::api::UpdateDhcpConfigRequest,
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
        _r: wardnet_common::api::ToggleDhcpRequest,
    ) -> Result<wardnet_common::api::DhcpConfigResponse, AppError> {
        unimplemented!()
    }
    async fn list_leases(&self) -> Result<wardnet_common::api::ListDhcpLeasesResponse, AppError> {
        unimplemented!()
    }
    async fn revoke_lease(
        &self,
        _id: uuid::Uuid,
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
        _r: wardnet_common::api::CreateDhcpReservationRequest,
    ) -> Result<wardnet_common::api::CreateDhcpReservationResponse, AppError> {
        unimplemented!()
    }
    async fn delete_reservation(
        &self,
        _id: uuid::Uuid,
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
    ) -> Result<wardnet_common::dhcp::DhcpLease, AppError> {
        unimplemented!()
    }
    async fn renew_lease(
        &self,
        _mac: &str,
        _hostname: Option<&str>,
    ) -> Result<wardnet_common::dhcp::DhcpLease, AppError> {
        unimplemented!()
    }
    async fn release_lease(&self, _mac: &str) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn cleanup_expired(&self) -> Result<u64, AppError> {
        unimplemented!()
    }
}

/// Mock `DhcpServer` whose `is_running` returns a fixed flag.
struct MockDhcpServer {
    running: AtomicBool,
}

#[async_trait]
impl DhcpServer for MockDhcpServer {
    async fn start(&self) -> Result<(), AppError> {
        Ok(())
    }
    async fn stop(&self) -> Result<(), AppError> {
        Ok(())
    }
    fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }
}

fn dhcp_check(enabled: bool, running: bool, fail: bool) -> DhcpServerHealthCheck {
    DhcpServerHealthCheck::new(
        Arc::new(MockDhcpService { enabled, fail }),
        Arc::new(MockDhcpServer {
            running: AtomicBool::new(running),
        }),
    )
}

#[tokio::test]
async fn dhcp_disabled_reports_up_even_when_not_running() {
    // Operators commonly run DHCP on their router; a disabled server is healthy.
    let outcome = dhcp_check(false, false, false).check().await;
    assert_eq!(outcome, CheckOutcome::Up);
}

#[tokio::test]
async fn dhcp_enabled_and_running_reports_up() {
    let outcome = dhcp_check(true, true, false).check().await;
    assert_eq!(outcome, CheckOutcome::Up);
}

#[tokio::test]
async fn dhcp_enabled_but_not_running_reports_down() {
    let outcome = dhcp_check(true, false, false).check().await;
    assert_eq!(
        outcome,
        CheckOutcome::down("dhcp enabled but server not running")
    );
}

#[tokio::test]
async fn dhcp_config_read_error_reports_down_without_leaking_detail() {
    let outcome = dhcp_check(true, true, true).check().await;
    assert_eq!(outcome, CheckOutcome::down("dhcp status unavailable"));
}

// ── Liveness ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn liveness_always_reports_up() {
    let check = crate::health::checks::LivenessHealthCheck;
    assert_eq!(check.name(), "liveness");
    assert_eq!(check.check().await, CheckOutcome::Up);
}

// ── Database ────────────────────────────────────────────────────────────────

/// Mock `MaintenanceRepository` whose `ping` succeeds or fails on demand. Only
/// `ping` is exercised by the health check; the rest is unreachable.
struct MockMaintenanceRepo {
    fail: bool,
}

#[async_trait]
impl wardnetd_data::repository::MaintenanceRepository for MockMaintenanceRepo {
    async fn incremental_vacuum(&self) -> anyhow::Result<u64> {
        unimplemented!()
    }
    async fn wal_checkpoint_truncate(
        &self,
    ) -> anyhow::Result<wardnetd_data::repository::WalCheckpointOutcome> {
        unimplemented!()
    }
    async fn optimize(&self) -> anyhow::Result<()> {
        unimplemented!()
    }
    async fn ping(&self) -> anyhow::Result<()> {
        if self.fail {
            Err(anyhow::anyhow!("simulated db failure"))
        } else {
            Ok(())
        }
    }
}

fn db_check(fail: bool) -> crate::health::checks::DbHealthCheck {
    crate::health::checks::DbHealthCheck::new(Arc::new(MockMaintenanceRepo { fail }))
}

#[tokio::test]
async fn database_ping_ok_reports_up() {
    let check = db_check(false);
    assert_eq!(check.name(), "database");
    assert_eq!(check.check().await, CheckOutcome::Up);
}

#[tokio::test]
async fn database_ping_error_reports_down_with_static_detail() {
    // The journal gets the real error; the unauthenticated `/health` detail
    // stays static so sqlx internals aren't disclosed to anonymous callers.
    let outcome = db_check(true).check().await;
    assert_eq!(outcome, CheckOutcome::down("database unreachable"));
}
