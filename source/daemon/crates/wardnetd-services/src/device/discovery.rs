use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::Instant;

use arc_swap::ArcSwap;
use async_trait::async_trait;
use tokio::sync::RwLock;
use uuid::Uuid;
use wardnet_common::device::{Device, DeviceConnectionMode, DeviceType};
use wardnet_common::event::WardnetEvent;

use wardnet_common::auth::AuthContext;

use crate::auth_context;
use crate::device::hostname_resolver::HostnameResolver;
use crate::device::packet_capture::ObservedDevice;
use crate::error::AppError;
use crate::event::EventPublisher;
use wardnetd_data::oui;
use wardnetd_data::repository::DeviceRepository;
use wardnetd_data::repository::DhcpRepository;
use wardnetd_data::repository::NetworkZoneRepository;
use wardnetd_data::repository::SystemConfigRepository;
use wardnetd_data::repository::device::DeviceRow;

/// In-memory tracking state for a device.
struct DeviceMemoryState {
    device_id: Uuid,
    mac: String,
    last_ip: String,
    last_seen: Instant,
    gone: bool,
}

/// How far back the IP-flap detector looks.
const FLAP_WINDOW: std::time::Duration = std::time::Duration::from_mins(1);

/// How many distinct IPs a single MAC may claim within [`FLAP_WINDOW`] before
/// its observations stop being trusted.
///
/// A real device changes address rarely, and one at a time — a DHCP move is a
/// single transition. The MAC behind issue #886 claimed four addresses inside
/// one second: a powerline bridge answering ARP for hosts across its segment.
/// Three is comfortably above honest churn and well below that.
const FLAP_MAX_DISTINCT_IPS: usize = 3;

/// Recent distinct IPs claimed by one MAC, for the flap detector (issue #886).
#[derive(Default)]
struct IpFlapHistory {
    /// Distinct IPs seen inside the window, each with the time it was last
    /// observed. Entries age out, so a MAC that settles becomes trusted again
    /// on its own.
    ips: VecDeque<(String, Instant)>,
    /// Whether the current flapping episode has already been warned about, so a
    /// storm of observations doesn't become a storm of log lines.
    warned: bool,
}

/// Result of processing a device observation.
#[derive(Debug)]
pub enum ObservationResult {
    /// A brand new device was registered.
    NewDevice {
        device_id: Uuid,
        manufacturer: Option<String>,
        device_type: DeviceType,
    },
    /// A known device changed its IP address.
    IpChanged { device_id: Uuid, old_ip: String },
    /// A known device was seen again (no state change).
    Seen(Uuid),
    /// A previously departed device has returned.
    Reappeared(Uuid),
    /// The observation was filtered out (e.g. IP outside LAN subnet).
    Ignored,
}

/// Device discovery and lifecycle management.
///
/// Processes packet capture observations to detect new devices, track IP changes,
/// detect departures, and resolve hostnames. Separate from [`DeviceService`] which
/// handles self-service user flows.
///
/// [`DeviceService`]: crate::DeviceService
#[async_trait]
pub trait DeviceDiscoveryService: Send + Sync {
    /// Restore device state from the database on startup.
    ///
    /// Loads all devices and populates the in-memory map. All devices are marked
    /// as gone since we do not know if they are present until we see packets.
    async fn restore_devices(&self) -> Result<(), AppError>;

    /// Rebuild the cached set of trusted subnets (the LAN `/24` plus every
    /// configured Network-Zone subnet).
    ///
    /// Observations from IPs outside every trusted subnet are ignored, so this
    /// set must be refreshed whenever a zone's subnet changes — otherwise a
    /// device DHCP-addressed inside a zone subnet outside the base LAN `/24`
    /// would have all its observations silently dropped, freezing its
    /// presence and starving zone enforcement of `DeviceIpChanged` events.
    ///
    /// Called once at startup and on every [`WardnetEvent::NetworkZoneChanged`].
    /// A zone whose subnet CIDR fails to parse is logged and skipped rather than
    /// failing the whole rebuild.
    async fn rebuild_trusted_subnets(&self) -> Result<(), AppError>;

    /// Process a device observation from packet capture.
    async fn process_observation(
        &self,
        obs: &ObservedDevice,
    ) -> Result<ObservationResult, AppError>;

    /// Flush batched `last_seen` updates to the database.
    ///
    /// Returns the number of devices updated.
    async fn flush_last_seen(&self) -> Result<u64, AppError>;

    /// Find departed devices and emit `DeviceGone` events.
    ///
    /// Returns the IDs of newly departed devices.
    async fn scan_departures(&self, timeout_secs: u64) -> Result<Vec<Uuid>, AppError>;

    /// Resolve a hostname for a device and update the registry.
    ///
    /// Precedence: DHCP option-12 (active lease's `hostname`) wins when
    /// non-empty; otherwise falls back to reverse DNS via the configured
    /// [`HostnameResolver`]. The admin-assigned `Device.name` is a separate
    /// column and overrides both at display time, so this method never
    /// touches it. Tolerates a missing device (e.g. a lease event arriving
    /// before packet capture has registered the MAC).
    async fn resolve_hostname(&self, mac: &str, ip: &str) -> Result<(), AppError>;

    /// Get all devices (admin view).
    async fn get_all_devices(&self) -> Result<Vec<Device>, AppError>;

    /// Get a single device by ID.
    async fn get_device_by_id(&self, id: Uuid) -> Result<Device, AppError>;

    /// Update a device's name and/or type (admin operation).
    async fn update_device(
        &self,
        id: Uuid,
        name: Option<&str>,
        device_type: Option<DeviceType>,
    ) -> Result<Device, AppError>;

    /// Process an observation sourced from an inbound `WireGuard` handshake for
    /// an already-known device (identified by `device_id`, not MAC — the
    /// `Device` row always pre-exists for a remote-access grant). Resolves the
    /// device's `mac` internally, then runs it through the same
    /// reappear/IP-changed/just-seen state machine
    /// [`process_observation`](Self::process_observation) uses for LAN
    /// observations, but skips the `lan_subnet` filter (peer IPs live in the
    /// inbound WG subnet) and skips the OUI manufacturer/device-type re-guess
    /// (already set from LAN history, or `Unknown` if never guessed — never
    /// overwritten here). Sets `connection_mode = Remote` on write. If
    /// `device_id` does not resolve to a device (should be impossible — the FK
    /// guarantees it), returns [`AppError::Internal`], never a "new device"
    /// path.
    async fn process_peer_observation(
        &self,
        device_id: Uuid,
        ip: &str,
    ) -> Result<ObservationResult, AppError>;

    /// Refresh a tracked device's liveness timestamp without a DB write or event
    /// publish — called every poll tick a `WireGuard` peer stays fresh, so the
    /// shared in-memory entry doesn't go stale under the unrelated LAN-departure
    /// sweep (`scan_departures`) merely because the peer path isn't re-observing
    /// via the full `process_peer_observation` write path on every tick.
    async fn touch_peer_presence(&self, device_id: Uuid) -> Result<(), AppError>;

    /// Explicitly mark a device gone due to `WireGuard` handshake staleness
    /// (`device_id`-keyed, resolves mac internally). Mirrors what
    /// [`scan_departures`](Self::scan_departures) does for a single device:
    /// flips the in-memory tracking entry's `gone = true` and publishes
    /// [`WardnetEvent::DeviceGone`]. Kept separate from `scan_departures`'s
    /// timeout-based sweep because `WireGuard` handshake staleness and
    /// ARP-timeout liveness are different signals on different timers. Does not
    /// touch `connection_mode` or delete anything — the row persists,
    /// enforcement is torn down for that IP until the next observation. No-op
    /// (returns `Ok(None)`) if the device is not currently tracked as present.
    ///
    /// `timeout` gates the flip on `entry.last_seen.elapsed() > timeout`
    /// (mirroring `scan_departures`'s own gate): the in-memory `last_seen` is a
    /// *shared* signal bumped by BOTH the LAN and `WireGuard` observation paths,
    /// so a device kept alive by fresh LAN traffic must NOT be torn down just
    /// because ITS `WireGuard` handshake signal went stale. The caller passes the
    /// same staleness window it used to decide the handshake was stale.
    async fn mark_peer_gone(
        &self,
        device_id: Uuid,
        timeout: std::time::Duration,
    ) -> Result<Option<Uuid>, AppError>;
}

/// Default implementation of [`DeviceDiscoveryService`].
pub struct DeviceDiscoveryServiceImpl {
    devices: Arc<dyn DeviceRepository>,
    zones: Arc<dyn NetworkZoneRepository>,
    dhcp: Arc<dyn DhcpRepository>,
    /// Read-only: the `quarantine_new_devices` toggle (#738). Gates the
    /// `NewDeviceQuarantined` admin-notification event on truly-new devices.
    system_config: Arc<dyn SystemConfigRepository>,
    events: Arc<dyn EventPublisher>,
    resolver: Arc<dyn HostnameResolver>,
    /// Base LAN subnet — the Pi's own `/24`. The immutable seed of the trusted
    /// subnet set, re-added on every [`Self::rebuild_trusted_subnets`].
    lan_subnet: ipnetwork::Ipv4Network,
    /// Cached set of trusted subnets: the LAN `/24` plus every configured
    /// Network-Zone subnet. Only observations from IPs inside one of these are
    /// processed — this filters out return traffic from the internet (remote
    /// IPs arriving with the router's MAC on the Ethernet frame).
    ///
    /// Held behind an `ArcSwap` so the per-observation hot path reads a
    /// lock-free snapshot while zone-change events rebuild it in place, rather
    /// than issuing a DB query per observation.
    trusted_subnets: ArcSwap<Vec<ipnetwork::Ipv4Network>>,
    /// Wardnet's own LAN IP. Never a device, whatever ARP says (issue #886).
    lan_ip: std::net::Ipv4Addr,
    /// Per-MAC recent distinct-IP history, keyed by MAC. Feeds the flap detector
    /// that stops trusting a MAC claiming many addresses at once (issue #886).
    ip_history: Arc<RwLock<HashMap<String, IpFlapHistory>>>,
    state: Arc<RwLock<HashMap<String, DeviceMemoryState>>>,
    /// Per-mac locks serializing the full observe-then-persist sequence for a
    /// single device across the LAN and `WireGuard` observation paths, so their DB
    /// writes can't land out of order relative to each other. Keyed by mac,
    /// entries created lazily; never removed (device count is small/bounded by
    /// hardware on a LAN, so this doesn't need eviction).
    device_locks: tokio::sync::Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
}

impl DeviceDiscoveryServiceImpl {
    /// Create a new discovery service with the given dependencies.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        devices: Arc<dyn DeviceRepository>,
        zones: Arc<dyn NetworkZoneRepository>,
        dhcp: Arc<dyn DhcpRepository>,
        system_config: Arc<dyn SystemConfigRepository>,
        events: Arc<dyn EventPublisher>,
        resolver: Arc<dyn HostnameResolver>,
        lan_subnet: ipnetwork::Ipv4Network,
        lan_ip: std::net::Ipv4Addr,
    ) -> Self {
        Self {
            devices,
            zones,
            dhcp,
            system_config,
            events,
            resolver,
            lan_subnet,
            // Seeded with just the LAN subnet; zone subnets are folded in by the
            // startup + event-driven `rebuild_trusted_subnets` calls.
            trusted_subnets: ArcSwap::from_pointee(vec![lan_subnet]),
            lan_ip,
            state: Arc::new(RwLock::new(HashMap::new())),
            ip_history: Arc::new(RwLock::new(HashMap::new())),
            device_locks: tokio::sync::Mutex::new(HashMap::new()),
        }
    }

    /// Record `ip` against `mac` and report whether that MAC is currently
    /// flapping — claiming more than [`FLAP_MAX_DISTINCT_IPS`] distinct
    /// addresses inside [`FLAP_WINDOW`].
    ///
    /// A device changes address rarely and one at a time. A MAC cycling through
    /// several addresses in seconds is not a device moving — it is something
    /// answering ARP on behalf of other hosts (the powerline bridge in issue
    /// #886, which claimed four addresses in one second, one of them the Pi's
    /// own) or spoofing outright. Either way it is not telling the truth about
    /// any of those addresses, so we stop believing it until it settles.
    ///
    /// Entries age out of the window, so a MAC recovers on its own without any
    /// operator action.
    async fn is_flapping(&self, mac: &str, ip: &str) -> bool {
        let mut history = self.ip_history.write().await;
        let entry = history.entry(mac.to_owned()).or_default();

        let now = Instant::now();
        entry.ips.retain(|(_, seen)| now - *seen < FLAP_WINDOW);
        if let Some((_, seen)) = entry.ips.iter_mut().find(|(known, _)| known == ip) {
            *seen = now;
        } else {
            entry.ips.push_back((ip.to_owned(), now));
        }

        let flapping = entry.ips.len() > FLAP_MAX_DISTINCT_IPS;
        if flapping && !entry.warned {
            entry.warned = true;
            let claimed: Vec<&str> = entry.ips.iter().map(|(ip, _)| ip.as_str()).collect();
            tracing::warn!(
                mac,
                claimed = ?claimed,
                window_secs = FLAP_WINDOW.as_secs(),
                "device discovery: {mac} claimed {claimed:?} within {window_secs}s — ignoring \
                 its observations until it settles; something is answering ARP for addresses \
                 it does not own (issue #886)",
                claimed = claimed,
                window_secs = FLAP_WINDOW.as_secs(),
            );
        } else if !flapping {
            entry.warned = false;
        }
        flapping
    }

    /// Refresh a tracked, present MAC's in-memory `last_seen` without touching
    /// its IP or the database.
    ///
    /// Called for observations we deliberately ignore (own-IP claims, flapping
    /// MACs): the observation is a lie about the *address*, but it still proves
    /// the MAC is on the wire. Without this, a persistently-lying MAC stops
    /// refreshing `last_seen`, the departure sweep marks it gone after the
    /// timeout, and `DeviceGone` tears down its zone enforcement fail-open
    /// while the device is still present (issue #886).
    async fn touch_presence(&self, mac: &str) {
        let mut state = self.state.write().await;
        if let Some(entry) = state.get_mut(mac)
            && !entry.gone
        {
            entry.last_seen = Instant::now();
        }
    }

    /// Fetch (or lazily create) the per-mac serialization lock. The returned
    /// `Arc<Mutex<()>>` is then `lock_owned().await`-ed by the caller to hold a
    /// guard across the whole observe-then-persist sequence.
    async fn lock_for_mac(&self, mac: &str) -> Arc<tokio::sync::Mutex<()>> {
        let mut locks = self.device_locks.lock().await;
        locks
            .entry(mac.to_owned())
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
            .clone()
    }

    /// Clear a departed device's stored `last_ip` so its row can no longer be
    /// resolved by a live device's source IP in `find_by_ip` (the IP-keyed
    /// self-service auth path) once DHCP recycles the departed address.
    ///
    /// Taken under the per-mac lock and gated on the device still being `gone`:
    /// a device that reappears in the same instant as the departure sweep has
    /// already had the live IP written to its row by its own observation, so we
    /// must not clobber that back to empty. Holding the same lock the
    /// observation path holds serialises the two; the re-check closes the window
    /// between the sweep flipping `gone` and this acquiring the lock.
    async fn clear_departed_ip(&self, device_id: Uuid, mac: &str) {
        let mac_lock = self.lock_for_mac(mac).await;
        let _guard = mac_lock.lock_owned().await;

        let still_gone = {
            let state = self.state.read().await;
            state.get(mac).is_none_or(|entry| entry.gone)
        };
        if !still_gone {
            return;
        }

        if let Err(e) = self.devices.clear_last_ip(&device_id.to_string()).await {
            tracing::warn!(
                error = %e,
                device_id = %device_id,
                mac = %mac,
                "device discovery: failed to clear departed device's last_ip"
            );
        }
    }

    /// Handle a device not found in the in-memory map.
    ///
    /// Checks the database for a previous record, and either reappears the device
    /// or inserts a brand new one.
    async fn handle_unknown_mac(
        &self,
        obs: &ObservedDevice,
    ) -> Result<ObservationResult, AppError> {
        // Check if device exists in DB (from a previous daemon run).
        let db_device = self
            .devices
            .find_by_mac(&obs.mac)
            .await
            .map_err(AppError::Internal)?;

        if let Some(device) = db_device {
            return self
                .reappear_device(device.id, &obs.mac, obs, device.hostname)
                .await;
        }

        self.insert_new_device(obs).await
    }

    /// Re-register a returning device in the in-memory map and DB.
    async fn reappear_device(
        &self,
        device_id: Uuid,
        mac: &str,
        obs: &ObservedDevice,
        hostname: Option<String>,
    ) -> Result<ObservationResult, AppError> {
        {
            let mut state = self.state.write().await;
            state.insert(
                obs.mac.clone(),
                DeviceMemoryState {
                    device_id,
                    mac: obs.mac.clone(),
                    last_ip: obs.ip.clone(),
                    last_seen: Instant::now(),
                    gone: false,
                },
            );
        }

        let now = chrono::Utc::now().to_rfc3339();
        self.devices
            .update_last_seen_and_ip(
                &device_id.to_string(),
                &obs.ip,
                &now,
                DeviceConnectionMode::Lan,
            )
            .await
            .map_err(AppError::Internal)?;

        self.events.publish(WardnetEvent::DeviceDiscovered {
            device_id,
            mac: mac.to_owned(),
            ip: obs.ip.clone(),
            hostname,
            timestamp: chrono::Utc::now(),
        });

        Ok(ObservationResult::Reappeared(device_id))
    }

    /// Insert a truly new device into the DB and in-memory map.
    async fn insert_new_device(&self, obs: &ObservedDevice) -> Result<ObservationResult, AppError> {
        let device_id = Uuid::new_v4();
        let manufacturer = oui::lookup_manufacturer(&obs.mac).map(String::from);
        let device_type = manufacturer
            .as_deref()
            .map_or(DeviceType::Unknown, oui::guess_device_type);
        let now = chrono::Utc::now().to_rfc3339();

        // Freshly-discovered devices land in the default-for-new zone (Guest).
        // Membership is sticky: it is set once here and never re-resolved.
        let zone = self
            .zones
            .find_default_for_new()
            .await
            .map_err(AppError::Internal)?;

        let row = DeviceRow {
            id: device_id.to_string(),
            mac: obs.mac.clone(),
            hostname: None,
            manufacturer: manufacturer.clone(),
            device_type: serde_json::to_string(&device_type)
                .map_or_else(|_| "unknown".to_owned(), |s| s.trim_matches('"').to_owned()),
            first_seen: now.clone(),
            last_seen: now,
            last_ip: obs.ip.clone(),
            zone_id: zone.id.to_string(),
            // Only the LAN path inserts devices; a remote grant always targets
            // an already-existing device (issue #810).
            connection_mode: DeviceConnectionMode::Lan,
        };

        self.devices
            .insert(&row)
            .await
            .map_err(AppError::Internal)?;

        {
            let mut state = self.state.write().await;
            state.insert(
                obs.mac.clone(),
                DeviceMemoryState {
                    device_id,
                    mac: obs.mac.clone(),
                    last_ip: obs.ip.clone(),
                    last_seen: Instant::now(),
                    gone: false,
                },
            );
        }

        self.events.publish(WardnetEvent::DeviceDiscovered {
            device_id,
            mac: obs.mac.clone(),
            ip: obs.ip.clone(),
            hostname: None,
            timestamp: chrono::Utc::now(),
        });

        // New-device quarantine (#738): this is the only truly-first-ever path
        // (reached solely when the MAC had no prior DB row), so gating the
        // notification here is idempotent by construction — a reconnecting
        // device never reaches this. Placement already happened above (the
        // device landed in the default-for-new zone); the toggle only controls
        // whether admins are nudged to approve. A read failure must not block
        // discovery, so it degrades to "no notification".
        match self.system_config.is_quarantine_new_devices().await {
            Ok(true) => {
                self.events.publish(WardnetEvent::NewDeviceQuarantined {
                    device_id,
                    mac: obs.mac.clone(),
                    zone_name: zone.name.clone(),
                    timestamp: chrono::Utc::now(),
                });
            }
            Ok(false) => {}
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    mac = %obs.mac,
                    "failed to read quarantine_new_devices toggle; skipping notification",
                );
            }
        }

        Ok(ObservationResult::NewDevice {
            device_id,
            manufacturer,
            device_type,
        })
    }

    /// Publish a [`WardnetEvent::DeviceGone`] for a departed device. Shared by
    /// the timeout sweep ([`scan_departures`]) and the `WireGuard`
    /// handshake-staleness path ([`mark_peer_gone`]) so the event payload stays
    /// identical regardless of which liveness signal triggered the departure.
    ///
    /// [`scan_departures`]: DeviceDiscoveryService::scan_departures
    /// [`mark_peer_gone`]: DeviceDiscoveryService::mark_peer_gone
    fn publish_device_gone(&self, device_id: Uuid, mac: String, last_ip: String) {
        self.events.publish(WardnetEvent::DeviceGone {
            device_id,
            mac,
            last_ip,
            timestamp: chrono::Utc::now(),
        });
    }

    /// Shared Reappear/IpChanged/JustSeen transition logic for the `state` map,
    /// used by both the LAN and WireGuard-peer observation paths. `on_unknown`
    /// controls what happens when `mac` isn't currently tracked: the LAN path
    /// passes `None` (defer to the DB — the mac might belong to an existing,
    /// currently-untracked device row) and gets back [`ObsAction::Unknown`]; the
    /// peer path passes `Some(device_id)` (the device row always pre-exists for
    /// a remote-access grant) and gets an insert-as-present [`ObsAction::Reappear`]
    /// instead.
    async fn observation_action(&self, mac: &str, ip: &str, on_unknown: Option<Uuid>) -> ObsAction {
        let mut state = self.state.write().await;
        match state.get_mut(mac) {
            Some(entry) if entry.gone => {
                entry.gone = false;
                entry.last_seen = Instant::now();
                ip.clone_into(&mut entry.last_ip);
                ObsAction::Reappear {
                    device_id: entry.device_id,
                    mac: entry.mac.clone(),
                }
            }
            Some(entry) if entry.last_ip != ip => {
                let old_ip = entry.last_ip.clone();
                ip.clone_into(&mut entry.last_ip);
                entry.last_seen = Instant::now();
                ObsAction::IpChanged {
                    device_id: entry.device_id,
                    mac: entry.mac.clone(),
                    old_ip,
                }
            }
            Some(entry) => {
                entry.last_seen = Instant::now();
                ObsAction::JustSeen(entry.device_id)
            }
            None => match on_unknown {
                Some(device_id) => {
                    state.insert(
                        mac.to_owned(),
                        DeviceMemoryState {
                            device_id,
                            mac: mac.to_owned(),
                            last_ip: ip.to_owned(),
                            last_seen: Instant::now(),
                            gone: false,
                        },
                    );
                    ObsAction::Reappear {
                        device_id,
                        mac: mac.to_owned(),
                    }
                }
                None => ObsAction::Unknown,
            },
        }
    }
}

/// Action to perform after examining in-memory state (used to avoid
/// holding the write lock across `.await` points).
enum ObsAction {
    /// Not in memory map; check DB or insert new.
    Unknown,
    /// Device was in map and marked gone; it's back.
    Reappear { device_id: Uuid, mac: String },
    /// Device is in map, IP changed.
    IpChanged {
        device_id: Uuid,
        mac: String,
        old_ip: String,
    },
    /// Device is in map, same IP, just update `last_seen` in memory.
    JustSeen(Uuid),
}

#[async_trait]
impl DeviceDiscoveryService for DeviceDiscoveryServiceImpl {
    async fn restore_devices(&self) -> Result<(), AppError> {
        // Fold the configured zone subnets into the trusted set before we start
        // processing observations, so a device inside a zone subnet isn't dropped
        // in the window between startup and the first zone-change event.
        self.rebuild_trusted_subnets().await?;

        let devices = self.devices.find_all().await.map_err(AppError::Internal)?;
        let count = devices.len();

        let mut state = self.state.write().await;
        let mut poisoned: Vec<(Uuid, String, DeviceConnectionMode)> = Vec::new();
        for mut device in devices {
            // Repair a row claiming our own LAN IP (written by a pre-fix daemon
            // — ingest refuses to create one now). Left alone it would be
            // silently un-enforceable forever while the UI shows it as a normal
            // device holding the Pi's address (issue #886). Clearing the IP is
            // truthful: we do not know where this device really is.
            if device
                .last_ip
                .parse::<std::net::Ipv4Addr>()
                .is_ok_and(|ip| !ip.is_unspecified() && ip == self.lan_ip)
            {
                tracing::error!(
                    device_id = %device.id,
                    mac = %device.mac,
                    ip = %device.last_ip,
                    "device discovery: stored device {mac} claims Wardnet's own LAN IP {ip} — \
                     clearing its address; it will be re-registered at its real IP on the \
                     next observation (issue #886)",
                    mac = device.mac,
                    ip = device.last_ip,
                );
                poisoned.push((device.id, device.mac.clone(), device.connection_mode));
                device.last_ip = String::new();
            }
            state.insert(
                device.mac.clone(),
                DeviceMemoryState {
                    device_id: device.id,
                    mac: device.mac,
                    last_ip: device.last_ip,
                    last_seen: Instant::now(),
                    gone: true,
                },
            );
        }
        drop(state);

        for (device_id, mac, mode) in poisoned {
            let now = chrono::Utc::now().to_rfc3339();
            if let Err(e) = self
                .devices
                .update_last_seen_and_ip(&device_id.to_string(), "", &now, mode)
                .await
            {
                tracing::warn!(
                    error = %e,
                    device_id = %device_id,
                    mac = %mac,
                    "device discovery: failed to clear own-IP device row (issue #886)"
                );
            }
        }

        tracing::info!(count, "restored device state from database: count={count}");
        Ok(())
    }

    async fn rebuild_trusted_subnets(&self) -> Result<(), AppError> {
        let zones = self.zones.find_all().await.map_err(AppError::Internal)?;

        // Start from the base LAN subnet; every valid zone subnet is folded in.
        let mut subnets: Vec<ipnetwork::Ipv4Network> = Vec::with_capacity(zones.len() + 1);
        subnets.push(self.lan_subnet);
        subnets.extend(
            crate::subnet::parse_zone_subnets(&zones)
                .into_iter()
                .map(|(_, net)| net),
        );

        tracing::debug!(
            count = subnets.len(),
            "device discovery: rebuilt trusted subnets: count={count}",
            count = subnets.len(),
        );
        self.trusted_subnets.store(Arc::new(subnets));
        Ok(())
    }

    async fn process_observation(
        &self,
        obs: &ObservedDevice,
    ) -> Result<ObservationResult, AppError> {
        // Filter out observations from IPs outside every trusted subnet (the LAN
        // `/24` plus any configured Network-Zone subnet). When the Pi is the
        // gateway, return traffic from the internet arrives with the router's MAC
        // but remote server IPs — ignore those. A device DHCP-addressed inside a
        // zone subnet is trusted here, so its presence and IP changes flow through
        // to zone enforcement.
        let parsed_ip = obs.ip.parse::<std::net::Ipv4Addr>().ok();
        if let Some(ip) = parsed_ip {
            let trusted = self.trusted_subnets.load();
            if !trusted.iter().any(|subnet| subnet.contains(ip)) {
                return Ok(ObservationResult::Ignored);
            }
        }

        // Serialize the full observe-then-persist sequence for this mac against
        // the WireGuard-peer path, so their DB writes can't land out of order
        // relative to their in-memory state transitions (per-mac only — unrelated
        // devices proceed concurrently).
        let mac_lock = self.lock_for_mac(&obs.mac).await;
        let _guard = mac_lock.lock_owned().await;

        // Record the claim in the flap history BEFORE any ignore-branch below:
        // a claim of our own IP must still count toward the flap threshold —
        // the #886 incident MAC claimed three real addresses plus the Pi's own,
        // and dropping the own-IP claim first would leave it forever one short
        // of the threshold, keeping its lies about the other three trusted.
        let flapping = self.is_flapping(&obs.mac, &obs.ip).await;

        // Our own address is never a device, whatever answers ARP for it. A row
        // claiming it makes zone enforcement act on the Pi itself — installing a
        // /32 host route for our own IP that collides with the kernel's local
        // route and blackholes the box to its entire network (issue #886).
        if parsed_ip.is_some_and(|ip| ip == self.lan_ip) {
            tracing::warn!(
                mac = %obs.mac,
                ip = %obs.ip,
                "device discovery: {mac} is claiming Wardnet's own LAN IP {ip} — refusing to \
                 record it; this is an address conflict on the LAN (issue #886)",
                mac = obs.mac,
                ip = obs.ip,
            );
            self.touch_presence(&obs.mac).await;
            return Ok(ObservationResult::Ignored);
        }

        // A MAC claiming many addresses at once is not a device moving; believe
        // none of them until it settles (issue #886).
        if flapping {
            self.touch_presence(&obs.mac).await;
            return Ok(ObservationResult::Ignored);
        }

        // Phase 1: classify the observation via the shared state machine. The
        // LAN path passes `None` for an untracked mac (defer to the DB — it may
        // be an existing, currently-untracked device row).
        let action = self.observation_action(&obs.mac, &obs.ip, None).await;

        match action {
            ObsAction::Unknown => self.handle_unknown_mac(obs).await,
            ObsAction::Reappear { device_id, mac } => {
                let now = chrono::Utc::now().to_rfc3339();
                self.devices
                    .update_last_seen_and_ip(
                        &device_id.to_string(),
                        &obs.ip,
                        &now,
                        DeviceConnectionMode::Lan,
                    )
                    .await
                    .map_err(AppError::Internal)?;

                self.events.publish(WardnetEvent::DeviceDiscovered {
                    device_id,
                    mac,
                    ip: obs.ip.clone(),
                    hostname: None,
                    timestamp: chrono::Utc::now(),
                });

                Ok(ObservationResult::Reappeared(device_id))
            }
            ObsAction::IpChanged {
                device_id,
                mac,
                old_ip,
            } => {
                let now = chrono::Utc::now().to_rfc3339();
                self.devices
                    .update_last_seen_and_ip(
                        &device_id.to_string(),
                        &obs.ip,
                        &now,
                        DeviceConnectionMode::Lan,
                    )
                    .await
                    .map_err(AppError::Internal)?;

                self.events.publish(WardnetEvent::DeviceIpChanged {
                    device_id,
                    mac,
                    old_ip: old_ip.clone(),
                    new_ip: obs.ip.clone(),
                    timestamp: chrono::Utc::now(),
                });

                Ok(ObservationResult::IpChanged { device_id, old_ip })
            }
            ObsAction::JustSeen(device_id) => Ok(ObservationResult::Seen(device_id)),
        }
    }

    async fn flush_last_seen(&self) -> Result<u64, AppError> {
        let updates: Vec<(String, String)> = {
            let state = self.state.read().await;
            let now = chrono::Utc::now().to_rfc3339();
            state
                .values()
                .filter(|s| !s.gone)
                .map(|s| (s.device_id.to_string(), now.clone()))
                .collect()
        };

        let count = u64::try_from(updates.len()).unwrap_or(0);
        if !updates.is_empty() {
            self.devices
                .update_last_seen_batch(&updates)
                .await
                .map_err(AppError::Internal)?;
        }

        Ok(count)
    }

    async fn scan_departures(&self, timeout_secs: u64) -> Result<Vec<Uuid>, AppError> {
        let timeout = std::time::Duration::from_secs(timeout_secs);

        let departed: Vec<(Uuid, String, String)> = {
            let mut state = self.state.write().await;
            let mut departed = Vec::new();
            for entry in state.values_mut() {
                if !entry.gone && entry.last_seen.elapsed() > timeout {
                    entry.gone = true;
                    departed.push((entry.device_id, entry.mac.clone(), entry.last_ip.clone()));
                }
            }
            departed
        };

        let ids: Vec<Uuid> = departed.iter().map(|(id, _, _)| *id).collect();

        for (device_id, mac, last_ip) in departed {
            self.clear_departed_ip(device_id, &mac).await;
            self.publish_device_gone(device_id, mac, last_ip);
        }

        Ok(ids)
    }

    async fn resolve_hostname(&self, mac: &str, ip: &str) -> Result<(), AppError> {
        // The MAC may belong to a device the registry hasn't seen yet (e.g.
        // a lease event arriving before packet capture observes the device).
        // Treat that as a no-op rather than an error; the next observation
        // will trigger another resolution attempt.
        //
        // Casing is canonicalised at the repository boundary (issue #312),
        // so a single MAC string is fed to both repos regardless of the
        // caller's source.
        let Some(device) = self
            .devices
            .find_by_mac(mac)
            .await
            .map_err(AppError::Internal)?
        else {
            tracing::debug!(mac, "resolve_hostname: device not yet registered");
            return Ok(());
        };

        // Prefer the client-supplied option-12 hostname when present; FR-022
        // says it's the primary source. Fall back to reverse DNS otherwise.
        let dhcp_hostname = self
            .dhcp
            .find_active_lease_by_mac(mac)
            .await
            .map_err(AppError::Internal)?
            .and_then(|l| l.hostname)
            .map(|h| h.trim().to_owned())
            .filter(|h| !h.is_empty());

        let hostname = match dhcp_hostname {
            Some(h) => Some(h),
            None => self.resolver.resolve(ip).await,
        };

        if let Some(hostname) = hostname {
            self.devices
                .update_hostname(&device.id.to_string(), &hostname)
                .await
                .map_err(AppError::Internal)?;
        }
        Ok(())
    }

    async fn get_all_devices(&self) -> Result<Vec<Device>, AppError> {
        auth_context::require_authenticated()?;
        let ctx = auth_context::try_current().unwrap_or(AuthContext::Anonymous);
        match ctx {
            AuthContext::Admin { .. } => self.devices.find_all().await.map_err(AppError::Internal),
            AuthContext::Device { mac } => {
                match self
                    .devices
                    .find_by_mac(&mac)
                    .await
                    .map_err(AppError::Internal)?
                {
                    Some(d) => Ok(vec![d]),
                    None => Ok(vec![]),
                }
            }
            // Already handled by require_authenticated above.
            AuthContext::Anonymous => unreachable!(),
        }
    }

    async fn get_device_by_id(&self, id: Uuid) -> Result<Device, AppError> {
        auth_context::require_authenticated()?;
        let ctx = auth_context::try_current().unwrap_or(AuthContext::Anonymous);

        let device = self
            .devices
            .find_by_id(&id.to_string())
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::NotFound(format!("device {id} not found")))?;

        // Device callers can only view their own device.
        if let AuthContext::Device { mac } = &ctx
            && device.mac != *mac
        {
            return Err(AppError::Forbidden(
                "not authorised to view this device".to_owned(),
            ));
        }

        Ok(device)
    }

    async fn update_device(
        &self,
        id: Uuid,
        name: Option<&str>,
        device_type: Option<DeviceType>,
    ) -> Result<Device, AppError> {
        auth_context::require_authenticated()?;
        let ctx = auth_context::try_current().unwrap_or(AuthContext::Anonymous);

        // Device callers can only update their own device, and only if not admin-locked.
        if let AuthContext::Device { mac } = &ctx {
            let device = self
                .devices
                .find_by_id(&id.to_string())
                .await
                .map_err(AppError::Internal)?
                .ok_or_else(|| AppError::NotFound(format!("device {id} not found")))?;

            if device.mac != *mac {
                return Err(AppError::Forbidden(
                    "not authorised to modify this device".to_owned(),
                ));
            }
            if device.admin_locked {
                return Err(AppError::Forbidden("device is locked by admin".to_owned()));
            }
        }

        // If a device_type was provided, serialize it; otherwise fetch the current one.
        let type_str = if let Some(dt) = device_type {
            serde_json::to_string(&dt)
                .map(|s| s.trim_matches('"').to_owned())
                .map_err(|e| AppError::Internal(anyhow::anyhow!("serialize device type: {e}")))?
        } else {
            let current = self
                .devices
                .find_by_id(&id.to_string())
                .await
                .map_err(AppError::Internal)?
                .ok_or_else(|| AppError::NotFound(format!("device {id} not found")))?;
            serde_json::to_string(&current.device_type)
                .map(|s| s.trim_matches('"').to_owned())
                .map_err(|e| AppError::Internal(anyhow::anyhow!("serialize device type: {e}")))?
        };

        self.devices
            .update_name_and_type(&id.to_string(), name, &type_str)
            .await
            .map_err(AppError::Internal)?;

        self.devices
            .find_by_id(&id.to_string())
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::NotFound(format!("device {id} not found")))
    }

    async fn process_peer_observation(
        &self,
        device_id: Uuid,
        ip: &str,
    ) -> Result<ObservationResult, AppError> {
        // The device row always pre-exists for a remote-access grant (the
        // `inbound_wg_peers.device_id` FK guarantees it), so a miss is an
        // internal invariant violation — never the "new device" insert path.
        let device = self
            .devices
            .find_by_id(&device_id.to_string())
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| {
                AppError::Internal(anyhow::anyhow!(
                    "process_peer_observation: device {device_id} not found (FK should prevent this)"
                ))
            })?;
        let mac = device.mac;

        // Serialize the full observe-then-persist sequence for this mac against
        // the LAN path (see `process_observation`), so the two paths' DB writes
        // can't interleave out of order.
        let mac_lock = self.lock_for_mac(&mac).await;
        let _guard = mac_lock.lock_owned().await;

        // Phase 1: reuse the same in-memory state machine as the LAN path
        // (keyed by mac), minus the `lan_subnet` filter — peer IPs live in the
        // inbound WG subnet, not the LAN. `Unknown` is unreachable here: passing
        // `Some(device_id)` inserts an absent entry as-present, since the device
        // is always known on this path.
        let action = self.observation_action(&mac, ip, Some(device_id)).await;

        // Phase 2: every real arm persists with `connection_mode = Remote`
        // (a handshake observation regardless of IP change), so persist once,
        // then publish the arm-specific event.
        let now = chrono::Utc::now().to_rfc3339();
        self.devices
            .update_last_seen_and_ip(
                &device_id.to_string(),
                ip,
                &now,
                DeviceConnectionMode::Remote,
            )
            .await
            .map_err(AppError::Internal)?;

        match action {
            ObsAction::Reappear { device_id, mac } => {
                self.events.publish(WardnetEvent::DeviceDiscovered {
                    device_id,
                    mac,
                    ip: ip.to_owned(),
                    hostname: None,
                    timestamp: chrono::Utc::now(),
                });
                Ok(ObservationResult::Reappeared(device_id))
            }
            ObsAction::IpChanged {
                device_id,
                mac,
                old_ip,
            } => {
                self.events.publish(WardnetEvent::DeviceIpChanged {
                    device_id,
                    mac,
                    old_ip: old_ip.clone(),
                    new_ip: ip.to_owned(),
                    timestamp: chrono::Utc::now(),
                });
                Ok(ObservationResult::IpChanged { device_id, old_ip })
            }
            ObsAction::JustSeen(device_id) => Ok(ObservationResult::Seen(device_id)),
            // Unreachable by construction (see phase-1); return an error rather
            // than panic so a background caller degrades gracefully.
            ObsAction::Unknown => Err(AppError::Internal(anyhow::anyhow!(
                "process_peer_observation: unexpected Unknown action for device {device_id}"
            ))),
        }
    }

    async fn touch_peer_presence(&self, device_id: Uuid) -> Result<(), AppError> {
        // Resolve mac internally (same pattern as process_peer_observation /
        // mark_peer_gone); a missing device is an internal invariant violation.
        let device = self
            .devices
            .find_by_id(&device_id.to_string())
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| {
                AppError::Internal(anyhow::anyhow!(
                    "touch_peer_presence: device {device_id} not found (FK should prevent this)"
                ))
            })?;
        let mac = device.mac;

        // Only bump an already-tracked, present entry. Creating an entry or
        // un-goneing one is `process_peer_observation`'s job on the real
        // stale→fresh transition — this is the cheap keep-alive for the ticks
        // in between.
        let mut state = self.state.write().await;
        if let Some(entry) = state.get_mut(&mac)
            && !entry.gone
        {
            entry.last_seen = Instant::now();
        }
        Ok(())
    }

    async fn mark_peer_gone(
        &self,
        device_id: Uuid,
        timeout: std::time::Duration,
    ) -> Result<Option<Uuid>, AppError> {
        // Resolve mac internally so callers never need to know it. A missing
        // device can't be tracked, so treat it as a no-op departure.
        let Some(device) = self
            .devices
            .find_by_id(&device_id.to_string())
            .await
            .map_err(AppError::Internal)?
        else {
            return Ok(None);
        };
        let mac = device.mac;

        let gone = {
            let mut state = self.state.write().await;
            match state.get_mut(&mac) {
                // Only flip if the SHARED liveness signal is also stale. If LAN
                // traffic (or any other path) has bumped `last_seen` inside the
                // window, decline — the device is still alive by another signal.
                Some(entry) if !entry.gone && entry.last_seen.elapsed() > timeout => {
                    entry.gone = true;
                    Some((entry.device_id, entry.mac.clone(), entry.last_ip.clone()))
                }
                _ => None,
            }
        };

        match gone {
            Some((id, mac, last_ip)) => {
                self.clear_departed_ip(id, &mac).await;
                self.publish_device_gone(id, mac, last_ip);
                Ok(Some(id))
            }
            None => Ok(None),
        }
    }
}
