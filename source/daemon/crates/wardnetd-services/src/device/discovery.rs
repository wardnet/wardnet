use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

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
    async fn mark_peer_gone(&self, device_id: Uuid) -> Result<Option<Uuid>, AppError>;
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
    /// LAN subnet — only observations from IPs within this range are processed.
    /// Filters out return traffic from the internet (remote IPs arriving with
    /// the router's MAC on the Ethernet frame).
    lan_subnet: ipnetwork::Ipv4Network,
    state: Arc<RwLock<HashMap<String, DeviceMemoryState>>>,
}

impl DeviceDiscoveryServiceImpl {
    /// Create a new discovery service with the given dependencies.
    pub fn new(
        devices: Arc<dyn DeviceRepository>,
        zones: Arc<dyn NetworkZoneRepository>,
        dhcp: Arc<dyn DhcpRepository>,
        system_config: Arc<dyn SystemConfigRepository>,
        events: Arc<dyn EventPublisher>,
        resolver: Arc<dyn HostnameResolver>,
        lan_subnet: ipnetwork::Ipv4Network,
    ) -> Self {
        Self {
            devices,
            zones,
            dhcp,
            system_config,
            events,
            resolver,
            lan_subnet,
            state: Arc::new(RwLock::new(HashMap::new())),
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

    /// Phase-1 of [`process_peer_observation`]: update the in-memory tracking
    /// entry for `mac` under the lock and classify the observation. Never
    /// yields [`ObsAction::Unknown`] — an untracked device is inserted as
    /// present (it is always a known device on this path).
    ///
    /// [`process_peer_observation`]: DeviceDiscoveryService::process_peer_observation
    async fn peer_observation_action(&self, mac: &str, device_id: Uuid, ip: &str) -> ObsAction {
        let mut state = self.state.write().await;
        match state.get_mut(mac) {
            Some(entry) if entry.gone => {
                entry.gone = false;
                entry.last_seen = Instant::now();
                ip.clone_into(&mut entry.last_ip);
                ObsAction::Reappear {
                    device_id,
                    mac: mac.to_owned(),
                }
            }
            Some(entry) if entry.last_ip != ip => {
                let old_ip = entry.last_ip.clone();
                ip.clone_into(&mut entry.last_ip);
                entry.last_seen = Instant::now();
                ObsAction::IpChanged {
                    device_id,
                    mac: mac.to_owned(),
                    old_ip,
                }
            }
            Some(entry) => {
                entry.last_seen = Instant::now();
                ObsAction::JustSeen(device_id)
            }
            None => {
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
        let devices = self.devices.find_all().await.map_err(AppError::Internal)?;
        let count = devices.len();

        let mut state = self.state.write().await;
        for device in devices {
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

        tracing::info!(count, "restored device state from database: count={count}");
        Ok(())
    }

    async fn process_observation(
        &self,
        obs: &ObservedDevice,
    ) -> Result<ObservationResult, AppError> {
        // Filter out observations from IPs outside the LAN subnet.
        // When the Pi is the gateway, return traffic from the internet arrives
        // with the router's MAC but remote server IPs — ignore those.
        if let Ok(ip) = obs.ip.parse::<std::net::Ipv4Addr>()
            && !self.lan_subnet.contains(ip)
        {
            return Ok(ObservationResult::Ignored);
        }

        // Phase 1: determine action while holding the lock, then drop it.
        let action = {
            let mut state = self.state.write().await;

            if let Some(entry) = state.get_mut(&obs.mac) {
                if entry.gone {
                    entry.gone = false;
                    entry.last_seen = Instant::now();
                    entry.last_ip.clone_from(&obs.ip);
                    ObsAction::Reappear {
                        device_id: entry.device_id,
                        mac: entry.mac.clone(),
                    }
                } else if entry.last_ip != obs.ip {
                    let old_ip = entry.last_ip.clone();
                    entry.last_ip.clone_from(&obs.ip);
                    entry.last_seen = Instant::now();
                    ObsAction::IpChanged {
                        device_id: entry.device_id,
                        mac: entry.mac.clone(),
                        old_ip,
                    }
                } else {
                    entry.last_seen = Instant::now();
                    ObsAction::JustSeen(entry.device_id)
                }
            } else {
                ObsAction::Unknown
            }
        };
        // Lock is dropped here.

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

        // Phase 1: reuse the same in-memory state machine as the LAN path
        // (keyed by mac), minus the `lan_subnet` filter — peer IPs live in the
        // inbound WG subnet, not the LAN. `Unknown` is unreachable here: an
        // absent entry is inserted-as-present, since the device is always known.
        let action = self.peer_observation_action(&mac, device_id, ip).await;

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

    async fn mark_peer_gone(&self, device_id: Uuid) -> Result<Option<Uuid>, AppError> {
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
                Some(entry) if !entry.gone => {
                    entry.gone = true;
                    Some((entry.device_id, entry.mac.clone(), entry.last_ip.clone()))
                }
                _ => None,
            }
        };

        match gone {
            Some((id, mac, last_ip)) => {
                self.publish_device_gone(id, mac, last_ip);
                Ok(Some(id))
            }
            None => Ok(None),
        }
    }
}
