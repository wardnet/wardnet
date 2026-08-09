use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use async_trait::async_trait;
use base64::Engine;
use tokio::sync::Mutex;
use uuid::Uuid;
use wardnet_common::api::{
    CreateTunnelRequest, CreateTunnelResponse, DeleteTunnelResponse, ListTunnelsResponse,
    TunnelDevicesResponse, TunnelTestResult,
};
use wardnet_common::event::WardnetEvent;
use wardnet_common::jobs::{JobDispatchedResponse, JobKind};
use wardnet_common::speed_test::TunnelSpeedTestHistoryResponse;
use wardnet_common::tunnel::{Tunnel, TunnelStatus};
use wardnet_common::wireguard_config;

use crate::auth_context;
use crate::error::AppError;
use crate::event::EventPublisher;
use crate::jobs::{JobService, JobServiceExt, ProgressReporter};
use crate::stats::meter::Meter;
use crate::tunnel::exit_probe::{ProbeError, TunnelExitProbe};
use crate::tunnel::interface::{
    CreateTunnelParams, TunnelConfig as TiTunnelConfig, TunnelInterface,
};
use crate::tunnel::latency_prober::{LatencyProbeError, TunnelLatencyProber};
use crate::tunnel::throughput_tester::ThroughputTester;
use crate::vpn::resolver::{EmptyServerListError, ServerResolver};
use wardnetd_data::repository::tunnel::TunnelRow;
use wardnetd_data::repository::tunnel_speed_test::{SpeedTestRow, TunnelSpeedTestRepository};
use wardnetd_data::repository::{SystemConfigRepository, TunnelRepository};
use wardnetd_data::secret_store::SecretStore;

use crate::tunnel::key_store::{KeyStore, KeyStoreAdapter};

/// Tunnel lifecycle management.
///
/// Orchestrates importing, bringing up, tearing down, and deleting
/// `WireGuard` tunnels. Coordinates between the repository (persistence),
/// key store (private keys on disk), `WireGuard` ops (kernel interface),
/// and event publisher (domain events).
#[async_trait]
pub trait TunnelService: Send + Sync {
    /// Import a tunnel from a `WireGuard` `.conf` file. Tunnel starts `Down`.
    async fn import_tunnel(
        &self,
        req: CreateTunnelRequest,
    ) -> Result<CreateTunnelResponse, AppError>;

    /// List all configured tunnels.
    async fn list_tunnels(&self) -> Result<ListTunnelsResponse, AppError>;

    /// Get a single tunnel by ID.
    async fn get_tunnel(&self, id: Uuid) -> Result<Tunnel, AppError>;

    /// Run an exit-IP/country probe through the tunnel.
    ///
    /// Brings the tunnel up if it was `Down`, waits for a fresh
    /// handshake, sends a single HTTP probe through the tunnel
    /// interface, then restores the prior up/down state. Returns the
    /// observed exit IP, ISO-3166 alpha-2 country code, and probe
    /// latency.
    ///
    /// Returns `AppError::Conflict` if a test is already in flight for
    /// the same tunnel id.
    async fn test_tunnel(&self, id: Uuid) -> Result<TunnelTestResult, AppError>;

    /// List the devices currently routed through this tunnel.
    async fn list_tunnel_devices(&self, id: Uuid) -> Result<TunnelDevicesResponse, AppError>;

    /// Update the per-tunnel `override_default_dns` flag.
    ///
    /// Toggles whether tunneled-device DNS queries are filtered + forwarded
    /// through the tunnel's DNS server (`true`) or left to the system-wide
    /// upstream pool (`false`).
    async fn set_dns_override(&self, id: Uuid, value: bool) -> Result<Tunnel, AppError>;

    /// Tear down and re-apply a tunnel interface from scratch (admin rebuild).
    ///
    /// Equivalent to the watchdog recovery path: tears down the `WireGuard`
    /// interface and brings it back up from the persisted config. Use when
    /// the tunnel is stale or devices cannot route through it.
    async fn rebuild(&self, id: Uuid) -> Result<(), AppError>;

    /// Bring a tunnel interface up.
    async fn bring_up(&self, id: Uuid) -> Result<(), AppError>;

    /// Tear down a tunnel interface.
    async fn tear_down(&self, id: Uuid, reason: &str) -> Result<(), AppError>;

    /// Delete a tunnel entirely (removes config, key, and interface).
    async fn delete_tunnel(&self, id: Uuid) -> Result<DeleteTunnelResponse, AppError>;

    /// Bring a tunnel interface up without requiring admin authentication.
    ///
    /// Used internally by the routing engine when a device's routing rule
    /// targets a tunnel that is currently down.
    async fn bring_up_internal(&self, id: Uuid) -> Result<(), AppError>;

    /// Tear down a tunnel interface without requiring admin authentication.
    ///
    /// Used internally by the idle tunnel watcher and routing engine for
    /// automated lifecycle management.
    async fn tear_down_internal(&self, id: Uuid, reason: &str) -> Result<(), AppError>;

    /// Restore tunnel configs from the database on startup (does NOT bring interfaces up).
    async fn restore_tunnels(&self) -> Result<(), AppError>;

    /// Collect stats for all Up tunnels, update the database, and publish events.
    ///
    /// Used by the tunnel monitor background task. No auth guard — called from
    /// background task context.
    async fn collect_stats(&self) -> Result<(), AppError>;

    /// Run health checks on all Up tunnels, logging warnings for stale handshakes.
    ///
    /// Used by the tunnel monitor background task. No auth guard — called from
    /// background task context.
    async fn run_health_check(&self) -> Result<(), AppError>;

    /// Probe the latency of every active tunnel and record it as a
    /// `tunnel.latency.rtt_ms` gauge labelled by `tunnel_id`.
    ///
    /// Used by the tunnel monitor background task. No auth guard — called from
    /// background task context.
    async fn probe_latencies(&self) -> Result<(), AppError>;

    /// Start an asynchronous speed test for a tunnel and return the job id.
    ///
    /// The handler returns immediately (202); the background job measures
    /// throughput, latency and jitter twice — once over the direct (WAN)
    /// path and once through the tunnel — and persists a single result row.
    /// If the tunnel is `Down` it is brought up for the run and torn back
    /// down afterwards. A concurrent run for the same tunnel (or an in-flight
    /// [`test_tunnel`](Self::test_tunnel)) returns `AppError::Conflict`; the
    /// in-flight slot is held for the full job duration.
    async fn start_speed_test(self: Arc<Self>, id: Uuid)
    -> Result<JobDispatchedResponse, AppError>;

    /// List the most recent speed test results for a tunnel, newest first
    /// (bounded to the last few runs).
    async fn list_speed_tests(&self, id: Uuid) -> Result<TunnelSpeedTestHistoryResponse, AppError>;
}

/// Default implementation of [`TunnelService`].
pub struct TunnelServiceImpl {
    tunnels: Arc<dyn TunnelRepository>,
    devices: Arc<dyn wardnetd_data::repository::DeviceRepository>,
    /// Holds the global `default_policy`. Read on deletion so a tunnel
    /// that *is* the default policy resets it to `"direct"` instead of
    /// leaving a dangling tunnel UUID behind.
    system_config: Arc<dyn SystemConfigRepository>,
    tunnel_interface: Arc<dyn TunnelInterface>,
    exit_probe: Arc<dyn TunnelExitProbe>,
    latency_prober: Arc<dyn TunnelLatencyProber>,
    throughput_tester: Arc<dyn ThroughputTester>,
    keys: Arc<dyn KeyStore>,
    events: Arc<dyn EventPublisher>,
    meter: Arc<Meter>,
    server_resolver: Arc<dyn ServerResolver>,
    jobs: Arc<dyn JobService>,
    speed_test_repo: Arc<dyn TunnelSpeedTestRepository>,
    /// Number of ICMP samples taken per leg of a speed test to derive median
    /// latency and jitter. From `config.tunnel.speed_test_latency_samples`.
    speed_test_latency_samples: u32,
    /// Last cumulative `(bytes_tx, bytes_rx)` observed per tunnel.
    /// Used by `collect_stats` to compute positive deltas to feed into
    /// the generic stats pipeline. A counter that goes *down* means the
    /// tunnel reconnected (the kernel `WireGuard` counters reset on
    /// recreate); we treat that as a baseline reset and skip the delta.
    last_bytes: Mutex<HashMap<Uuid, (u64, u64)>>,
    /// Tunnel ids with a `test_tunnel` call in progress. Each id is
    /// inserted on entry and removed via the [`InFlightGuard`] RAII
    /// guard. Concurrent calls for the same id return
    /// `AppError::Conflict`.
    tests_in_flight: Arc<std::sync::Mutex<HashSet<Uuid>>>,
}

/// Parse the port from a `host:port` endpoint string. Defaults to `51820`
/// if the endpoint is absent, malformed, or the port cannot be parsed.
fn parse_port(endpoint: Option<&str>) -> u16 {
    let port_str = endpoint.and_then(|ep| ep.rsplit(':').next());
    if let Some(p) = port_str.and_then(|p| p.parse::<u16>().ok()) {
        p
    } else {
        if endpoint.is_some() {
            tracing::warn!(endpoint = ?endpoint, "could not parse port from endpoint, using default 51820");
        }
        51820
    }
}

/// Number of recent speed test results returned by `list_speed_tests`.
const SPEED_TEST_HISTORY_LIMIT: i64 = 5;

/// One leg of a speed test (direct or tunnel).
struct LegMeasurement {
    throughput_mbps: f64,
    latency_ms: f64,
    jitter_ms: f64,
}

/// Reduce a set of ICMP RTT samples (ms) to `(median, jitter)`, where jitter
/// is the sample standard deviation. Returns `(0.0, 0.0)` for an empty slice
/// (callers reject empty sample sets before this point).
pub(crate) fn summarize_latency(samples: &[u64]) -> (f64, f64) {
    if samples.is_empty() {
        return (0.0, 0.0);
    }
    let mut sorted: Vec<u64> = samples.to_vec();
    sorted.sort_unstable();
    let mid = sorted.len() / 2;
    #[allow(clippy::cast_precision_loss)]
    let median = if sorted.len().is_multiple_of(2) {
        f64::midpoint(sorted[mid - 1] as f64, sorted[mid] as f64)
    } else {
        sorted[mid] as f64
    };

    #[allow(clippy::cast_precision_loss)]
    let n = samples.len() as f64;
    #[allow(clippy::cast_precision_loss)]
    let mean = samples.iter().map(|&s| s as f64).sum::<f64>() / n;
    let jitter = if samples.len() > 1 {
        #[allow(clippy::cast_precision_loss)]
        let variance = samples
            .iter()
            .map(|&s| {
                let d = s as f64 - mean;
                d * d
            })
            .sum::<f64>()
            / (n - 1.0);
        variance.sqrt()
    } else {
        0.0
    };
    (median, jitter)
}

impl TunnelServiceImpl {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        tunnels: Arc<dyn TunnelRepository>,
        devices: Arc<dyn wardnetd_data::repository::DeviceRepository>,
        system_config: Arc<dyn SystemConfigRepository>,
        tunnel_interface: Arc<dyn TunnelInterface>,
        exit_probe: Arc<dyn TunnelExitProbe>,
        latency_prober: Arc<dyn TunnelLatencyProber>,
        throughput_tester: Arc<dyn ThroughputTester>,
        secret_store: Arc<dyn SecretStore>,
        events: Arc<dyn EventPublisher>,
        meter: Arc<Meter>,
        server_resolver: Arc<dyn ServerResolver>,
        jobs: Arc<dyn JobService>,
        speed_test_repo: Arc<dyn TunnelSpeedTestRepository>,
        speed_test_latency_samples: u32,
    ) -> Self {
        let keys: Arc<dyn KeyStore> = Arc::new(KeyStoreAdapter::new(secret_store));
        Self {
            tunnels,
            devices,
            system_config,
            tunnel_interface,
            exit_probe,
            latency_prober,
            throughput_tester,
            keys,
            events,
            meter,
            server_resolver,
            jobs,
            speed_test_repo,
            speed_test_latency_samples,
            last_bytes: Mutex::new(HashMap::new()),
            tests_in_flight: Arc::new(std::sync::Mutex::new(HashSet::new())),
        }
    }

    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn with_key_store(
        tunnels: Arc<dyn TunnelRepository>,
        devices: Arc<dyn wardnetd_data::repository::DeviceRepository>,
        system_config: Arc<dyn SystemConfigRepository>,
        tunnel_interface: Arc<dyn TunnelInterface>,
        exit_probe: Arc<dyn TunnelExitProbe>,
        latency_prober: Arc<dyn TunnelLatencyProber>,
        throughput_tester: Arc<dyn ThroughputTester>,
        keys: Arc<dyn KeyStore>,
        events: Arc<dyn EventPublisher>,
        meter: Arc<Meter>,
        server_resolver: Arc<dyn ServerResolver>,
        jobs: Arc<dyn JobService>,
        speed_test_repo: Arc<dyn TunnelSpeedTestRepository>,
        speed_test_latency_samples: u32,
    ) -> Self {
        Self {
            tunnels,
            devices,
            system_config,
            tunnel_interface,
            exit_probe,
            latency_prober,
            throughput_tester,
            keys,
            events,
            meter,
            server_resolver,
            jobs,
            speed_test_repo,
            speed_test_latency_samples,
            last_bytes: Mutex::new(HashMap::new()),
            tests_in_flight: Arc::new(std::sync::Mutex::new(HashSet::new())),
        }
    }

    /// Look up a tunnel by ID, returning `AppError::NotFound` when absent.
    async fn require_tunnel(&self, id: Uuid) -> Result<Tunnel, AppError> {
        self.tunnels
            .find_by_id(&id.to_string())
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::NotFound(format!("tunnel {id} not found")))
    }

    /// Decode a base64-encoded `WireGuard` key into a 32-byte array.
    fn decode_key(b64: &str) -> Result<[u8; 32], AppError> {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b64.trim())
            .map_err(|e| AppError::Internal(anyhow::anyhow!("invalid base64 key: {e}")))?;
        bytes
            .try_into()
            .map_err(|_| AppError::Internal(anyhow::anyhow!("WireGuard key must be 32 bytes")))
    }

    /// Core logic for bringing a tunnel up (no auth check).
    async fn bring_up_core(&self, id: Uuid) -> Result<(), AppError> {
        let tunnel = self.require_tunnel(id).await?;

        // No-op if the kernel interface is already configured. `Up`,
        // `Connecting`, and `Reconnecting` all mean "iface exists" — only
        // `Down` requires (re)creation.
        if tunnel.status != TunnelStatus::Down {
            return Ok(());
        }

        // On any failure to configure the interface, emit `TunnelStartFailed`
        // (drives the admin-PWA "failed to start" push) before propagating.
        let interface_name = tunnel.interface_name.clone();
        if let Err(error) = self.bring_up_configure(id, tunnel).await {
            self.events.publish(WardnetEvent::TunnelStartFailed {
                tunnel_id: id,
                interface_name,
                error: error.to_string(),
                timestamp: chrono::Utc::now(),
            });
            return Err(error);
        }
        Ok(())
    }

    /// The fallible interface-configuration work of [`Self::bring_up_core`],
    /// split out so the caller can wrap any failure into a `TunnelStartFailed`
    /// event. `tunnel` is the already-loaded, confirmed-`Down` record.
    async fn bring_up_configure(&self, id: Uuid, tunnel: Tunnel) -> Result<(), AppError> {
        // Load stored `WireGuard` configuration.
        let tunnel_config = self
            .tunnels
            .find_config_by_id(&id.to_string())
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::NotFound(format!("tunnel config {id} not found")))?;

        // Re-resolve endpoint for "best server" tunnels on each bring-up.
        let tunnel_config = self.reresolve_endpoint(id, &tunnel, tunnel_config).await?;

        // Load and decode private key from key store.
        let private_key_b64 = self.keys.load_key(&id).await.map_err(AppError::Internal)?;
        let private_key = Self::decode_key(&private_key_b64)?;

        // Decode peer public key.
        let peer_public_key = Self::decode_key(&tunnel_config.peer.public_key)?;

        // Decode optional preshared key.
        let peer_preshared_key = tunnel_config
            .peer
            .preshared_key
            .as_deref()
            .map(Self::decode_key)
            .transpose()?;

        // Parse peer endpoint — resolve hostname if needed (e.g. a provider
        // hostname like `pt149.nordvpn.com:51820` must be resolved to an IP).
        let peer_endpoint =
            Self::resolve_peer_endpoint(tunnel_config.peer.endpoint.as_deref()).await?;

        // Parse allowed IPs.
        let peer_allowed_ips = tunnel_config
            .peer
            .allowed_ips
            .iter()
            .map(|ip| ip.parse())
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| AppError::Internal(anyhow::anyhow!("invalid allowed IP: {e}")))?;

        // Parse interface addresses (e.g. `10.66.0.2/32`).
        let interface_addresses = tunnel_config
            .address
            .iter()
            .map(|a| a.parse())
            .collect::<Result<Vec<ipnetwork::IpNetwork>, _>>()
            .map_err(|e| AppError::Internal(anyhow::anyhow!("invalid interface address: {e}")))?;

        let params = CreateTunnelParams {
            interface_name: tunnel.interface_name.clone(),
            config: TiTunnelConfig::WireGuard {
                address: interface_addresses,
                private_key,
                listen_port: tunnel_config.listen_port,
                peer_public_key,
                peer_endpoint,
                peer_allowed_ips,
                peer_preshared_key,
                persistent_keepalive: tunnel_config.peer.persistent_keepalive,
            },
        };

        // Create the tunnel interface and bring it up.
        self.tunnel_interface
            .create(params)
            .await
            .map_err(AppError::Internal)?;
        self.tunnel_interface
            .bring_up(&tunnel.interface_name)
            .await
            .map_err(AppError::Internal)?;

        // Update status in the database. Stays `connecting` until the
        // health-check loop observes the first handshake — see
        // `run_health_check`.
        self.tunnels
            .update_status(&id.to_string(), "connecting")
            .await
            .map_err(AppError::Internal)?;

        // Publish domain event. `TunnelUp` is reserved for the moment a
        // handshake is actually observed.
        self.events.publish(WardnetEvent::TunnelConnecting {
            tunnel_id: id,
            interface_name: tunnel.interface_name,
            endpoint: tunnel.endpoint,
            timestamp: chrono::Utc::now(),
        });

        Ok(())
    }

    /// Re-resolve the best server endpoint for "best server" tunnels.
    /// For tunnels with a static server or no provider, the config is
    /// returned unchanged.
    async fn reresolve_endpoint(
        &self,
        id: Uuid,
        tunnel: &Tunnel,
        config: wardnet_common::tunnel::TunnelConfig,
    ) -> Result<wardnet_common::tunnel::TunnelConfig, AppError> {
        let (Some(selector), Some(_)) = (&tunnel.server_selector, &tunnel.provider) else {
            return Ok(config);
        };
        let provider_id = tunnel.provider.as_deref().expect("checked above");
        let port = parse_port(config.peer.endpoint.as_deref());
        match self
            .server_resolver
            .resolve(provider_id, selector, port)
            .await
        {
            Ok(Some((new_ep, server_name))) => {
                let now = chrono::Utc::now().to_rfc3339();
                let mut updated_peer = config.peer.clone();
                updated_peer.endpoint = Some(new_ep);
                let peer_json = serde_json::to_string(&updated_peer)
                    .map_err(|e| AppError::Internal(e.into()))?;
                if let Err(e) = self
                    .tunnels
                    .update_endpoint(
                        &id.to_string(),
                        updated_peer.endpoint.as_deref().unwrap_or_default(),
                        &peer_json,
                        &server_name,
                        &now,
                    )
                    .await
                {
                    tracing::warn!(
                        tunnel_id = %id,
                        error = %e,
                        "endpoint re-resolution: failed to persist updated endpoint",
                    );
                }
                Ok(wardnet_common::tunnel::TunnelConfig {
                    peer: updated_peer,
                    ..config
                })
            }
            Ok(None) => {
                tracing::warn!(
                    tunnel_id = %id,
                    "no provider registered for re-resolution, using stored endpoint",
                );
                Ok(config)
            }
            Err(e) => {
                if e.downcast_ref::<EmptyServerListError>().is_some() {
                    return Err(AppError::Internal(e));
                }
                tracing::warn!(
                    tunnel_id = %id,
                    error = %e,
                    "endpoint re-resolution failed (transient), using stored endpoint",
                );
                Ok(config)
            }
        }
    }

    /// Resolve a peer endpoint string to a `SocketAddr`. Tries direct parse
    /// first (already an IP:port), then falls back to DNS lookup.
    async fn resolve_peer_endpoint(
        ep: Option<&str>,
    ) -> Result<Option<std::net::SocketAddr>, AppError> {
        let Some(ep) = ep else {
            return Ok(None);
        };
        if let Ok(addr) = ep.parse::<std::net::SocketAddr>() {
            return Ok(Some(addr));
        }
        let addr = tokio::net::lookup_host(ep)
            .await
            .map_err(|e| {
                AppError::Internal(anyhow::anyhow!(
                    "failed to resolve peer endpoint '{ep}': {e}"
                ))
            })?
            .next()
            .ok_or_else(|| {
                AppError::Internal(anyhow::anyhow!(
                    "DNS resolution returned no addresses for '{ep}'"
                ))
            })?;
        Ok(Some(addr))
    }

    /// Reconcile DB tunnel status against kernel reality. After a daemon
    /// restart or external `modprobe -r wireguard` / `ip link delete`, the
    /// DB can claim a tunnel is `Up`/`Connecting`/`Reconnecting` while the
    /// kernel has no iface — every subsequent decision based on
    /// `tunnel.status` is then wrong. Flip iface-missing tunnels to `Down`
    /// here so the existing routing-path bring-up and `bring_up_core`'s
    /// `Down` guard both work as designed.
    ///
    /// Returns the subset of tunnels whose iface is still present, ready
    /// for the rest of the health-check pass.
    async fn reconcile_iface_presence(
        &self,
        all_tunnels: Vec<Tunnel>,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<Tunnel>, AppError> {
        let live_ifaces: std::collections::HashSet<String> = self
            .tunnel_interface
            .list()
            .await
            .map_err(AppError::Internal)?
            .into_iter()
            .collect();

        let mut surviving = Vec::new();
        for tunnel in all_tunnels {
            if tunnel.status == TunnelStatus::Down {
                continue;
            }
            if live_ifaces.contains(&tunnel.interface_name) {
                surviving.push(tunnel);
                continue;
            }
            if let Err(e) = self
                .tunnels
                .update_status(&tunnel.id.to_string(), "down")
                .await
            {
                tracing::error!(
                    tunnel_id = %tunnel.id,
                    error = %e,
                    "health check: failed to mark iface-missing tunnel down for {}: {e}",
                    tunnel.interface_name
                );
                continue;
            }
            tracing::warn!(
                tunnel_id = %tunnel.id,
                interface = %tunnel.interface_name,
                previous_status = ?tunnel.status,
                "health check: kernel interface is gone, marking tunnel down"
            );
            self.events.publish(WardnetEvent::TunnelDown {
                tunnel_id: tunnel.id,
                interface_name: tunnel.interface_name,
                reason: "interface absent".to_owned(),
                timestamp: now,
            });
        }
        Ok(surviving)
    }

    /// Tear down + bring up each stuck tunnel sequentially. Errors are
    /// logged and swallowed — one failure should not stop recovery for
    /// the others.
    async fn recreate_stuck_tunnels(&self, ids: Vec<Uuid>) {
        for id in ids {
            tracing::warn!(
                tunnel_id = %id,
                "health check: tunnel stuck in reconnecting, recreating"
            );
            if let Err(e) = self
                .tear_down_core(id, "stuck in reconnecting, recreating")
                .await
            {
                tracing::error!(
                    tunnel_id = %id,
                    error = %e,
                    "health check: failed to tear down stuck tunnel: {e}"
                );
                continue;
            }
            if let Err(e) = self.bring_up_core(id).await {
                tracing::error!(
                    tunnel_id = %id,
                    error = %e,
                    "health check: failed to bring stuck tunnel back up: {e}"
                );
            }
        }
    }

    /// Compute positive deltas since the previous observation and feed
    /// them into the generic stats pipeline as `tunnel.bytes.tx` /
    /// `tunnel.bytes.rx` counter increments, labelled by `tunnel_id`.
    ///
    /// The first observation per tunnel records the baseline without
    /// recording a delta. If a subsequent counter is *lower* than the
    /// stored value we treat it as a reset — the kernel `WireGuard`
    /// counters zero on iface recreate — and refresh the baseline
    /// without recording.
    pub(crate) async fn record_byte_deltas(&self, tunnel_id: Uuid, bytes_tx: u64, bytes_rx: u64) {
        let mut map = self.last_bytes.lock().await;
        let prev = map.insert(tunnel_id, (bytes_tx, bytes_rx));
        let Some((prev_tx, prev_rx)) = prev else {
            return;
        };
        drop(map);

        if bytes_tx < prev_tx || bytes_rx < prev_rx {
            // Counter reset — the new value is the fresh baseline,
            // already stored above.
            return;
        }

        let tx_delta = bytes_tx - prev_tx;
        let rx_delta = bytes_rx - prev_rx;

        let labels = format!(r#"{{"tunnel_id":"{tunnel_id}"}}"#);
        if tx_delta > 0 {
            #[allow(clippy::cast_precision_loss)]
            self.meter
                .counter("tunnel.bytes.tx")
                .add(&labels, tx_delta as f64);
        }
        if rx_delta > 0 {
            #[allow(clippy::cast_precision_loss)]
            self.meter
                .counter("tunnel.bytes.rx")
                .add(&labels, rx_delta as f64);
        }
    }

    /// Try to claim the in-flight slot for this tunnel id. Returns
    /// `None` and sets up `AppError::Conflict` for the caller when a
    /// test is already in flight.
    fn acquire_in_flight(&self, id: Uuid) -> Result<InFlightGuard, AppError> {
        let mut guard = self
            .tests_in_flight
            .lock()
            .map_err(|_| AppError::Internal(anyhow::anyhow!("tests_in_flight mutex poisoned")))?;
        if !guard.insert(id) {
            return Err(AppError::Conflict(format!(
                "test already in progress for tunnel {id}"
            )));
        }
        Ok(InFlightGuard {
            set: self.tests_in_flight.clone(),
            id,
        })
    }

    /// Wait for a fresh handshake, then send a single exit probe.
    ///
    /// Returns the [`crate::tunnel::ExitInfo`] together with the
    /// measured probe latency in milliseconds. Errors map to
    /// [`AppError::UpstreamUnavailable`] for handshake/probe failures
    /// (the user can retry) and [`AppError::Internal`] for unexpected
    /// failures.
    async fn run_test_probe(
        &self,
        interface_name: &str,
    ) -> Result<(crate::tunnel::ExitInfo, u64), AppError> {
        self.await_fresh_handshake(interface_name, std::time::Duration::from_millis(3500))
            .await?;

        let started = std::time::Instant::now();
        let info = self
            .exit_probe
            .probe(interface_name)
            .await
            .map_err(|e| match e {
                ProbeError::Timeout(ms) => {
                    AppError::UpstreamUnavailable(format!("probe timed out after {ms} ms"))
                }
                ProbeError::Connect(msg) => {
                    AppError::UpstreamUnavailable(format!("probe connect failed: {msg}"))
                }
                ProbeError::Parse(msg) => {
                    AppError::UpstreamUnavailable(format!("probe parse failed: {msg}"))
                }
                ProbeError::Unsupported(msg) => {
                    AppError::Internal(anyhow::anyhow!("probe unsupported: {msg}"))
                }
            })?;
        let latency_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        Ok((info, latency_ms))
    }

    /// Block until the tunnel reports a recent handshake or `budget`
    /// elapses. Polls `tunnel_interface.get_stats` every 100 ms and
    /// considers a handshake "fresh" when its timestamp is within the
    /// last 5 seconds — long enough to forgive small clock drift but
    /// short enough that a stale value from a previous session won't
    /// pass.
    async fn await_fresh_handshake(
        &self,
        interface_name: &str,
        budget: std::time::Duration,
    ) -> Result<(), AppError> {
        let deadline = tokio::time::Instant::now() + budget;
        let freshness = chrono::Duration::seconds(5);
        loop {
            match self.tunnel_interface.get_stats(interface_name).await {
                Ok(Some(stats)) => {
                    if let Some(ts) = stats.last_handshake
                        && (chrono::Utc::now() - ts) <= freshness
                    {
                        return Ok(());
                    }
                }
                Ok(None) => {}
                Err(e) => {
                    return Err(AppError::Internal(anyhow::anyhow!(
                        "failed to read tunnel stats while waiting for handshake: {e}"
                    )));
                }
            }

            if tokio::time::Instant::now() >= deadline {
                return Err(AppError::UpstreamUnavailable(format!(
                    "tunnel handshake did not complete within {} ms",
                    budget.as_millis(),
                )));
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    }

    /// Core logic for tearing down a tunnel (no auth check).
    async fn tear_down_core(&self, id: Uuid, reason: &str) -> Result<(), AppError> {
        let tunnel = self.require_tunnel(id).await?;

        // No-op if already down.
        if tunnel.status == TunnelStatus::Down {
            return Ok(());
        }

        // Tear down and remove the tunnel interface.
        self.tunnel_interface
            .tear_down(&tunnel.interface_name)
            .await
            .map_err(AppError::Internal)?;
        self.tunnel_interface
            .remove(&tunnel.interface_name)
            .await
            .map_err(AppError::Internal)?;

        // Update status in the database.
        self.tunnels
            .update_status(&id.to_string(), "down")
            .await
            .map_err(AppError::Internal)?;

        // Publish domain event.
        self.events.publish(WardnetEvent::TunnelDown {
            tunnel_id: id,
            interface_name: tunnel.interface_name,
            reason: reason.to_owned(),
            timestamp: chrono::Utc::now(),
        });

        Ok(())
    }

    /// Measure one leg of a speed test: a single throughput download plus
    /// `speed_test_latency_samples` ICMP probes. `interface` is `Some(iface)`
    /// for the tunnel leg (binds the socket) or `None` for the direct/WAN
    /// baseline. A leg fails (whole test fails) if the download errors or
    /// every latency probe fails — a half-measurement is meaningless.
    async fn measure_leg(&self, interface: Option<&str>) -> Result<LegMeasurement, AppError> {
        let throughput = self
            .throughput_tester
            .download(interface)
            .await
            .map_err(|e| {
                AppError::UpstreamUnavailable(format!("throughput download failed: {e}"))
            })?;

        let samples_wanted = self.speed_test_latency_samples.max(1);
        let mut samples = Vec::with_capacity(samples_wanted as usize);
        for _ in 0..samples_wanted {
            if let Ok(rtt_ms) = self.latency_prober.probe(interface).await {
                samples.push(rtt_ms);
            }
        }
        if samples.is_empty() {
            return Err(AppError::UpstreamUnavailable(
                "all latency probes failed".to_owned(),
            ));
        }
        let (latency_ms, jitter_ms) = summarize_latency(&samples);
        Ok(LegMeasurement {
            throughput_mbps: throughput.mbps,
            latency_ms,
            jitter_ms,
        })
    }

    /// Body of the speed test job. Measures the direct (WAN) baseline, then
    /// the tunnel (bringing it up first if it was `Down`), persists one row
    /// with both legs, and restores the prior tunnel state before returning.
    async fn run_speed_test(&self, id: Uuid, reporter: &ProgressReporter) -> Result<(), AppError> {
        let tunnel = self.require_tunnel(id).await?;
        let was_down = tunnel.status == TunnelStatus::Down;
        let interface_name = tunnel.interface_name.clone();
        reporter.set_progress(5).await;

        // Direct (WAN) baseline first. The two legs run sequentially — both
        // ultimately traverse the same physical uplink, so measuring them at
        // once would split the pipe and corrupt both numbers.
        let direct = self.measure_leg(None).await;
        reporter.set_progress(40).await;

        // Bring the tunnel up if needed, wait for a fresh handshake, then
        // measure through it. Wrapped so a failure still hits the teardown
        // path below instead of leaking a tunnel we brought up.
        let through = async {
            if was_down {
                self.bring_up_core(id).await?;
                self.await_fresh_handshake(&interface_name, std::time::Duration::from_millis(3500))
                    .await?;
            }
            self.measure_leg(Some(&interface_name)).await
        }
        .await;
        reporter.set_progress(90).await;

        if was_down && let Err(e) = self.tear_down_core(id, "speed test completed").await {
            tracing::warn!(
                tunnel_id = %id,
                error = %e,
                "speed test: failed to tear down tunnel after run; leaving best-effort"
            );
        }

        let direct = direct?;
        let through = through?;

        let row = SpeedTestRow {
            id: Uuid::new_v4().to_string(),
            tunnel_id: id.to_string(),
            direct_throughput_mbps: direct.throughput_mbps,
            tunnel_throughput_mbps: through.throughput_mbps,
            direct_latency_ms: direct.latency_ms,
            tunnel_latency_ms: through.latency_ms,
            direct_jitter_ms: direct.jitter_ms,
            tunnel_jitter_ms: through.jitter_ms,
            tested_at: chrono::Utc::now().to_rfc3339(),
        };
        self.speed_test_repo
            .insert(&row)
            .await
            .map_err(AppError::Internal)?;
        reporter.set_progress(100).await;
        Ok(())
    }
}

/// RAII guard that releases the tunnel id from `tests_in_flight` when
/// dropped. Constructed by [`TunnelServiceImpl::acquire_in_flight`]
/// only when the slot was successfully claimed, so `Drop` can always
/// remove unconditionally.
struct InFlightGuard {
    set: Arc<std::sync::Mutex<HashSet<Uuid>>>,
    id: Uuid,
}

impl Drop for InFlightGuard {
    fn drop(&mut self) {
        if let Ok(mut guard) = self.set.lock() {
            guard.remove(&self.id);
        }
    }
}

#[async_trait]
impl TunnelService for TunnelServiceImpl {
    async fn import_tunnel(
        &self,
        req: CreateTunnelRequest,
    ) -> Result<CreateTunnelResponse, AppError> {
        auth_context::require_admin()?;

        // Parse the `WireGuard` .conf content.
        let config = wireguard_config::parse(&req.config)
            .map_err(|e| AppError::BadRequest(e.to_string()))?;

        let peer = config
            .peers
            .first()
            .ok_or_else(|| AppError::BadRequest("config has no peers".to_owned()))?;

        // Determine interface name.
        let idx = self
            .tunnels
            .next_interface_index()
            .await
            .map_err(AppError::Internal)?;
        let interface_name = format!("{}{idx}", crate::tunnel::interface::TUNNEL_INTERFACE_PREFIX);

        // Generate tunnel ID.
        let id = Uuid::new_v4();

        // Save private key to key store.
        self.keys
            .save_key(&id, &config.interface.private_key)
            .await
            .map_err(AppError::Internal)?;

        // Extract endpoint from the first peer.
        let endpoint = peer.endpoint.clone().unwrap_or_default();

        // Serialize sub-structures as JSON for storage.
        let address_json = serde_json::to_string(&config.interface.address)
            .map_err(|e| AppError::Internal(e.into()))?;
        let dns_json = serde_json::to_string(&config.interface.dns)
            .map_err(|e| AppError::Internal(e.into()))?;
        let peer_config_json =
            serde_json::to_string(peer).map_err(|e| AppError::Internal(e.into()))?;

        // Default to override-default-DNS when the imported config carries
        // a DNS server: tunneled devices route their DNS through wardnet
        // (so the ad-blocking filter still runs) and wardnet forwards
        // those queries to the tunnel's DNS server with `SO_BINDTODEVICE`
        // to avoid leaking via the ISP's default route. Tunnels with no
        // configured DNS default to false — there is nothing to override
        // and the system-wide upstream pool is the right choice.
        let override_default_dns = !config.interface.dns.is_empty();

        let endpoint_resolved_at_dt: Option<chrono::DateTime<chrono::Utc>> =
            if req.server_selector.is_some() {
                Some(chrono::Utc::now())
            } else {
                None
            };
        let server_selector_country = req.server_selector.as_ref().map(|s| s.country.clone());

        let row = TunnelRow {
            id: id.to_string(),
            label: req.label.clone(),
            country_code: req.country_code.clone(),
            provider: req.provider.clone(),
            interface_name: interface_name.clone(),
            endpoint: endpoint.clone(),
            status: "down".to_owned(),
            address: address_json,
            dns: dns_json,
            peer_config: peer_config_json,
            listen_port: config.interface.listen_port,
            override_default_dns,
            server_selector_country,
            resolved_server_name: req.resolved_server_name.clone(),
            endpoint_resolved_at: endpoint_resolved_at_dt
                .as_ref()
                .map(chrono::DateTime::to_rfc3339),
        };

        self.tunnels
            .insert(&row)
            .await
            .map_err(AppError::Internal)?;

        let now = chrono::Utc::now();
        let tunnel = Tunnel {
            id,
            label: req.label,
            country_code: req.country_code,
            provider: req.provider,
            interface_name,
            endpoint,
            status: TunnelStatus::Down,
            last_handshake: None,
            bytes_tx: 0,
            bytes_rx: 0,
            created_at: now,
            override_default_dns,
            server_selector: req.server_selector,
            resolved_server_name: req.resolved_server_name,
            endpoint_resolved_at: endpoint_resolved_at_dt,
        };

        Ok(CreateTunnelResponse {
            tunnel,
            message: "tunnel imported successfully".to_owned(),
        })
    }

    async fn list_tunnels(&self) -> Result<ListTunnelsResponse, AppError> {
        auth_context::require_authenticated()?;

        let tunnels = self.tunnels.find_all().await.map_err(AppError::Internal)?;
        Ok(ListTunnelsResponse { tunnels })
    }

    async fn get_tunnel(&self, id: Uuid) -> Result<Tunnel, AppError> {
        auth_context::require_admin()?;

        self.require_tunnel(id).await
    }

    async fn test_tunnel(&self, id: Uuid) -> Result<TunnelTestResult, AppError> {
        auth_context::require_admin()?;

        // Claim the in-flight slot before any state changes so a
        // double-click is rejected with 409 instead of starting a
        // second concurrent bring-up.
        let _guard = self.acquire_in_flight(id)?;

        let tunnel = self.require_tunnel(id).await?;
        let was_up_before = matches!(
            tunnel.status,
            TunnelStatus::Up | TunnelStatus::Connecting | TunnelStatus::Reconnecting,
        );
        let interface_name = tunnel.interface_name.clone();

        // Bring the tunnel up if needed. `bring_up_core` is idempotent
        // for the non-`Down` cases.
        if !was_up_before {
            self.bring_up_core(id).await?;
        }

        // Drive the probe to completion, capturing the result so we
        // always tear down before returning when we brought the tunnel
        // up ourselves.
        let probe_outcome = self.run_test_probe(&interface_name).await;

        if !was_up_before && let Err(e) = self.tear_down_core(id, "test completed").await {
            tracing::warn!(
                tunnel_id = %id,
                error = %e,
                "tunnel test: failed to tear down tunnel after probe; leaving best-effort"
            );
        }

        let (exit_info, latency_ms) = probe_outcome?;

        Ok(TunnelTestResult {
            tunnel_id: id,
            exit_ip: exit_info.ip,
            country_code: exit_info.country_code,
            latency_ms,
        })
    }

    async fn list_tunnel_devices(&self, id: Uuid) -> Result<TunnelDevicesResponse, AppError> {
        auth_context::require_admin()?;
        self.require_tunnel(id).await?;
        let devices = self
            .devices
            .find_devices_for_tunnel(&id.to_string())
            .await
            .map_err(AppError::Internal)?;
        Ok(TunnelDevicesResponse { devices })
    }

    async fn set_dns_override(&self, id: Uuid, value: bool) -> Result<Tunnel, AppError> {
        auth_context::require_admin()?;
        self.require_tunnel(id).await?;
        self.tunnels
            .update_dns_override(&id.to_string(), value)
            .await
            .map_err(AppError::Internal)?;
        // Re-emit a routing-rules-changed signal so the DNS-upstream
        // snapshot rebuild picks up the new value for already-applied
        // device rules. Devices keep their applied rule; only the
        // upstream selection (filter on/off, tunnel vs default upstream)
        // is affected.
        self.events.publish(WardnetEvent::TunnelDnsOverrideChanged {
            tunnel_id: id,
            timestamp: chrono::Utc::now(),
        });
        let tunnel = self.require_tunnel(id).await?;
        Ok(tunnel)
    }

    async fn rebuild(&self, id: Uuid) -> Result<(), AppError> {
        auth_context::require_admin()?;
        let _guard = self.acquire_in_flight(id)?;
        self.tear_down_core(id, "admin rebuild").await?;
        if let Err(e) = self.bring_up_core(id).await {
            tracing::error!(tunnel_id = %id, error = %e, "rebuild: tear-down succeeded but bring-up failed - tunnel is down");
            return Err(e);
        }
        Ok(())
    }

    async fn bring_up(&self, id: Uuid) -> Result<(), AppError> {
        auth_context::require_admin()?;
        self.bring_up_core(id).await
    }

    async fn tear_down(&self, id: Uuid, reason: &str) -> Result<(), AppError> {
        auth_context::require_admin()?;
        self.tear_down_core(id, reason).await
    }

    async fn bring_up_internal(&self, id: Uuid) -> Result<(), AppError> {
        self.bring_up_core(id).await
    }

    async fn tear_down_internal(&self, id: Uuid, reason: &str) -> Result<(), AppError> {
        self.tear_down_core(id, reason).await
    }

    async fn delete_tunnel(&self, id: Uuid) -> Result<DeleteTunnelResponse, AppError> {
        auth_context::require_admin()?;

        let tunnel = self.require_tunnel(id).await?;

        // Switch all routing rules targeting this tunnel to Direct so devices
        // don't lose connectivity.
        let now = chrono::Utc::now().to_rfc3339();
        let switched = self
            .devices
            .switch_tunnel_rules_to_direct(&id.to_string(), &now)
            .await
            .map_err(AppError::Internal)?;

        if !switched.is_empty() {
            tracing::info!(
                tunnel_id = %id,
                device_count = switched.len(),
                "switched devices from deleted tunnel to direct routing"
            );
            // Emit routing rule change events so the routing listener updates
            // kernel state for each affected device.
            for device_id_str in &switched {
                if let Ok(device_id) = device_id_str.parse::<Uuid>() {
                    self.events.publish(WardnetEvent::RoutingRuleChanged {
                        device_id,
                        target: wardnet_common::routing::RoutingTarget::Direct,
                        previous_target: Some(wardnet_common::routing::RoutingTarget::Tunnel {
                            tunnel_id: id,
                        }),
                        changed_by: wardnet_common::routing::RuleCreator::Admin,
                        timestamp: chrono::Utc::now(),
                    });
                }
            }
        }

        // The global default policy may also point at this tunnel. Reset it
        // to "direct" in the same step, mirroring the explicit-rule switch
        // above, so `Default`-ruled devices degrade the same way at the
        // same time — instead of the config keeping a dangling tunnel UUID
        // that silently resolves to direct on each later lookup.
        //
        // The stored policy is matched through `from_default_policy` — the
        // same interpreter the routing engine resolves with — not raw string
        // equality: `set_default_policy` stores the admin-supplied string
        // verbatim, and the UUID parser accepts non-canonical forms
        // (uppercase, braced, un-hyphenated) that would defeat an exact
        // compare while still routing through this tunnel.
        let default_policy = self
            .system_config
            .get_default_policy()
            .await
            .map_err(AppError::Internal)?;
        let policy_targets_tunnel = default_policy.as_deref().is_some_and(|p| {
            matches!(
                wardnet_common::routing::RoutingTarget::from_default_policy(p),
                wardnet_common::routing::RoutingTarget::Tunnel { tunnel_id } if tunnel_id == id
            )
        });
        if policy_targets_tunnel {
            // Conditional on the exact stored value so a concurrent
            // `set_default_policy` write is never clobbered: if it lands
            // in between, the compare-and-set misses and that writer's own
            // event covers the change.
            let reset = self
                .system_config
                .reset_default_policy_from(default_policy.as_deref().unwrap_or_default())
                .await
                .map_err(AppError::Internal)?;
            if reset {
                tracing::info!(
                    tunnel_id = %id,
                    "deleted tunnel {id} was the default routing policy, reset policy to direct"
                );
                // Announce the reset so the routing engine re-resolves
                // `Default`-ruled devices immediately and the Network-Zone
                // enforcer (#736) re-clamps them, rather than each device
                // degrading lazily on its next resolve.
                self.events.publish(WardnetEvent::DefaultPolicyChanged {
                    policy: "direct".to_owned(),
                    timestamp: chrono::Utc::now(),
                });
            }
        }

        // If the kernel interface is configured, tear it down first.
        if tunnel.status != TunnelStatus::Down {
            self.tear_down_core(id, "tunnel deleted").await?;
        }

        // Delete private key from key store.
        self.keys
            .delete_key(&id)
            .await
            .map_err(AppError::Internal)?;

        // Delete from database.
        self.tunnels
            .delete(&id.to_string())
            .await
            .map_err(AppError::Internal)?;

        Ok(DeleteTunnelResponse {
            message: format!("tunnel {} deleted", tunnel.label),
        })
    }

    async fn restore_tunnels(&self) -> Result<(), AppError> {
        // Documented exception to the auth-guard rule (.agents/auth.md §Rules #2,
        // category (a): startup/restore): runs during boot, before any admin
        // session can exist, to rehydrate tunnel state from the database.
        let tunnels = self.tunnels.find_all().await.map_err(AppError::Internal)?;

        tracing::info!(
            count = tunnels.len(),
            "restored tunnel configurations from database"
        );
        Ok(())
    }

    async fn collect_stats(&self) -> Result<(), AppError> {
        let all_tunnels = self.tunnels.find_all().await.map_err(AppError::Internal)?;
        // Stats apply to any tunnel whose kernel iface is configured —
        // `Up`, `Connecting`, and `Reconnecting` all qualify. `Down`
        // tunnels have no iface to read.
        let active_tunnels: Vec<_> = all_tunnels
            .into_iter()
            .filter(|t| t.status != wardnet_common::tunnel::TunnelStatus::Down)
            .collect();

        for tunnel in active_tunnels {
            let stats = match self
                .tunnel_interface
                .get_stats(&tunnel.interface_name)
                .await
            {
                Ok(Some(s)) => s,
                Ok(None) => continue,
                Err(e) => {
                    tracing::error!(
                        interface = %tunnel.interface_name,
                        error = %e,
                        "stats loop: failed to get stats for {}: {e}", tunnel.interface_name
                    );
                    continue;
                }
            };

            let last_handshake_str = stats.last_handshake.map(|ts| ts.to_rfc3339());

            if let Err(e) = self
                .tunnels
                .update_stats(
                    &tunnel.id.to_string(),
                    stats.bytes_tx.cast_signed(),
                    stats.bytes_rx.cast_signed(),
                    last_handshake_str.as_deref(),
                )
                .await
            {
                tracing::error!(
                    tunnel_id = %tunnel.id,
                    error = %e,
                    "stats loop: failed to update stats in database for tunnel {}: {e}", tunnel.id
                );
                continue;
            }

            self.record_byte_deltas(tunnel.id, stats.bytes_tx, stats.bytes_rx)
                .await;

            self.events
                .publish(wardnet_common::event::WardnetEvent::TunnelStatsUpdated {
                    tunnel_id: tunnel.id,
                    status: tunnel.status,
                    bytes_tx: stats.bytes_tx,
                    bytes_rx: stats.bytes_rx,
                    last_handshake: stats.last_handshake,
                    timestamp: chrono::Utc::now(),
                });
        }
        Ok(())
    }

    async fn run_health_check(&self) -> Result<(), AppError> {
        let stale_threshold = chrono::Duration::minutes(3);
        // After this much time without a handshake while in `Reconnecting`,
        // the iface is present but the peer hasn't replied — recreate to
        // force fresh DNS resolution and a clean iface state. 5x the stale
        // threshold gives the peer plenty of room to come back on its own.
        let stuck_recovery_threshold = chrono::Duration::minutes(15);
        let now = chrono::Utc::now();
        let all_tunnels = self.tunnels.find_all().await.map_err(AppError::Internal)?;
        let active_tunnels = self.reconcile_iface_presence(all_tunnels, now).await?;

        // IDs of tunnels that are stuck in `Reconnecting` and need an active
        // recreate after the per-tunnel match below has had its chance to
        // flip them back to `Up`.
        let mut to_recreate: Vec<Uuid> = Vec::new();

        for tunnel in active_tunnels {
            // A handshake is "fresh" if we have one and it's within the
            // stale threshold. `None` is treated as stale.
            let fresh_handshake = tunnel
                .last_handshake
                .is_some_and(|ts| (now - ts) <= stale_threshold);

            match (tunnel.status, fresh_handshake) {
                // Connecting → Up: first handshake observed.
                // Reconnecting → Up: peer started replying again.
                (
                    wardnet_common::tunnel::TunnelStatus::Connecting
                    | wardnet_common::tunnel::TunnelStatus::Reconnecting,
                    true,
                ) => {
                    if let Err(e) = self
                        .tunnels
                        .update_status(&tunnel.id.to_string(), "up")
                        .await
                    {
                        tracing::error!(
                            tunnel_id = %tunnel.id,
                            error = %e,
                            "health check: failed to mark tunnel up for {}: {e}",
                            tunnel.interface_name
                        );
                        continue;
                    }
                    tracing::info!(
                        tunnel_id = %tunnel.id,
                        interface = %tunnel.interface_name,
                        previous_status = ?tunnel.status,
                        "health check: tunnel is now up (handshake observed)"
                    );
                    self.events.publish(WardnetEvent::TunnelUp {
                        tunnel_id: tunnel.id,
                        interface_name: tunnel.interface_name,
                        endpoint: tunnel.endpoint,
                        timestamp: now,
                    });
                }
                // Up → Reconnecting: handshake gone stale or absent.
                (wardnet_common::tunnel::TunnelStatus::Up, false) => {
                    if let Err(e) = self
                        .tunnels
                        .update_status(&tunnel.id.to_string(), "reconnecting")
                        .await
                    {
                        tracing::error!(
                            tunnel_id = %tunnel.id,
                            error = %e,
                            "health check: failed to mark tunnel reconnecting for {}: {e}",
                            tunnel.interface_name
                        );
                        continue;
                    }
                    tracing::warn!(
                        tunnel_id = %tunnel.id,
                        interface = %tunnel.interface_name,
                        last_handshake = ?tunnel.last_handshake,
                        "health check: tunnel is reconnecting (handshake stale or absent)"
                    );
                    self.events.publish(WardnetEvent::TunnelReconnecting {
                        tunnel_id: tunnel.id,
                        interface_name: tunnel.interface_name,
                        last_handshake: tunnel.last_handshake,
                        timestamp: now,
                    });
                }
                // Reconnecting and the peer still hasn't replied. If we've
                // been stuck this long, the iface itself may be wedged or
                // the peer DNS may have rotated — schedule a recreate.
                (wardnet_common::tunnel::TunnelStatus::Reconnecting, false) => {
                    let stuck = tunnel
                        .last_handshake
                        .is_none_or(|ts| (now - ts) > stuck_recovery_threshold);
                    if stuck {
                        to_recreate.push(tunnel.id);
                    }
                }
                // No-op: Up + fresh, Connecting + still no handshake.
                _ => {}
            }
        }

        self.recreate_stuck_tunnels(to_recreate).await;

        Ok(())
    }

    async fn probe_latencies(&self) -> Result<(), AppError> {
        let all_tunnels = self.tunnels.find_all().await.map_err(AppError::Internal)?;
        let active_tunnels: Vec<_> = all_tunnels
            .into_iter()
            .filter(|t| t.status != wardnet_common::tunnel::TunnelStatus::Down)
            .collect();

        for tunnel in active_tunnels {
            match self
                .latency_prober
                .probe(Some(&tunnel.interface_name))
                .await
            {
                Ok(rtt_ms) => {
                    let labels = format!(r#"{{"tunnel_id":"{}"}}"#, tunnel.id);
                    #[allow(clippy::cast_precision_loss)]
                    self.meter
                        .gauge("tunnel.latency.rtt_ms")
                        .set(&labels, rtt_ms as f64);
                }
                Err(LatencyProbeError::Unsupported(msg)) => {
                    // Platform-level: log once-per-loop and skip the rest of the
                    // tunnels — every iface would fail the same way.
                    tracing::debug!(
                        error = %msg,
                        "latency probe unsupported on this platform: {msg}"
                    );
                    return Ok(());
                }
                Err(e) => {
                    tracing::warn!(
                        tunnel_id = %tunnel.id,
                        interface = %tunnel.interface_name,
                        error = %e,
                        "latency probe failed for {}: {e}", tunnel.interface_name
                    );
                }
            }
        }
        Ok(())
    }

    async fn start_speed_test(
        self: Arc<Self>,
        id: Uuid,
    ) -> Result<JobDispatchedResponse, AppError> {
        auth_context::require_admin()?;

        // Claim the in-flight slot synchronously so a concurrent run (or an
        // overlapping `test_tunnel`) is rejected with 409 immediately, before
        // any state changes. The guard is then moved into the job closure so
        // the slot is held for the full job duration, not just this handler.
        let guard = self.acquire_in_flight(id)?;

        // Fail fast if the tunnel doesn't exist, rather than 202-then-fail.
        self.require_tunnel(id).await?;

        let me = Arc::clone(&self);
        let job_id = self
            .jobs
            .dispatch(JobKind::SpeedTest, move |reporter| async move {
                let _guard = guard;
                me.run_speed_test(id, &reporter)
                    .await
                    .map_err(|e| anyhow::anyhow!("{e}"))
            })
            .await;
        Ok(JobDispatchedResponse { job_id })
    }

    async fn list_speed_tests(&self, id: Uuid) -> Result<TunnelSpeedTestHistoryResponse, AppError> {
        auth_context::require_admin()?;
        self.require_tunnel(id).await?;
        let results = self
            .speed_test_repo
            .find_recent(&id.to_string(), SPEED_TEST_HISTORY_LIMIT)
            .await
            .map_err(AppError::Internal)?;
        Ok(TunnelSpeedTestHistoryResponse { results })
    }
}
