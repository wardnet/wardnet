use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use sysinfo::{Disks, System};
use tokio_util::sync::CancellationToken;
use wardnet_common::api::{
    DhcpSelfProbeResponse, DiscoverGatewayMacRequest, DiscoverGatewayMacResponse,
    LastShutdownStatus, NetworkStatusResponse, RouterMacSource, SystemStatusResponse,
};

use crate::auth_context;
use crate::error::AppError;
use crate::system::{NetworkInspector, NetworkProbe, SystemPowerOps};
use wardnetd_data::repository::{SystemConfigRepository, TunnelRepository};

/// Loose colon-separated 6-octet MAC validator (e.g. `aa:bb:cc:dd:ee:ff`).
/// Accepts upper or lower case; canonicalises to lowercase for storage
/// per the project-wide MAC convention (issue #312).
fn normalise_mac(input: &str) -> Option<String> {
    let trimmed = input.trim();
    let bytes: Vec<&str> = trimmed.split(':').collect();
    if bytes.len() != 6 {
        return None;
    }
    let mut out = String::with_capacity(17);
    for (i, byte) in bytes.iter().enumerate() {
        if byte.len() != 2 || !byte.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        if i > 0 {
            out.push(':');
        }
        out.push_str(&byte.to_lowercase());
    }
    Some(out)
}

/// How long the restart handler waits before cancelling the shutdown
/// token. Lets the HTTP response flush so the client sees
/// `204 No Content` before the graceful-shutdown path starts draining
/// connections. Anything longer isn't needed; anything shorter
/// occasionally cuts the response short on a busy connection.
const RESTART_GRACE: Duration = Duration::from_millis(500);

/// Same grace window for `request_reboot` / `request_shutdown` —
/// lets the 204 flush before we ask logind to take the host down.
const POWER_GRACE: Duration = Duration::from_millis(500);

/// System-wide status and health information.
///
/// Aggregates daemon metadata (version, uptime) with counts from the database
/// and host resource usage (CPU, memory) to produce a single status snapshot
/// for the admin dashboard.
#[async_trait]
pub trait SystemService: Send + Sync {
    /// Return the daemon version string. No authentication required.
    fn version(&self) -> &'static str;

    /// Return the daemon uptime. No authentication required.
    fn uptime(&self) -> std::time::Duration;

    /// Build a status response with version, uptime, entity counts, and host resource usage.
    async fn status(&self) -> Result<SystemStatusResponse, AppError>;

    /// Read the LAN interface's current address + default gateway and
    /// classify whether the IP came from DHCP or a Wardnet-managed
    /// static config.
    ///
    /// Powers the wizard's network step (read-only confirmation) and
    /// the post-install Settings page.
    async fn network_status(&self) -> Result<NetworkStatusResponse, AppError>;

    /// Send a DHCPDISCOVER from a synthetic MAC and report whether
    /// Wardnet (and any foreign DHCP server) responded.
    ///
    /// The wizard's primary-mode DHCP-onboarding step calls this after
    /// the operator says they've disabled DHCP on their router; a
    /// foreign-only response tells the UI to re-show the disable-DHCP
    /// guide and a Wardnet-only response means the LAN is correctly
    /// owned.
    async fn dhcp_self_probe(&self) -> Result<DhcpSelfProbeResponse, AppError>;

    /// Discover (or accept) the upstream router MAC and persist it.
    ///
    /// If `request.mac` is set the value is validated and stored as
    /// the `manual` source. Otherwise an ARP probe is sent at
    /// `request.target_ip` (falling back to the gateway from
    /// [`Self::network_status`]); a successful reply is stored as the
    /// `arp` source. Auto-discovery returning no reply surfaces as
    /// `AppError::NotFound` so the UI can drop into manual entry.
    async fn discover_gateway_mac(
        &self,
        request: DiscoverGatewayMacRequest,
    ) -> Result<DiscoverGatewayMacResponse, AppError>;

    /// Ask the daemon to exit cleanly so the supervisor (systemd on a
    /// Pi install, the operator on dev) brings it back up.
    ///
    /// The call returns immediately; a background task performs the
    /// actual `exit(0)` after a short grace period so the HTTP
    /// response completes before the socket closes. Admin-only.
    async fn request_restart(&self) -> Result<(), AppError>;

    /// Ask the host to reboot (Pi-level, not just the daemon).
    ///
    /// Symmetric with [`Self::request_restart`]: returns immediately,
    /// then a spawned task waits ~500 ms for the response to flush
    /// before invoking the wired [`SystemPowerOps::reboot`]. systemd
    /// drives the actual shutdown sequence and SIGTERMs wardnetd as
    /// part of `shutdown.target`. Admin-only.
    async fn request_reboot(&self) -> Result<(), AppError>;

    /// Ask the host to power off. Same shape as [`Self::request_reboot`]
    /// but invokes [`SystemPowerOps::poweroff`]. Admin-only.
    async fn request_shutdown(&self) -> Result<(), AppError>;

    /// Update the heartbeat marker so a future unclean-shutdown
    /// classification has a recent `last_seen_at` to surface.
    ///
    /// Called by the background `HeartbeatRunner` on a 60-second
    /// cadence. Admin-only — the runner wraps the call in
    /// `auth_context::with_context(AuthContext::Admin { admin_id: Uuid::nil() })`.
    async fn record_heartbeat(&self) -> Result<(), AppError>;

    /// Record that the daemon is exiting gracefully so the next boot
    /// classifies the prior run as `Graceful` (not `Unclean`).
    ///
    /// Called from `main.rs` after the runner-shutdown sequence
    /// completes and before the process exits. Admin-only — invoked
    /// from the shutdown path inside an `auth_context::with_context`
    /// wrapper.
    async fn record_graceful_shutdown(&self) -> Result<(), AppError>;

    /// Acknowledge the most recent unclean shutdown so the dashboard
    /// banner is dismissed. Idempotent. Admin-only.
    async fn acknowledge_last_shutdown(&self) -> Result<(), AppError>;
}

/// Default implementation of [`SystemService`] backed by [`SystemConfigRepository`].
///
/// Holds a persistent `sysinfo::System` instance behind a `tokio::sync::Mutex`
/// so that successive calls to `status()` produce accurate CPU readings
/// (sysinfo needs two measurement points to compute CPU usage).
///
/// Also carries the daemon's shutdown [`CancellationToken`].
/// `request_restart` cancels it after a short grace period, which in
/// turn triggers the `with_graceful_shutdown` arm in `main.rs`
/// (see `shutdown_signal`) — axum finishes draining in-flight
/// requests and the background runners shut down cleanly via their
/// own `shutdown().await` sequence. Plumbing the same token through
/// here keeps `std::process::exit` out of the codebase.
pub struct SystemServiceImpl {
    system_config: Arc<dyn SystemConfigRepository>,
    tunnel_repo: Arc<dyn TunnelRepository>,
    started_at: Instant,
    sysinfo: tokio::sync::Mutex<System>,
    shutdown_token: CancellationToken,
    power_ops: Arc<dyn SystemPowerOps>,
    network_inspector: Arc<dyn NetworkInspector>,
    network_probe: Arc<dyn NetworkProbe>,
    db_path: Option<PathBuf>,
}

impl SystemServiceImpl {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        system_config: Arc<dyn SystemConfigRepository>,
        tunnel_repo: Arc<dyn TunnelRepository>,
        started_at: Instant,
        shutdown_token: CancellationToken,
        power_ops: Arc<dyn SystemPowerOps>,
        network_inspector: Arc<dyn NetworkInspector>,
        network_probe: Arc<dyn NetworkProbe>,
        db_path: Option<PathBuf>,
    ) -> Self {
        Self {
            system_config,
            tunnel_repo,
            started_at,
            sysinfo: tokio::sync::Mutex::new(System::new_all()),
            shutdown_token,
            power_ops,
            network_inspector,
            network_probe,
            db_path,
        }
    }
}

#[async_trait]
impl SystemService for SystemServiceImpl {
    fn version(&self) -> &'static str {
        env!("WARDNET_VERSION")
    }

    fn uptime(&self) -> std::time::Duration {
        self.started_at.elapsed()
    }

    async fn status(&self) -> Result<SystemStatusResponse, AppError> {
        auth_context::require_admin()?;

        let version = self.version().to_owned();

        let device_count = self
            .system_config
            .device_count()
            .await
            .map_err(AppError::Internal)?;

        let tunnel_count = self
            .system_config
            .tunnel_count()
            .await
            .map_err(AppError::Internal)?;

        let db_size_bytes = self
            .system_config
            .db_size_bytes()
            .await
            .map_err(AppError::Internal)?;

        let last_shutdown_info = self
            .system_config
            .read_last_shutdown()
            .await
            .map_err(AppError::Internal)?;
        let last_shutdown = LastShutdownStatus {
            state: last_shutdown_info.state,
            at: last_shutdown_info.at,
            acknowledged_at: last_shutdown_info.acknowledged_at,
        };

        // Refresh CPU and memory readings from the persistent sysinfo instance.
        let (cpu_usage_percent, memory_used_bytes, memory_total_bytes) = {
            let mut sys = self.sysinfo.lock().await;
            sys.refresh_cpu_all();
            sys.refresh_memory();
            (
                sys.global_cpu_usage(),
                sys.used_memory(),
                sys.total_memory(),
            )
        };

        let (disk_free_bytes, disk_total_bytes) = if let Some(ref db_path) = self.db_path {
            let path = db_path.clone();
            tokio::task::spawn_blocking(move || {
                let disks = Disks::new_with_refreshed_list();
                let best = disks
                    .iter()
                    .filter(|d| path.starts_with(d.mount_point()))
                    .max_by_key(|d| d.mount_point().as_os_str().len());
                best.map_or((0, 0), |d| (d.available_space(), d.total_space()))
            })
            .await
            .map_err(|e| AppError::Internal(e.into()))?
        } else {
            (0, 0)
        };

        Ok(SystemStatusResponse {
            version,
            release_version: crate::version::RELEASE_VERSION.to_owned(),
            uptime_seconds: self.started_at.elapsed().as_secs(),
            device_count: u64::try_from(device_count).unwrap_or(0),
            tunnel_count: u64::try_from(tunnel_count).unwrap_or(0),
            tunnel_active_count: u64::try_from(
                self.tunnel_repo
                    .count_active()
                    .await
                    .map_err(AppError::Internal)?,
            )
            .unwrap_or(0),
            db_size_bytes,
            cpu_usage_percent,
            memory_used_bytes,
            memory_total_bytes,
            disk_free_bytes,
            disk_total_bytes,
            last_shutdown,
        })
    }

    async fn network_status(&self) -> Result<NetworkStatusResponse, AppError> {
        auth_context::require_admin()?;
        let snap = self
            .network_inspector
            .inspect()
            .await
            .map_err(AppError::Internal)?;
        let router_mac = self
            .system_config
            .get_router_mac()
            .await
            .map_err(AppError::Internal)?;
        Ok(NetworkStatusResponse {
            interface: snap.interface,
            ip: snap.ip,
            gateway: snap.gateway,
            dhcp_source: snap.dhcp_source,
            router_mac,
        })
    }

    async fn dhcp_self_probe(&self) -> Result<DhcpSelfProbeResponse, AppError> {
        auth_context::require_admin()?;

        let snap = self
            .network_inspector
            .inspect()
            .await
            .map_err(AppError::Internal)?;
        let our_ip = snap.ip;

        let outcome = self
            .network_probe
            .dhcp_self_probe()
            .await
            .map_err(AppError::Internal)?;

        let mut wardnet_responded = false;
        let mut foreign_server_ip: Option<std::net::Ipv4Addr> = None;
        for ip in outcome.responders {
            if ip == our_ip {
                wardnet_responded = true;
            } else if foreign_server_ip.is_none() {
                foreign_server_ip = Some(ip);
            }
        }

        let foreign_responded = foreign_server_ip.is_some();

        tracing::info!(
            wardnet_responded,
            foreign_responded,
            ?foreign_server_ip,
            "dhcp self-probe complete"
        );

        Ok(DhcpSelfProbeResponse {
            wardnet_responded,
            foreign_responded,
            foreign_server_ip,
        })
    }

    async fn discover_gateway_mac(
        &self,
        request: DiscoverGatewayMacRequest,
    ) -> Result<DiscoverGatewayMacResponse, AppError> {
        auth_context::require_admin()?;

        // Manual path: validate the operator's input and persist as-is.
        if let Some(raw) = request.mac.as_deref() {
            let normalised = normalise_mac(raw).ok_or_else(|| {
                AppError::BadRequest(format!(
                    "MAC address must be six colon-separated hex octets, got {raw}"
                ))
            })?;
            self.system_config
                .set_router_mac(&normalised)
                .await
                .map_err(AppError::Internal)?;
            return Ok(DiscoverGatewayMacResponse {
                mac: normalised,
                source: RouterMacSource::Manual,
            });
        }

        // Auto path: figure out which IP to probe. Prefer the explicit
        // hint, fall back to the inspector's gateway.
        let target_ip = if let Some(ip) = request.target_ip {
            ip
        } else {
            let snap = self
                .network_inspector
                .inspect()
                .await
                .map_err(AppError::Internal)?;
            snap.gateway.ok_or_else(|| {
                AppError::BadRequest(
                    "no default gateway detected; supply target_ip or fall back to manual entry"
                        .to_owned(),
                )
            })?
        };

        let mac = self
            .network_probe
            .arp_probe(target_ip)
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| {
                AppError::NotFound(format!(
                    "no ARP reply from {target_ip}; manual entry required"
                ))
            })?;

        let normalised = normalise_mac(&mac).ok_or_else(|| {
            AppError::Internal(anyhow::anyhow!("ARP probe returned malformed MAC: {mac}"))
        })?;

        self.system_config
            .set_router_mac(&normalised)
            .await
            .map_err(AppError::Internal)?;

        tracing::info!(
            mac = %normalised,
            target = %target_ip,
            "discovered upstream router MAC via ARP"
        );

        Ok(DiscoverGatewayMacResponse {
            mac: normalised,
            source: RouterMacSource::Arp,
        })
    }

    async fn request_restart(&self) -> Result<(), AppError> {
        auth_context::require_admin()?;
        tracing::warn!(
            grace_ms = RESTART_GRACE.as_millis(),
            "daemon restart requested via API — triggering graceful shutdown in {grace_ms}ms",
            grace_ms = RESTART_GRACE.as_millis(),
        );
        let token = self.shutdown_token.clone();
        tokio::spawn(async move {
            tokio::time::sleep(RESTART_GRACE).await;
            // Cancelling the token wakes `shutdown_signal` in main.rs
            // which returns from `axum::serve.with_graceful_shutdown`.
            // The main task then runs the existing runner.shutdown()
            // sequence before the process exits with code 0.
            // `Restart=always` in wardnetd.service brings the daemon
            // back up on a Pi install; on the dev mock the process
            // exits cleanly and the operator re-runs `make run-dev`.
            token.cancel();
        });
        Ok(())
    }

    async fn request_reboot(&self) -> Result<(), AppError> {
        auth_context::require_admin()?;
        tracing::warn!(
            action = "reboot",
            grace_ms = POWER_GRACE.as_millis(),
            "host reboot requested via API — invoking systemctl reboot in {grace_ms}ms",
            grace_ms = POWER_GRACE.as_millis(),
        );
        let ops = self.power_ops.clone();
        // systemd drives the rest of the shutdown sequence: it sends
        // SIGTERM to wardnetd as part of `shutdown.target`, our
        // existing `shutdown_signal` handler runs, and the unit stops
        // before the host reboots. We never touch `shutdown_token`
        // here — single source of truth for the actual shutdown is
        // systemd, not the daemon.
        tokio::spawn(async move {
            tokio::time::sleep(POWER_GRACE).await;
            if let Err(e) = ops.reboot().await {
                tracing::error!(error = %e, "host reboot invocation failed: error={e}");
            }
        });
        Ok(())
    }

    async fn request_shutdown(&self) -> Result<(), AppError> {
        auth_context::require_admin()?;
        tracing::warn!(
            action = "shutdown",
            grace_ms = POWER_GRACE.as_millis(),
            "host shutdown requested via API — invoking systemctl poweroff in {grace_ms}ms",
            grace_ms = POWER_GRACE.as_millis(),
        );
        let ops = self.power_ops.clone();
        tokio::spawn(async move {
            tokio::time::sleep(POWER_GRACE).await;
            if let Err(e) = ops.poweroff().await {
                tracing::error!(error = %e, "host poweroff invocation failed: error={e}");
            }
        });
        Ok(())
    }

    async fn record_heartbeat(&self) -> Result<(), AppError> {
        auth_context::require_admin()?;
        self.system_config
            .record_heartbeat()
            .await
            .map_err(AppError::Internal)?;
        Ok(())
    }

    async fn record_graceful_shutdown(&self) -> Result<(), AppError> {
        auth_context::require_admin()?;
        self.system_config
            .record_graceful_shutdown()
            .await
            .map_err(AppError::Internal)?;
        Ok(())
    }

    async fn acknowledge_last_shutdown(&self) -> Result<(), AppError> {
        auth_context::require_admin()?;
        self.system_config
            .acknowledge_last_shutdown()
            .await
            .map_err(AppError::Internal)?;
        Ok(())
    }
}
