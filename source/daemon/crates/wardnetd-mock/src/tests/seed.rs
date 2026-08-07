//! Integration test for [`crate::seed::populate`].
//!
//! Builds an in-memory `SQLite` pool via the production `init_pool_from_connection_string`
//! helper, runs `populate`, then asserts that the expected counts of devices,
//! tunnels, blocklists, allowlist entries, and custom rules are present.

use wardnetd_data::{
    RepositoryFactory, SqliteRepositoryFactory, db::init_pool_from_connection_string,
};

use wardnet_common::device::{DeviceSignalKind, ManufacturerSource};

use crate::seed::populate;

#[tokio::test]
async fn populate_inserts_expected_demo_data() {
    let pool = init_pool_from_connection_string(":memory:")
        .await
        .expect("in-memory pool should initialise");

    let factory: Box<dyn RepositoryFactory> = Box::new(SqliteRepositoryFactory::from_pool(
        pool,
        std::path::PathBuf::from(":memory:"),
    ));
    let ids = populate(factory.as_ref())
        .await
        .expect("populate should succeed");

    assert_eq!(ids.device_ids.len(), 13, "should seed 13 devices");
    assert_eq!(ids.tunnel_ids.len(), 3, "should seed 3 tunnels");

    // Verify via repository reads.
    let devices = factory.device().find_all().await.unwrap();
    assert_eq!(devices.len(), 13);

    // The seed must cover every manufacturer state the UI renders, or local
    // dev never exercises them (issue #1099).
    let by_mac = |mac: &str| {
        devices
            .iter()
            .find(|d| d.mac == mac)
            .unwrap_or_else(|| panic!("seed should contain {mac}"))
            .clone()
    };

    // A public IEEE registrant, stated as fact.
    let ieee = by_mac("a8:bb:cc:11:22:01");
    assert_eq!(ieee.manufacturer.as_deref(), Some("Apple Inc."));
    assert_eq!(ieee.manufacturer_source, Some(ManufacturerSource::Ieee));
    assert!(
        !ieee.is_randomized,
        "demo MACs must be universally administered — 0xAA would set the \
         locally-administered bit and flag every device a privacy MAC"
    );

    // A block the IEEE lists as `Private`, named only by the curated catalog
    // and therefore rendered as a hedge.
    let catalog = by_mac("5c:e7:53:4e:ec:d9");
    assert_eq!(catalog.manufacturer.as_deref(), Some("Govee"));
    assert_eq!(
        catalog.manufacturer_source,
        Some(ManufacturerSource::Catalog)
    );

    // The Govee lamp carries the observations behind its hedged name, so the
    // device detail view has a populated signals section in local dev — and,
    // just as importantly, every other seeded device has none, which is the
    // empty state the same view has to handle (issue #1099).
    let govee_signals = factory
        .device_identification()
        .find_by_device(&catalog.id.to_string())
        .await
        .unwrap();
    let mdns = govee_signals
        .iter()
        .find(|s| s.kind == DeviceSignalKind::MdnsService)
        .expect("seed should record the Govee mDNS service");
    assert_eq!(mdns.value, "_govee._tcp");
    assert!(
        mdns.inferred,
        "a catalogued service type must be flagged as a vendor-list match"
    );
    assert!(
        govee_signals
            .iter()
            .any(|s| s.kind == DeviceSignalKind::DhcpHostname && !s.inferred),
        "a plain DHCP hostname must not be marked inferred"
    );

    let laptop_signals = factory
        .device_identification()
        .find_by_device(&ieee.id.to_string())
        .await
        .unwrap();
    assert!(
        laptop_signals.is_empty(),
        "most seeded devices must have no signals, so the empty state is reachable in local dev"
    );

    // A privacy MAC: flagged, and with no manufacturer invented for it.
    let randomized = by_mac("02:1a:2b:3c:4d:5e");
    assert!(randomized.is_randomized);
    assert_eq!(randomized.manufacturer, None);
    assert_eq!(randomized.manufacturer_source, None);

    let tunnels = factory.tunnel().find_all().await.unwrap();
    assert_eq!(tunnels.len(), 3);

    // After issue #221 the filter sources are profile-scoped — assert the
    // demo allowlist + custom rule landed in the Ad Blocking builtin profile.
    let ad_blocking_id: uuid::Uuid = "00000000-0000-0000-0000-000000000100".parse().unwrap();
    let dns_filter_repo = factory.dns_filter();
    let blocklists = dns_filter_repo
        .list_blocklists(ad_blocking_id)
        .await
        .unwrap();
    // The legacy DNS migration seeded two disabled blocklists; the Stage 7
    // migration backfills them into the Ad Blocking profile. seed() adds none.
    assert_eq!(blocklists.len(), 2);
    assert!(
        blocklists.iter().all(|b| !b.enabled),
        "seeded blocklists should be disabled so no HTTP fetch is scheduled"
    );

    let allowlist = dns_filter_repo
        .list_allowlist(ad_blocking_id)
        .await
        .unwrap();
    assert_eq!(allowlist.len(), 1);

    let custom_rules = dns_filter_repo
        .list_custom_rules(ad_blocking_id)
        .await
        .unwrap();
    assert_eq!(custom_rules.len(), 1);
}

#[tokio::test]
async fn populate_routing_rule_references_first_device_and_tunnel() {
    let pool = init_pool_from_connection_string(":memory:").await.unwrap();
    let factory: Box<dyn RepositoryFactory> = Box::new(SqliteRepositoryFactory::from_pool(
        pool,
        std::path::PathBuf::from(":memory:"),
    ));
    let ids = populate(factory.as_ref()).await.unwrap();

    let first_device_id = ids.device_ids.first().expect("at least one device");
    let rule = factory
        .device()
        .find_rule_for_device(&first_device_id.to_string())
        .await
        .unwrap();

    assert!(rule.is_some(), "first device should have a routing rule");
}
