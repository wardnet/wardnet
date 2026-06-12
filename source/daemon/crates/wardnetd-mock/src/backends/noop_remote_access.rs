//! Stateful in-memory mock of the remote-access (DDNS + TLS) services.
//!
//! Lets web-ui devs exercise the full HTTPS/DDNS flow — bridge / BYOD-Cloudflare
//! registration, the `issuing → issued` certificate progress, the external
//! resolution check, and teardown — **entirely offline**, with no real bridge,
//! Cloudflare, Let's Encrypt, or DNS. Despite the `noop_` prefix it is stateful;
//! the prefix marks that it performs **no** kernel or network I/O (and keeps it
//! out of coverage, like the sibling no-op backends).
//!
//! ## Simulated behaviour
//! - `register_with_bridge` / `configure_cloudflare` record a fake FQDN + public
//!   IP and start a timer.
//! - For [`ISSUE_DELAY`] after registration the TLS phase reports `issuing` and
//!   the resolution check `pending`; after it, `issued` (a 90-day cert) and
//!   `match` — so the frontend's issuing spinner, poll loop, live state, and the
//!   resolution badge are all reachable without waiting on real propagation.
//! - A name/domain containing [`FAILURE_TRIGGER`] simulates an issuance failure
//!   (phase `failed`), and [`TAKEN_NAMES`] read as unavailable — exercising the
//!   error/edge UI.
//! - `teardown` resets to the unconfigured state.
//!
//! Both mock services share one [`MockRemoteAccessState`] so the DDNS and TLS
//! sides agree on what is configured and how far issuance has progressed.

use std::net::Ipv4Addr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use chrono::{Duration as ChronoDuration, Utc};
use wardnet_common::api::{
    DdnsResolutionCheckResponse, DdnsResolutionVerdict, TlsProvisioningPhase, TlsStatusResponse,
};
use wardnetd_services::ddns::{DdnsRegistration, DdnsService, DdnsStatus};
use wardnetd_services::error::AppError;
use wardnetd_services::tls::{TlsService, TlsStatus};

/// How long simulated issuance "takes" — long enough for the frontend's issuing
/// spinner + 3-second poll loop to be visible, short enough not to annoy.
const ISSUE_DELAY: Duration = Duration::from_secs(5);

/// Simulated certificate validity once "issued".
const CERT_VALIDITY_DAYS: i64 = 90;

/// Bridge names the mock reports as already taken (exercises the availability
/// check's "taken — try another" state).
const TAKEN_NAMES: &[&str] = &["taken", "admin", "wardnet", "test", "root"];

/// A substring in a name/domain that forces a simulated issuance failure.
const FAILURE_TRIGGER: &str = "fail";

/// A fake but plausible public IP the mock "publishes".
const FAKE_PUBLIC_IP: &str = "203.0.113.45";

struct Sim {
    provider: &'static str,
    fqdn: String,
    public_ip: String,
    started: Instant,
    force_failure: bool,
}

/// Shared simulated remote-access state, held by both mock services.
#[derive(Default)]
pub struct MockRemoteAccessState {
    sim: Mutex<Option<Sim>>,
}

impl MockRemoteAccessState {
    #[must_use]
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Whether the (configured) simulation has finished "issuing".
    fn issued(sim: &Sim) -> bool {
        !sim.force_failure && sim.started.elapsed() >= ISSUE_DELAY
    }

    /// Derive the coarse TLS provisioning status from the current simulation.
    fn tls_status_response(&self) -> TlsStatusResponse {
        let guard = self.sim.lock().unwrap();
        match &*guard {
            None => TlsStatusResponse {
                phase: TlsProvisioningPhase::Idle,
                domain: None,
                not_after: None,
                error: None,
            },
            Some(sim) if sim.force_failure => TlsStatusResponse {
                phase: TlsProvisioningPhase::Failed,
                domain: Some(sim.fqdn.clone()),
                not_after: None,
                error: Some("simulated certificate issuance failure (mock daemon)".to_owned()),
            },
            Some(sim) if Self::issued(sim) => TlsStatusResponse {
                phase: TlsProvisioningPhase::Issued,
                domain: Some(sim.fqdn.clone()),
                not_after: Some(
                    (Utc::now() + ChronoDuration::days(CERT_VALIDITY_DAYS)).to_rfc3339(),
                ),
                error: None,
            },
            Some(sim) => TlsStatusResponse {
                phase: TlsProvisioningPhase::Issuing,
                domain: Some(sim.fqdn.clone()),
                not_after: None,
                error: None,
            },
        }
    }

    /// Derive the [`TlsStatus`] enum from the current simulation.
    fn tls_status(&self) -> TlsStatus {
        let guard = self.sim.lock().unwrap();
        match &*guard {
            None => TlsStatus::NotConfigured,
            Some(sim) if Self::issued(sim) => TlsStatus::Issued {
                domain: sim.fqdn.clone(),
                not_after: Utc::now() + ChronoDuration::days(CERT_VALIDITY_DAYS),
            },
            Some(sim) => TlsStatus::Pending {
                domain: sim.fqdn.clone(),
            },
        }
    }

    fn clear(&self) {
        *self.sim.lock().unwrap() = None;
    }
}

/// Mock [`DdnsService`] — see [module docs](self).
pub struct MockDdnsService {
    state: Arc<MockRemoteAccessState>,
}

impl MockDdnsService {
    #[must_use]
    pub fn new(state: Arc<MockRemoteAccessState>) -> Self {
        Self { state }
    }
}

#[async_trait]
impl DdnsService for MockDdnsService {
    async fn register_with_bridge(&self, name: String) -> Result<DdnsRegistration, AppError> {
        let fqdn = format!("{name}.my.wardnet.services");
        *self.state.sim.lock().unwrap() = Some(Sim {
            provider: "bridge",
            fqdn: fqdn.clone(),
            public_ip: FAKE_PUBLIC_IP.to_owned(),
            started: Instant::now(),
            force_failure: name.contains(FAILURE_TRIGGER),
        });
        Ok(DdnsRegistration {
            subdomain: fqdn,
            region: "us".to_owned(),
        })
    }

    async fn check_name_available(&self, name: String) -> Result<bool, AppError> {
        Ok(!TAKEN_NAMES.contains(&name.as_str()))
    }

    async fn configure_cloudflare(
        &self,
        _token: String,
        domain: String,
    ) -> Result<DdnsRegistration, AppError> {
        *self.state.sim.lock().unwrap() = Some(Sim {
            provider: "cloudflare",
            fqdn: domain.clone(),
            public_ip: FAKE_PUBLIC_IP.to_owned(),
            started: Instant::now(),
            force_failure: domain.contains(FAILURE_TRIGGER),
        });
        Ok(DdnsRegistration {
            subdomain: domain,
            region: "cloudflare".to_owned(),
        })
    }

    async fn refresh_public_ip(&self) -> Result<Option<Ipv4Addr>, AppError> {
        let guard = self.state.sim.lock().unwrap();
        Ok(guard.as_ref().and_then(|s| s.public_ip.parse().ok()))
    }

    async fn status(&self) -> Result<DdnsStatus, AppError> {
        let guard = self.state.sim.lock().unwrap();
        Ok(match &*guard {
            Some(s) => DdnsStatus {
                provider: Some(s.provider.to_owned()),
                fqdn: Some(s.fqdn.clone()),
                last_public_ip: Some(s.public_ip.clone()),
            },
            None => DdnsStatus {
                provider: None,
                fqdn: None,
                last_public_ip: None,
            },
        })
    }

    async fn teardown(&self) -> Result<(), AppError> {
        self.state.clear();
        Ok(())
    }

    async fn resolution_check(&self) -> Result<DdnsResolutionCheckResponse, AppError> {
        let guard = self.state.sim.lock().unwrap();
        Ok(match &*guard {
            None => DdnsResolutionCheckResponse {
                verdict: DdnsResolutionVerdict::NotConfigured,
                fqdn: None,
                expected_ip: None,
                resolved_ips: Vec::new(),
            },
            Some(s) if MockRemoteAccessState::issued(s) => DdnsResolutionCheckResponse {
                verdict: DdnsResolutionVerdict::Match,
                fqdn: Some(s.fqdn.clone()),
                expected_ip: Some(s.public_ip.clone()),
                resolved_ips: vec![s.public_ip.clone()],
            },
            Some(s) => DdnsResolutionCheckResponse {
                verdict: DdnsResolutionVerdict::Pending,
                fqdn: Some(s.fqdn.clone()),
                expected_ip: Some(s.public_ip.clone()),
                resolved_ips: Vec::new(),
            },
        })
    }

    async fn set_acme_challenge(&self, _values: &[String]) -> Result<(), AppError> {
        Ok(())
    }

    async fn clear_acme_challenge(&self) -> Result<(), AppError> {
        Ok(())
    }
}

/// Mock [`TlsService`] — see [module docs](self).
pub struct MockTlsService {
    state: Arc<MockRemoteAccessState>,
}

impl MockTlsService {
    #[must_use]
    pub fn new(state: Arc<MockRemoteAccessState>) -> Self {
        Self { state }
    }
}

#[async_trait]
impl TlsService for MockTlsService {
    async fn ensure_certificate(&self) -> Result<TlsStatus, AppError> {
        // No real ACME work — the phase is derived from elapsed time, so the
        // background task the register handler spawns just reads current state.
        Ok(self.state.tls_status())
    }

    async fn status(&self) -> Result<TlsStatus, AppError> {
        Ok(self.state.tls_status())
    }

    async fn mark_provisioning_started(&self) -> Result<(), AppError> {
        Ok(())
    }

    async fn provisioning_status(&self) -> Result<TlsStatusResponse, AppError> {
        Ok(self.state.tls_status_response())
    }

    async fn teardown(&self) -> Result<(), AppError> {
        self.state.clear();
        Ok(())
    }
}
