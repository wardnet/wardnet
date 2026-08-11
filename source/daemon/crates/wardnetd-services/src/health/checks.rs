//! Concrete [`HealthCheck`] adapters wired onto the [`HealthMonitor`] at
//! startup (issue #214).
//!
//! Each adapter is a thin shim over an already-present signal — a DB ping, or a
//! server's configured-vs-actual running state — kept deliberately cheap and
//! non-blocking. The production daemon registers all four; the mock registers
//! only the backend-independent subset (liveness + database), because its
//! DNS/DHCP servers are no-ops that never bind a socket.
//!
//! ## Why the DNS/DHCP probes compare *desired vs actual*
//!
//! `DnsServer`/`DhcpServer::is_running()` reflects the admin **enable-state**,
//! not health: both runners start their server only when the corresponding
//! config flag is set, and stop it when toggled off. A naive `is_running()`
//! probe would report DOWN for a *legitimately disabled* service, the soft
//! watchdog would withhold its ping, and systemd would restart-loop a perfectly
//! healthy daemon (e.g. one whose operator keeps DHCP on their router). So the
//! probe reads the configured `enabled` flag (under an admin context, exactly
//! like the runners do) and reports DOWN **only** when the service is
//! configured-enabled yet not running — i.e. it actually crashed.

use std::sync::Arc;

use async_trait::async_trait;
use wardnet_common::auth::AuthContext;

use super::{CheckOutcome, HealthCheck};
use crate::auth_context;
use crate::dhcp::DhcpService;
use crate::dhcp::server::DhcpServer;
use crate::dns::DnsService;
use crate::dns::server::DnsServer;
use crate::private_dns::{DotServer, PrivateDnsService, cert_ready};
use crate::tls::TlsService;
use wardnetd_data::repository::MaintenanceRepository;

/// Admin context used by the desired-vs-actual probes to read config through
/// the auth-gated service layer — the same nil-admin pattern the DNS/DHCP/
/// heartbeat runners use for their own background reads.
fn admin_ctx() -> AuthContext {
    AuthContext::system()
}

/// Liveness probe — always `Up`.
///
/// Its value is indirect: a fresh `refreshed_at` on a snapshot that *contains*
/// this component proves the refresh loop is actually scheduling and the
/// monitor's machinery runs end-to-end. Pairs with the soft watchdog's
/// staleness check.
pub struct LivenessHealthCheck;

#[async_trait]
impl HealthCheck for LivenessHealthCheck {
    fn name(&self) -> &'static str {
        "liveness"
    }

    async fn check(&self) -> CheckOutcome {
        CheckOutcome::Up
    }
}

/// Database connectivity probe — issues a cheap `SELECT 1` through the shared
/// pool so a wedged or unreachable `SQLite` layer surfaces as DOWN.
pub struct DbHealthCheck {
    repo: Arc<dyn MaintenanceRepository>,
}

impl DbHealthCheck {
    #[must_use]
    pub fn new(repo: Arc<dyn MaintenanceRepository>) -> Self {
        Self { repo }
    }
}

#[async_trait]
impl HealthCheck for DbHealthCheck {
    fn name(&self) -> &'static str {
        "database"
    }

    async fn check(&self) -> CheckOutcome {
        match self.repo.ping().await {
            Ok(()) => CheckOutcome::Up,
            Err(e) => {
                // Log the real error to the journal; keep the unauthenticated
                // `/health` `detail` static so raw sqlx internals (paths, DB
                // state) aren't disclosed to anonymous callers.
                tracing::warn!(error = %e, "database health probe failed: {e}");
                CheckOutcome::down("database unreachable")
            }
        }
    }
}

/// DNS server health probe — DOWN only when DNS is configured-enabled but its
/// UDP server is not actually running (a crash). A toggled-off DNS server is
/// UP, not a failure. See the module docs.
pub struct DnsServerHealthCheck {
    service: Arc<dyn DnsService>,
    server: Arc<dyn DnsServer>,
}

impl DnsServerHealthCheck {
    #[must_use]
    pub fn new(service: Arc<dyn DnsService>, server: Arc<dyn DnsServer>) -> Self {
        Self { service, server }
    }
}

#[async_trait]
impl HealthCheck for DnsServerHealthCheck {
    fn name(&self) -> &'static str {
        "dns"
    }

    async fn check(&self) -> CheckOutcome {
        match auth_context::with_context(admin_ctx(), self.service.get_dns_config()).await {
            Ok(config) if config.enabled && !self.server.is_running() => {
                CheckOutcome::down("dns enabled but server not running")
            }
            Ok(_) => CheckOutcome::Up,
            Err(e) => {
                tracing::warn!(error = %e, "dns health probe: config read failed: {e}");
                CheckOutcome::down("dns status unavailable")
            }
        }
    }
}

/// `DoT` (`:853`) server health probe — DOWN only when Private DNS is
/// configured-enabled, an issued certificate is live (the runner's two
/// preconditions), and the listener is still not running (a crash). A
/// disabled feature — or one whose certificate hasn't been issued yet, a
/// normal state right after enrollment — is UP, not a failure. See the
/// module docs.
pub struct DotServerHealthCheck {
    service: Arc<dyn PrivateDnsService>,
    tls: Arc<dyn TlsService>,
    server: Arc<dyn DotServer>,
}

impl DotServerHealthCheck {
    #[must_use]
    pub fn new(
        service: Arc<dyn PrivateDnsService>,
        tls: Arc<dyn TlsService>,
        server: Arc<dyn DotServer>,
    ) -> Self {
        Self {
            service,
            tls,
            server,
        }
    }
}

#[async_trait]
impl HealthCheck for DotServerHealthCheck {
    fn name(&self) -> &'static str {
        "dot"
    }

    async fn check(&self) -> CheckOutcome {
        let enabled = match auth_context::with_context(admin_ctx(), self.service.is_enabled()).await
        {
            Ok(enabled) => enabled,
            Err(e) => {
                tracing::warn!(error = %e, "dot health probe: status read failed: {e}");
                return CheckOutcome::down("dot status unavailable");
            }
        };
        if !enabled {
            return CheckOutcome::Up;
        }
        match auth_context::with_context(admin_ctx(), self.tls.status()).await {
            Ok(tls_status) if cert_ready(&tls_status) && !self.server.is_running() => {
                CheckOutcome::down("private dns enabled but dot server not running")
            }
            Ok(_) => CheckOutcome::Up,
            Err(e) => {
                tracing::warn!(error = %e, "dot health probe: tls status read failed: {e}");
                CheckOutcome::down("dot status unavailable")
            }
        }
    }
}

/// DHCP server health probe — DOWN only when DHCP is configured-enabled but its
/// UDP server is not actually running (a crash). A toggled-off DHCP server
/// (common — operators often keep DHCP on their router) is UP. See the module
/// docs.
pub struct DhcpServerHealthCheck {
    service: Arc<dyn DhcpService>,
    server: Arc<dyn DhcpServer>,
}

impl DhcpServerHealthCheck {
    #[must_use]
    pub fn new(service: Arc<dyn DhcpService>, server: Arc<dyn DhcpServer>) -> Self {
        Self { service, server }
    }
}

#[async_trait]
impl HealthCheck for DhcpServerHealthCheck {
    fn name(&self) -> &'static str {
        "dhcp"
    }

    async fn check(&self) -> CheckOutcome {
        match auth_context::with_context(admin_ctx(), self.service.get_dhcp_config()).await {
            Ok(config) if config.enabled && !self.server.is_running() => {
                CheckOutcome::down("dhcp enabled but server not running")
            }
            Ok(_) => CheckOutcome::Up,
            Err(e) => {
                tracing::warn!(error = %e, "dhcp health probe: config read failed: {e}");
                CheckOutcome::down("dhcp status unavailable")
            }
        }
    }
}
