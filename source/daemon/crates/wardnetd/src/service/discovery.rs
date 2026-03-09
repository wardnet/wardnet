use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use tokio::sync::RwLock;
use uuid::Uuid;
use wardnet_types::device::{Device, DeviceType};
use wardnet_types::event::WardnetEvent;

use crate::error::AppError;
use crate::event::EventPublisher;
use crate::hostname_resolver::HostnameResolver;
use crate::oui;
use crate::packet_capture::ObservedDevice;
use crate::repository::DeviceRepository;
use crate::repository::device::DeviceRow;

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
}

/// Device discovery and lifecycle management.
///
/// Processes packet capture observations to detect new devices, track IP changes,
/// detect departures, and resolve hostnames. Separate from [`DeviceService`] which
/// handles self-service user flows.
///
/// [`DeviceService`]: crate::service::DeviceService
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

    /// Resolve hostname for a device and update the database.
    async fn resolve_hostname(&self, device_id: Uuid, ip: String) -> Result<(), AppError>;

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
}

/// Default implementation of [`DeviceDiscoveryService`].
pub struct DeviceDiscoveryServiceImpl {
    devices: Arc<dyn DeviceRepository>,
    events: Arc<dyn EventPublisher>,
    resolver: Arc<dyn HostnameResolver>,
    state: Arc<RwLock<HashMap<String, DeviceMemoryState>>>,
}

impl DeviceDiscoveryServiceImpl {
    /// Create a new discovery service with the given dependencies.
    pub fn new(
        devices: Arc<dyn DeviceRepository>,
        events: Arc<dyn EventPublisher>,
        resolver: Arc<dyn HostnameResolver>,
    ) -> Self {
        Self {
            devices,
            events,
            resolver,
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
            .update_last_seen_and_ip(&device_id.to_string(), &obs.ip, &now)
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

        let row = DeviceRow {
            id: device_id.to_string(),
            mac: obs.mac.clone(),
            hostname: None,
            manufacturer: manufacturer.clone(),
            device_type: serde_json::to_string(&device_type)
                .unwrap_or_else(|_| "\"unknown\"".to_owned()),
            first_seen: now.clone(),
            last_seen: now,
            last_ip: obs.ip.clone(),
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

        Ok(ObservationResult::NewDevice {
            device_id,
            manufacturer,
            device_type,
        })
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

        tracing::info!(count, "restored device state from database");
        Ok(())
    }

    async fn process_observation(
        &self,
        obs: &ObservedDevice,
    ) -> Result<ObservationResult, AppError> {
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
                    .update_last_seen_and_ip(&device_id.to_string(), &obs.ip, &now)
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
                    .update_last_seen_and_ip(&device_id.to_string(), &obs.ip, &now)
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
            self.events.publish(WardnetEvent::DeviceGone {
                device_id,
                mac,
                last_ip,
                timestamp: chrono::Utc::now(),
            });
        }

        Ok(ids)
    }

    async fn resolve_hostname(&self, device_id: Uuid, ip: String) -> Result<(), AppError> {
        if let Some(hostname) = self.resolver.resolve(&ip).await {
            self.devices
                .update_hostname(&device_id.to_string(), &hostname)
                .await
                .map_err(AppError::Internal)?;
        }
        Ok(())
    }

    async fn get_all_devices(&self) -> Result<Vec<Device>, AppError> {
        self.devices.find_all().await.map_err(AppError::Internal)
    }

    async fn get_device_by_id(&self, id: Uuid) -> Result<Device, AppError> {
        self.devices
            .find_by_id(&id.to_string())
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::NotFound(format!("device {id} not found")))
    }

    async fn update_device(
        &self,
        id: Uuid,
        name: Option<&str>,
        device_type: Option<DeviceType>,
    ) -> Result<Device, AppError> {
        // If a device_type was provided, serialize it; otherwise fetch the current one.
        let type_str = if let Some(dt) = device_type {
            serde_json::to_string(&dt)
                .map_err(|e| AppError::Internal(anyhow::anyhow!("serialize device type: {e}")))?
        } else {
            let current = self
                .devices
                .find_by_id(&id.to_string())
                .await
                .map_err(AppError::Internal)?
                .ok_or_else(|| AppError::NotFound(format!("device {id} not found")))?;
            serde_json::to_string(&current.device_type)
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
}
