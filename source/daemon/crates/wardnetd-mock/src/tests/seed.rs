//! Integration test for [`crate::seed::populate`].
//!
//! Builds an in-memory `SQLite` pool via the production `init_pool_from_connection_string`
//! helper, runs `populate`, then asserts that the expected counts of devices,
//! tunnels, blocklists (including the seeded failing one), allowlist
//! entries, and custom rules are present.

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
    assert_seeded_blocklists(&blocklists);

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

/// Assert the blocklist state the `blocklist_refresh_failing` anomaly needs.
///
/// Split out of the main body because these facts belong together — change
/// any one of them and the mock silently stops showing the anomaly, which is
/// the whole reason the failing list is seeded.
fn assert_seeded_blocklists(blocklists: &[wardnet_common::dns::Blocklist]) {
    // The legacy DNS migration seeded two disabled blocklists; the Stage 7
    // migration backfills them into the Ad Blocking profile. seed() adds one
    // more: the failing list.
    assert_eq!(blocklists.len(), 3);
    assert_eq!(
        blocklists.iter().filter(|b| !b.enabled).count(),
        2,
        "the two migration-seeded blocklists must stay disabled so no HTTP fetch is scheduled"
    );

    // The detector reports only a list that is *enabled* and already past the
    // alert threshold (default 5), so both halves have to hold.
    let failing = blocklists
        .iter()
        .find(|b| b.enabled)
        .expect("seed should add one enabled blocklist");
    assert!(
        failing.consecutive_failures >= 5,
        "the seeded blocklist must be past the default alert threshold, got {}",
        failing.consecutive_failures
    );
    assert!(
        failing.url.contains(".invalid"),
        "the seeded failing blocklist must point at an unresolvable host so nothing fetches it"
    );
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

// ── The household directory (ADR-0031, #1147) ───────────────────────────────
//
// Seeded separately from `populate`, and *after* the setup wizard has created
// the first admin — `setup_admin` refuses once any user row exists, so seeding
// a directory at boot would silently switch off the one flow this mock exists
// to let a developer replay.

use crate::seed::{DEMO_ENROLMENT_TOKEN, populate_household};
use wardnet_common::auth::UserRole;
use wardnetd_services::auth::password::hash_token;

/// A factory with the real migrations applied, plus the demo devices.
async fn seeded_factory() -> (Box<dyn RepositoryFactory>, Vec<uuid::Uuid>) {
    let pool = init_pool_from_connection_string(":memory:")
        .await
        .expect("in-memory pool should initialise");
    let factory: Box<dyn RepositoryFactory> = Box::new(SqliteRepositoryFactory::from_pool(
        pool,
        std::path::PathBuf::from(":memory:"),
    ));
    let ids = populate(factory.as_ref()).await.expect("populate");
    (factory, ids.device_ids)
}

async fn run_household(factory: &dyn RepositoryFactory, device_ids: &[uuid::Uuid]) {
    populate_household(
        factory.user().as_ref(),
        factory.user_enrolment().as_ref(),
        factory.device().as_ref(),
        device_ids,
    )
    .await
    .expect("populate_household");
}

#[tokio::test]
async fn populate_household_seeds_three_credential_less_users() {
    let (factory, device_ids) = seeded_factory().await;
    run_household(factory.as_ref(), &device_ids).await;

    let users = factory.user().find_all().await.unwrap();
    assert_eq!(users.len(), 3);

    let by_name = |name: &str| {
        users
            .iter()
            .find(|u| u.display_name == name)
            .unwrap_or_else(|| panic!("{name} should be seeded"))
    };

    // One admin, one enabled member, one disabled member — so the directory
    // shows every state without an admin breaking their own account to see it.
    assert_eq!(by_name("Ana").role, UserRole::Admin);
    assert!(by_name("Ana").enabled);
    assert_eq!(by_name("Bruno").role, UserRole::Member);
    assert!(by_name("Bruno").enabled);
    assert!(!by_name("Cleo").enabled);
    // Cleo has no email: several household members legitimately have none, and
    // the unique index must tolerate more than one of them.
    assert_eq!(by_name("Cleo").email, None);

    // **No credentials.** An admin never learns a member's password, so a
    // member holding one nobody typed is a state the product cannot reach.
    for user in &users {
        let creds = factory
            .user_credential()
            .list_for_user(&user.id)
            .await
            .unwrap();
        assert!(
            creds.is_empty(),
            "{} must be seeded credential-less",
            user.display_name
        );
    }
}

#[tokio::test]
async fn populate_household_seeds_one_open_and_one_spent_invitation() {
    let (factory, device_ids) = seeded_factory().await;
    run_household(factory.as_ref(), &device_ids).await;

    let bruno = factory
        .user()
        .find_by_email("bruno@example.invalid")
        .await
        .unwrap()
        .expect("Bruno should be seeded");
    let rows = factory
        .user_enrolment()
        .list_for_user(&bruno.id)
        .await
        .unwrap();
    assert_eq!(rows.len(), 2);

    let open = rows.iter().filter(|r| r.used_at.is_none()).count();
    let spent = rows.iter().filter(|r| r.used_at.is_some()).count();
    assert_eq!((open, spent), (1, 1), "one of each, so the UI shows both");

    // The open one is genuinely redeemable with the documented token — only
    // its hash is stored, exactly as in production, so the mock does not
    // short-circuit the real single-use check.
    let hash = hash_token(DEMO_ENROLMENT_TOKEN);
    assert!(
        rows.iter()
            .any(|r| r.token_hash == hash && r.used_at.is_none()),
        "the advertised token must match the outstanding invitation"
    );
}

#[tokio::test]
async fn populate_household_assigns_two_device_owners() {
    let (factory, device_ids) = seeded_factory().await;
    run_household(factory.as_ref(), &device_ids).await;

    let devices = factory.device().find_all().await.unwrap();
    let with_owner: Vec<_> = devices
        .iter()
        .filter(|d| d.owner_user_id.is_some())
        .collect();
    // Two owned and the rest unowned, so the device screens show both states.
    assert_eq!(with_owner.len(), 2);

    let users = factory.user().find_all().await.unwrap();
    for device in with_owner {
        let assigned = device.owner_user_id.unwrap().to_string();
        assert!(
            users.iter().any(|u| u.id == assigned),
            "an owner must name a real user, not a dangling id"
        );
    }
}

#[tokio::test]
async fn populate_household_is_idempotent_for_on_disk_restarts() {
    // The waiting task fires as soon as *any* user exists, which on a
    // persisted database is the very first tick. Re-creating Ana would hit the
    // unique email index and abort the whole function, silently taking the
    // device owners and the demo invitation with it.
    let (factory, device_ids) = seeded_factory().await;
    run_household(factory.as_ref(), &device_ids).await;
    run_household(factory.as_ref(), &device_ids).await;

    assert_eq!(factory.user().find_all().await.unwrap().len(), 3);

    let bruno = factory
        .user()
        .find_by_email("bruno@example.invalid")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        factory
            .user_enrolment()
            .list_for_user(&bruno.id)
            .await
            .unwrap()
            .len(),
        2,
        "a second run must not duplicate the invitations"
    );
}

#[tokio::test]
async fn populate_household_tolerates_devices_that_have_gone() {
    // A miss is logged rather than fatal: the seed should still create the
    // directory even if the ids it was handed no longer resolve.
    let (factory, _) = seeded_factory().await;
    let ghosts = vec![uuid::Uuid::new_v4(), uuid::Uuid::new_v4()];
    run_household(factory.as_ref(), &ghosts).await;

    assert_eq!(factory.user().find_all().await.unwrap().len(), 3);
    let devices = factory.device().find_all().await.unwrap();
    assert!(devices.iter().all(|d| d.owner_user_id.is_none()));
}
