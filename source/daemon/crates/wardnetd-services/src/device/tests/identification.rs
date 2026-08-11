//! Service-layer tests for [`DeviceIdentificationServiceImpl`] (issue #1099).
//!
//! Builds the real `SqliteDeviceIdentificationRepository` + `SqliteDeviceRepository`
//! over an in-memory pool, matching the pattern the Network-Zone service tests
//! use, so the auth gate, the catalog resolution and the bounding rules are all
//! exercised against real SQL rather than a mock that could drift from it.

use std::future::Future;
use std::net::IpAddr;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use async_trait::async_trait;
use sqlx::SqlitePool;
use sqlx::sqlite::SqlitePoolOptions;
use wardnet_common::auth::AuthContext;
use wardnet_common::device::{DeviceConnectionMode, DeviceSignalKind, ManufacturerSource};
use wardnetd_data::repository::device::DeviceRow;
use wardnetd_data::repository::{
    DeviceRepository, SqliteDeviceIdentificationRepository, SqliteDeviceRepository,
};
use wardnetd_data::vendor_catalog;

use crate::auth_context;
use crate::device::identification::{DeviceIdentificationService, DeviceIdentificationServiceImpl};
use crate::device::prober::DeviceProber;
use crate::error::AppError;

const ZONE: &str = "00000000-0000-0000-0000-000000000201";

/// Departure timeout used by the test harness — the production default from
/// `detection.departure_timeout_secs`.
const DEPARTURE_TIMEOUT: Duration = Duration::from_mins(5);

/// Hand-written [`DeviceProber`] recording what it was asked to contact, per
/// `.agents/code-conventions.md` (no mocking libraries).
#[derive(Default)]
struct MockDeviceProber {
    /// Ports this prober claims answered, whatever it was asked to probe.
    answering: Vec<u16>,
    /// Every `(ip, ports)` pair the service passed in.
    calls: Mutex<Vec<(IpAddr, Vec<u16>)>>,
}

impl MockDeviceProber {
    fn answering(ports: &[u16]) -> Arc<Self> {
        Arc::new(Self {
            answering: ports.to_vec(),
            calls: Mutex::new(Vec::new()),
        })
    }

    fn silent() -> Arc<Self> {
        Arc::new(Self::default())
    }

    fn calls(&self) -> Vec<(IpAddr, Vec<u16>)> {
        self.calls.lock().unwrap().clone()
    }
}

#[async_trait]
impl DeviceProber for MockDeviceProber {
    async fn probe(&self, ip: IpAddr, ports: &[u16]) -> Vec<u16> {
        self.calls.lock().unwrap().push((ip, ports.to_vec()));
        self.answering.clone()
    }
}

async fn test_pool() -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("../wardnetd-data/migrations")
        .run(&pool)
        .await
        .unwrap();
    pool
}

struct Harness {
    svc: DeviceIdentificationServiceImpl,
    devices: Arc<dyn DeviceRepository>,
}

async fn build() -> Harness {
    build_with_prober(MockDeviceProber::silent()).await
}

async fn build_with_prober(prober: Arc<MockDeviceProber>) -> Harness {
    let pool = test_pool().await;
    let identification = Arc::new(SqliteDeviceIdentificationRepository::new(pool.clone()));
    let devices: Arc<dyn DeviceRepository> = Arc::new(SqliteDeviceRepository::new(pool));
    Harness {
        svc: DeviceIdentificationServiceImpl::new(
            identification,
            devices.clone(),
            prober,
            DEPARTURE_TIMEOUT,
        ),
        devices,
    }
}

/// Insert a device with no manufacturer, returning its id.
async fn insert_device(devices: &Arc<dyn DeviceRepository>, mac: &str) -> String {
    insert_device_with_ip(devices, mac, "192.168.1.10").await
}

/// Insert a device with no manufacturer at a specific `last_ip`, returning its
/// id. The IP is what the mDNS observer resolves against.
async fn insert_device_with_ip(
    devices: &Arc<dyn DeviceRepository>,
    mac: &str,
    last_ip: &str,
) -> String {
    insert_device_at(devices, mac, last_ip, chrono::Utc::now()).await
}

/// Insert a device with an explicit address and last-seen time, for the probe
/// guards. `last_seen` drives the departure check and `last_ip` the address
/// check, so both have to be settable.
async fn insert_device_at(
    devices: &Arc<dyn DeviceRepository>,
    mac: &str,
    last_ip: &str,
    last_seen: chrono::DateTime<chrono::Utc>,
) -> String {
    let id = uuid::Uuid::new_v4().to_string();
    devices
        .insert(&DeviceRow {
            id: id.clone(),
            mac: mac.to_owned(),
            hostname: None,
            manufacturer: None,
            manufacturer_source: None,
            is_randomized: false,
            device_type: "unknown".to_owned(),
            first_seen: "2026-03-07T00:00:00Z".to_owned(),
            last_seen: last_seen.to_rfc3339(),
            last_ip: last_ip.to_owned(),
            zone_id: ZONE.to_owned(),
            connection_mode: DeviceConnectionMode::Lan,
        })
        .await
        .unwrap();
    id
}

async fn as_admin<F: Future>(fut: F) -> F::Output {
    auth_context::with_context(
        AuthContext::Admin {
            admin_id: uuid::Uuid::nil(),
        },
        fut,
    )
    .await
}

#[tokio::test]
async fn recording_is_admin_gated() {
    // Background producers (DHCP, mDNS) must supply an admin context. Without
    // the gate an unauthenticated LAN path would be writing device rows.
    let h = build().await;
    let id = insert_device(&h.devices, "aa:bb:cc:dd:ee:01").await;

    let result = h
        .svc
        .record_signal(&id, DeviceSignalKind::DhcpHostname, "lamp")
        .await;

    assert!(result.is_err(), "no ambient context must be refused");
}

#[tokio::test]
async fn an_mdns_service_names_an_unnamed_device() {
    let h = build().await;
    let id = insert_device(&h.devices, "aa:bb:cc:dd:ee:01").await;

    as_admin(
        h.svc
            .record_signal(&id, DeviceSignalKind::MdnsService, "_googlecast._tcp"),
    )
    .await
    .unwrap();

    let device = h.devices.find_by_id(&id).await.unwrap().unwrap();
    assert_eq!(device.manufacturer.as_deref(), Some("Google"));
    assert_eq!(device.manufacturer_source, Some(ManufacturerSource::Signal));
}

#[tokio::test]
async fn a_second_vendors_signal_does_not_flip_the_manufacturer() {
    // An Apple TV answers both `_airplay._tcp` and `_googlecast._tcp`. Without
    // first-writer-wins its manufacturer would flip on every re-announcement.
    let h = build().await;
    let id = insert_device(&h.devices, "aa:bb:cc:dd:ee:01").await;

    as_admin(
        h.svc
            .record_signal(&id, DeviceSignalKind::MdnsService, "_airplay._tcp"),
    )
    .await
    .unwrap();
    as_admin(
        h.svc
            .record_signal(&id, DeviceSignalKind::MdnsService, "_googlecast._tcp"),
    )
    .await
    .unwrap();

    let device = h.devices.find_by_id(&id).await.unwrap().unwrap();
    assert_eq!(device.manufacturer.as_deref(), Some("Apple"));
    // Both observations are still recorded — only the naming is first-wins.
    assert_eq!(as_admin(h.svc.signals_for(&id)).await.unwrap().len(), 2);
}

#[tokio::test]
async fn a_hostname_is_recorded_but_names_nobody() {
    // Option 12 is a useful signal to show an admin, but it carries no vendor
    // information, so it must not invent a manufacturer.
    let h = build().await;
    let id = insert_device(&h.devices, "aa:bb:cc:dd:ee:01").await;

    as_admin(
        h.svc
            .record_signal(&id, DeviceSignalKind::DhcpHostname, "some-host"),
    )
    .await
    .unwrap();

    let device = h.devices.find_by_id(&id).await.unwrap().unwrap();
    assert_eq!(device.manufacturer, None);
    assert_eq!(as_admin(h.svc.signals_for(&id)).await.unwrap().len(), 1);
}

#[tokio::test]
async fn an_oversized_value_is_truncated() {
    // DHCP options are attacker-controlled; one packet must not be able to
    // store an arbitrarily long string.
    let h = build().await;
    let id = insert_device(&h.devices, "aa:bb:cc:dd:ee:01").await;

    as_admin(
        h.svc
            .record_signal(&id, DeviceSignalKind::DhcpVendorClass, &"x".repeat(4096)),
    )
    .await
    .unwrap();

    let signals = as_admin(h.svc.signals_for(&id)).await.unwrap();
    assert_eq!(signals.len(), 1);
    assert!(
        signals[0].value.len() <= 128,
        "value should be clamped, got {}",
        signals[0].value.len()
    );
}

#[tokio::test]
async fn truncation_respects_utf8_boundaries() {
    // The clamp walks back to a character boundary. A value whose 128th byte
    // lands mid-character must not panic, and must not emit invalid UTF-8.
    let h = build().await;
    let id = insert_device(&h.devices, "aa:bb:cc:dd:ee:01").await;

    // One ASCII byte then two-byte characters, so the 128-byte cut lands
    // *inside* a character and the boundary walk actually has to step back.
    // (A pure "é" string would put byte 128 exactly on a boundary and never
    // exercise the loop.)
    let value = format!("a{}", "é".repeat(200));
    as_admin(
        h.svc
            .record_signal(&id, DeviceSignalKind::DhcpVendorClass, &value),
    )
    .await
    .unwrap();

    let signals = as_admin(h.svc.signals_for(&id)).await.unwrap();
    assert_eq!(signals.len(), 1);
    assert!(signals[0].value.len() <= 128);
    assert!(
        signals[0].value.starts_with('a') && signals[0].value.ends_with('é'),
        "truncation must land on a character boundary, got {:?}",
        signals[0].value
    );
}

#[tokio::test]
async fn a_probed_port_resolves_to_its_vendor() {
    // The admin-triggered prober records the port that answered; the catalog
    // turns that into a vendor name.
    let h = build().await;
    let id = insert_device(&h.devices, "aa:bb:cc:dd:ee:01").await;

    as_admin(
        h.svc
            .record_signal(&id, DeviceSignalKind::ProbedPort, "6668"),
    )
    .await
    .unwrap();

    let device = h.devices.find_by_id(&id).await.unwrap().unwrap();
    assert_eq!(device.manufacturer.as_deref(), Some("Tuya"));
}

#[tokio::test]
async fn a_port_outside_the_catalog_names_nobody() {
    let h = build().await;
    let id = insert_device(&h.devices, "aa:bb:cc:dd:ee:01").await;

    as_admin(h.svc.record_signal(&id, DeviceSignalKind::ProbedPort, "22"))
        .await
        .unwrap();
    // A non-numeric value must also degrade rather than panic.
    as_admin(
        h.svc
            .record_signal(&id, DeviceSignalKind::ProbedPort, "not-a-port"),
    )
    .await
    .unwrap();

    let device = h.devices.find_by_id(&id).await.unwrap().unwrap();
    assert_eq!(device.manufacturer, None);
}

#[tokio::test]
async fn a_client_cycling_its_vendor_class_cannot_grow_the_table() {
    let h = build().await;
    let id = insert_device(&h.devices, "aa:bb:cc:dd:ee:01").await;

    for i in 0..40 {
        as_admin(h.svc.record_signal(
            &id,
            DeviceSignalKind::DhcpVendorClass,
            &format!("vendor-{i}"),
        ))
        .await
        .unwrap();
    }

    let signals = as_admin(h.svc.signals_for(&id)).await.unwrap();
    assert!(
        signals.len() <= 16,
        "signals per kind should be capped, got {}",
        signals.len()
    );
}

#[tokio::test]
async fn an_unknown_mac_is_dropped_rather_than_failing() {
    // DHCP routinely sees a client before ARP discovery inserts its row. A
    // lease must not fail because an observability side-effect had nowhere to
    // land.
    let h = build().await;

    let result = as_admin(h.svc.record_signal_for_mac(
        "ff:ff:ff:ff:ff:ff",
        DeviceSignalKind::DhcpHostname,
        "ghost",
    ))
    .await;

    assert!(result.is_ok());
}

#[tokio::test]
async fn record_for_mac_is_case_insensitive() {
    let h = build().await;
    let id = insert_device(&h.devices, "aa:bb:cc:dd:ee:01").await;

    as_admin(h.svc.record_signal_for_mac(
        "AA:BB:CC:DD:EE:01",
        DeviceSignalKind::MdnsService,
        "_govee._tcp",
    ))
    .await
    .unwrap();

    assert_eq!(as_admin(h.svc.signals_for(&id)).await.unwrap().len(), 1);
}

#[tokio::test]
async fn an_mdns_signal_names_the_single_device_holding_an_ip() {
    // The happy path: an IP that resolves to exactly one device attributes the
    // observed service type to it, just like the MAC path.
    let h = build().await;
    let id = insert_device_with_ip(&h.devices, "aa:bb:cc:dd:ee:01", "192.168.1.42").await;

    as_admin(h.svc.record_signal_for_ip(
        "192.168.1.42".parse().unwrap(),
        DeviceSignalKind::MdnsService,
        "_googlecast._tcp.local.",
    ))
    .await
    .unwrap();

    let device = h.devices.find_by_id(&id).await.unwrap().unwrap();
    assert_eq!(device.manufacturer.as_deref(), Some("Google"));
    assert_eq!(as_admin(h.svc.signals_for(&id)).await.unwrap().len(), 1);
}

#[tokio::test]
async fn an_ip_with_no_device_is_skipped() {
    // A browse result for an address no device currently holds is dropped
    // rather than failing — an observability side-effect must never error out
    // the passive observer.
    let h = build().await;
    let id = insert_device_with_ip(&h.devices, "aa:bb:cc:dd:ee:01", "192.168.1.42").await;

    as_admin(h.svc.record_signal_for_ip(
        "192.168.1.99".parse().unwrap(),
        DeviceSignalKind::MdnsService,
        "_googlecast._tcp.local.",
    ))
    .await
    .unwrap();

    let device = h.devices.find_by_id(&id).await.unwrap().unwrap();
    assert_eq!(device.manufacturer, None);
    assert!(as_admin(h.svc.signals_for(&id)).await.unwrap().is_empty());
}

#[tokio::test]
async fn an_ip_shared_by_two_devices_is_skipped() {
    // The core of the ADR's mDNS decision: `last_ip` is not unique (a departed
    // row can still hold a recycled address), and a wrong attribution is worse
    // than no signal. When an IP maps to more than one device, neither is
    // named and nothing is recorded.
    let h = build().await;
    let a = insert_device_with_ip(&h.devices, "aa:bb:cc:dd:ee:01", "192.168.1.42").await;
    let b = insert_device_with_ip(&h.devices, "aa:bb:cc:dd:ee:02", "192.168.1.42").await;

    as_admin(h.svc.record_signal_for_ip(
        "192.168.1.42".parse().unwrap(),
        DeviceSignalKind::MdnsService,
        "_googlecast._tcp.local.",
    ))
    .await
    .unwrap();

    for id in [&a, &b] {
        let device = h.devices.find_by_id(id).await.unwrap().unwrap();
        assert_eq!(device.manufacturer, None, "no device may be named on a tie");
        assert!(
            as_admin(h.svc.signals_for(id)).await.unwrap().is_empty(),
            "no signal may be recorded on a tie"
        );
    }
}

#[tokio::test]
async fn record_signal_for_ip_is_admin_gated() {
    // The observer is a background component and must supply an admin context,
    // exactly like the DHCP producer.
    let h = build().await;
    insert_device_with_ip(&h.devices, "aa:bb:cc:dd:ee:01", "192.168.1.42").await;

    let result = h
        .svc
        .record_signal_for_ip(
            "192.168.1.42".parse().unwrap(),
            DeviceSignalKind::MdnsService,
            "_googlecast._tcp.local.",
        )
        .await;

    assert!(result.is_err(), "no ambient context must be refused");
}

#[tokio::test]
async fn reconcile_names_a_privately_listed_device_discovered_earlier() {
    // The migration can only *remove* placeholder manufacturers — the catalog
    // lives in Rust. Without this pass an upgrade leaves the Govee lamp that
    // motivated issue #1099 unidentified forever, since only insert_new_device
    // consults the catalog and the device was discovered long ago.
    let h = build().await;
    let id = insert_device(&h.devices, "5c:e7:53:4e:ec:d9").await;

    let named = as_admin(h.svc.reconcile_from_catalog()).await.unwrap();

    assert_eq!(named, 1);
    let device = h.devices.find_by_id(&id).await.unwrap().unwrap();
    assert_eq!(device.manufacturer.as_deref(), Some("Govee"));
    assert_eq!(
        device.manufacturer_source,
        Some(ManufacturerSource::Catalog)
    );
}

#[tokio::test]
async fn reconcile_is_idempotent_and_leaves_named_devices_alone() {
    let h = build().await;
    insert_device(&h.devices, "5c:e7:53:4e:ec:d9").await;

    assert_eq!(as_admin(h.svc.reconcile_from_catalog()).await.unwrap(), 1);
    assert_eq!(
        as_admin(h.svc.reconcile_from_catalog()).await.unwrap(),
        0,
        "a second pass must find nothing left to name"
    );
}

#[tokio::test]
async fn reconcile_skips_devices_the_catalog_cannot_name() {
    let h = build().await;
    let id = insert_device(&h.devices, "aa:bb:cc:dd:ee:01").await;

    assert_eq!(as_admin(h.svc.reconcile_from_catalog()).await.unwrap(), 0);
    let device = h.devices.find_by_id(&id).await.unwrap().unwrap();
    assert_eq!(device.manufacturer, None);
}

// ---------------------------------------------------------------------------
// Admin-triggered probe (issue #1116)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn probing_is_admin_gated() {
    let h = build().await;
    let id = insert_device(&h.devices, "aa:bb:cc:dd:ee:01").await;

    assert!(h.svc.probe_device(&id).await.is_err());
}

#[tokio::test]
async fn an_answering_port_becomes_a_signal_and_names_the_device() {
    // The whole point of the feature: a Tuya bulb answering :6668 stops being
    // "Unknown manufacturer".
    let prober = MockDeviceProber::answering(&[6668]);
    let h = build_with_prober(prober).await;
    let id = insert_device(&h.devices, "aa:bb:cc:dd:ee:01").await;

    let outcome = as_admin(h.svc.probe_device(&id)).await.unwrap();

    assert_eq!(outcome.answering_ports, vec![6668]);
    let signals = as_admin(h.svc.signals_for(&id)).await.unwrap();
    assert_eq!(signals.len(), 1);
    assert_eq!(signals[0].kind, DeviceSignalKind::ProbedPort);
    assert_eq!(signals[0].value, "6668");
    let device = h.devices.find_by_id(&id).await.unwrap().unwrap();
    assert_eq!(device.manufacturer.as_deref(), Some("Tuya"));
    assert_eq!(device.manufacturer_source, Some(ManufacturerSource::Signal));
}

#[tokio::test]
async fn a_probe_that_finds_nothing_is_a_result_not_a_failure() {
    // `ports_probed` is what lets the UI say "we contacted these and none
    // answered" rather than leaving the button looking broken.
    let h = build_with_prober(MockDeviceProber::silent()).await;
    let id = insert_device(&h.devices, "aa:bb:cc:dd:ee:01").await;

    let outcome = as_admin(h.svc.probe_device(&id)).await.unwrap();

    assert!(outcome.answering_ports.is_empty());
    assert!(!outcome.ports_probed.is_empty());
    assert!(as_admin(h.svc.signals_for(&id)).await.unwrap().is_empty());
}

#[tokio::test]
async fn the_prober_is_asked_for_exactly_the_catalog_ports() {
    // The probe surface is the catalog's and only the catalog's — ADR 0025 §5.
    // A service that widened it here would defeat the bound the catalog test
    // asserts.
    let prober = MockDeviceProber::silent();
    let h = build_with_prober(prober.clone()).await;
    let id = insert_device_at(
        &h.devices,
        "aa:bb:cc:dd:ee:01",
        "192.168.1.77",
        chrono::Utc::now(),
    )
    .await;

    let outcome = as_admin(h.svc.probe_device(&id)).await.unwrap();

    let calls = prober.calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].0, "192.168.1.77".parse::<IpAddr>().unwrap());
    assert_eq!(calls[0].1, vendor_catalog::probe_ports());
    assert_eq!(outcome.ports_probed, vendor_catalog::probe_ports());
}

#[tokio::test]
async fn a_departed_device_is_refused_without_being_contacted() {
    // `last_ip` is last-observation-wins. Probing a device that left contacts
    // whoever holds that address now, and `set_manufacturer_if_absent` would
    // write their vendor onto this row permanently.
    let prober = MockDeviceProber::answering(&[6668]);
    let h = build_with_prober(prober.clone()).await;
    let id = insert_device_at(
        &h.devices,
        "aa:bb:cc:dd:ee:01",
        "192.168.1.10",
        chrono::Utc::now() - chrono::Duration::seconds(301),
    )
    .await;

    let err = as_admin(h.svc.probe_device(&id)).await.unwrap_err();

    assert!(matches!(err, AppError::Conflict(_)), "got {err:?}");
    assert!(
        prober.calls().is_empty(),
        "a departed device must not be contacted"
    );
    assert_eq!(
        h.devices
            .find_by_id(&id)
            .await
            .unwrap()
            .unwrap()
            .manufacturer,
        None
    );
}

#[tokio::test]
async fn a_device_seen_just_inside_the_timeout_is_still_probeable() {
    // The boundary matters: a device that checked in 299 s ago is present, and
    // refusing it would make the button unusable for anything idle.
    let prober = MockDeviceProber::silent();
    let h = build_with_prober(prober.clone()).await;
    let id = insert_device_at(
        &h.devices,
        "aa:bb:cc:dd:ee:01",
        "192.168.1.10",
        chrono::Utc::now() - chrono::Duration::seconds(299),
    )
    .await;

    as_admin(h.svc.probe_device(&id)).await.unwrap();

    assert_eq!(prober.calls().len(), 1);
}

#[tokio::test]
async fn a_globally_routable_address_is_refused() {
    // A record that has drifted to a public address points at someone we have
    // no business contacting.
    let prober = MockDeviceProber::answering(&[6668]);
    let h = build_with_prober(prober.clone()).await;
    let id = insert_device_at(
        &h.devices,
        "aa:bb:cc:dd:ee:01",
        "93.184.216.34",
        chrono::Utc::now(),
    )
    .await;

    let err = as_admin(h.svc.probe_device(&id)).await.unwrap_err();

    assert!(matches!(err, AppError::Conflict(_)), "got {err:?}");
    assert!(prober.calls().is_empty());
}

#[tokio::test]
async fn a_remote_wireguard_peer_stays_probeable() {
    // Inbound WG peers live on 10.100.64.0/24 — private, so roaming devices are
    // not collateral damage of the address guard.
    let prober = MockDeviceProber::silent();
    let h = build_with_prober(prober.clone()).await;
    let id = insert_device_at(
        &h.devices,
        "aa:bb:cc:dd:ee:01",
        "10.100.64.7",
        chrono::Utc::now(),
    )
    .await;

    as_admin(h.svc.probe_device(&id)).await.unwrap();

    assert_eq!(prober.calls().len(), 1);
}

#[tokio::test]
async fn an_unparseable_address_is_refused() {
    let prober = MockDeviceProber::answering(&[6668]);
    let h = build_with_prober(prober.clone()).await;
    let id = insert_device_at(&h.devices, "aa:bb:cc:dd:ee:01", "", chrono::Utc::now()).await;

    let err = as_admin(h.svc.probe_device(&id)).await.unwrap_err();

    assert!(matches!(err, AppError::Conflict(_)), "got {err:?}");
    assert!(prober.calls().is_empty());
}

#[tokio::test]
async fn probing_an_unknown_device_is_a_not_found() {
    let h = build().await;

    let err = as_admin(h.svc.probe_device(&uuid::Uuid::new_v4().to_string()))
        .await
        .unwrap_err();

    assert!(matches!(err, AppError::NotFound(_)), "got {err:?}");
}

#[tokio::test]
async fn the_daemon_will_not_probe_itself() {
    // `is_private_ip` counts loopback as private — it exists for the DDNS and
    // rebinding guards, where the question is "is this publishable". Here a
    // loopback `last_ip` would mean probing the Pi's own listeners.
    for address in ["127.0.0.1", "0.0.0.0", "::1"] {
        let prober = MockDeviceProber::answering(&[6668]);
        let h = build_with_prober(prober.clone()).await;
        let id =
            insert_device_at(&h.devices, "aa:bb:cc:dd:ee:01", address, chrono::Utc::now()).await;

        let err = as_admin(h.svc.probe_device(&id)).await.unwrap_err();

        assert!(
            matches!(err, AppError::Conflict(_)),
            "{address} should be refused, got {err:?}"
        );
        assert!(prober.calls().is_empty(), "{address} must not be contacted");
    }
}
