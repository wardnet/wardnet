//! Integration test for [`crate::seed::populate`].
//!
//! Builds an in-memory `SQLite` pool via the production `init_pool_from_connection_string`
//! helper, runs `populate`, then asserts that the expected counts of devices,
//! tunnels, blocklists, allowlist entries, and custom rules are present.

use wardnetd_data::{
    RepositoryFactory, SqliteRepositoryFactory, db::init_pool_from_connection_string,
};

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

    assert_eq!(ids.device_ids.len(), 5, "should seed 5 devices");
    assert_eq!(ids.tunnel_ids.len(), 2, "should seed 2 tunnels");

    // Verify via repository reads.
    let devices = factory.device().find_all().await.unwrap();
    assert_eq!(devices.len(), 5);

    let tunnels = factory.tunnel().find_all().await.unwrap();
    assert_eq!(tunnels.len(), 2);

    let blocklists = factory.dns().list_blocklists().await.unwrap();
    // Migrations seed two default blocklists (both disabled); seed() adds none.
    assert_eq!(blocklists.len(), 2);
    assert!(
        blocklists.iter().all(|b| !b.enabled),
        "seeded blocklists should be disabled so no HTTP fetch is scheduled"
    );

    let allowlist = factory.dns().list_allowlist().await.unwrap();
    assert_eq!(allowlist.len(), 1);

    let custom_rules = factory.dns().list_custom_rules().await.unwrap();
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
