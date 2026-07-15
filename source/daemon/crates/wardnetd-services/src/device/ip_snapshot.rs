use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Arc;

use arc_swap::ArcSwap;
use uuid::Uuid;

use wardnetd_data::repository::DeviceRepository;

/// Lock-free IP → device-id map consulted from the DNS hot path.
///
/// The DNS server resolves each query's source IP to a device id **at query
/// time** so log rows and stats carry a stable identity that survives DHCP
/// reassigning the IP to a different device later. A per-query repository
/// read is off the table on that path, so this mirrors the routing service's
/// `dns_upstream_snapshot` pattern: consumers hold the inner
/// [`ArcSwap`] and do a wait-free `load()` per query, while rebuilds swap in
/// a fresh map built from the devices table.
///
/// Rebuilds are full (never incremental) so the map is always exactly what
/// the devices table says: startup plus every event that can move an IP
/// between devices (`DeviceDiscovered`, `DeviceIpChanged`, `DeviceGone`).
pub struct DeviceIpSnapshot {
    devices: Arc<dyn DeviceRepository>,
    snapshot: Arc<ArcSwap<HashMap<IpAddr, Uuid>>>,
}

impl DeviceIpSnapshot {
    /// Create an empty snapshot backed by the given repository. Call
    /// [`Self::rebuild`] once at startup to populate it.
    #[must_use]
    pub fn new(devices: Arc<dyn DeviceRepository>) -> Self {
        Self {
            devices,
            snapshot: Arc::new(ArcSwap::from_pointee(HashMap::new())),
        }
    }

    /// Shared handle for hot-path consumers (the DNS server). Each query
    /// does `snapshot.load().get(&ip)` — wait-free, no locks.
    #[must_use]
    pub fn snapshot(&self) -> Arc<ArcSwap<HashMap<IpAddr, Uuid>>> {
        Arc::clone(&self.snapshot)
    }

    /// Rebuild the map from the devices table and atomically swap it in.
    ///
    /// Departed devices have `last_ip` cleared by discovery, but a stale
    /// duplicate can still exist transiently (old device not yet re-seen
    /// after its IP was handed to a new one). `last_seen` breaks the tie:
    /// the most recently seen claimant of an IP wins.
    pub async fn rebuild(&self) -> anyhow::Result<()> {
        let mut devices = self.devices.find_all().await?;
        devices.sort_by_key(|d| d.last_seen);

        let mut map = HashMap::with_capacity(devices.len());
        for device in devices {
            if device.last_ip.is_empty() {
                continue;
            }
            let Ok(ip) = device.last_ip.parse::<IpAddr>() else {
                tracing::warn!(
                    device_id = %device.id,
                    last_ip = %device.last_ip,
                    "skipping device with unparsable last_ip in IP snapshot: device_id={}, last_ip={}",
                    device.id,
                    device.last_ip
                );
                continue;
            };
            map.insert(ip, device.id);
        }

        let entry_count = map.len();
        tracing::debug!(
            entry_count,
            "rebuilt device IP snapshot: entry_count={entry_count}"
        );
        self.snapshot.store(Arc::new(map));
        Ok(())
    }
}
