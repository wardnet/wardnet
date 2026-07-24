//! Tests for [`SqliteRoutingProfileRepository`] against a fresh in-memory DB.
//!
//! Exercises profile/rule CRUD, the tunnel/direct target roundtrip, the
//! ordered device→profile assignment, and FK cascade on profile/device delete.

use uuid::Uuid;

use super::test_pool;
use crate::repository::SqliteRoutingProfileRepository;
use crate::repository::routing_profile::{
    RoutingProfileRepository, RoutingProfileRow, RoutingProfileUpdate, RoutingRuleRow,
    RoutingRuleUpdate,
};
use wardnet_common::routing_profile::DomainRoutingTarget;

async fn insert_device(pool: &sqlx::SqlitePool, id: &str, ip: &str) {
    sqlx::query(
        "INSERT INTO devices (id, mac, last_ip, device_type, first_seen, last_seen, zone_id) \
         VALUES (?, ?, ?, 'unknown', ?, ?, '00000000-0000-0000-0000-000000000201')",
    )
    .bind(id)
    .bind(format!(
        "aa:bb:cc:dd:ee:{:02x}",
        u32::from(id.as_bytes()[35]) & 0xff
    ))
    .bind(ip)
    .bind("2026-07-20T00:00:00Z")
    .bind("2026-07-20T00:00:00Z")
    .execute(pool)
    .await
    .unwrap();
}

async fn insert_tunnel(pool: &sqlx::SqlitePool, id: &str, interface: &str) {
    sqlx::query(
        "INSERT INTO tunnels (id, label, country_code, interface_name, endpoint) \
         VALUES (?, 'test', 'US', ?, '1.2.3.4:51820')",
    )
    .bind(id)
    .bind(interface)
    .execute(pool)
    .await
    .unwrap();
}

fn profile_row(name: &str) -> RoutingProfileRow {
    RoutingProfileRow {
        id: Uuid::new_v4().to_string(),
        name: name.to_owned(),
    }
}

// ── Profiles ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn create_list_update_delete_profile() {
    let repo = SqliteRoutingProfileRepository::new(test_pool().await);

    let created = repo
        .create_profile(&profile_row("Streaming"))
        .await
        .unwrap();
    assert_eq!(created.name, "Streaming");

    let all = repo.list_profiles().await.unwrap();
    assert_eq!(all.len(), 1);

    let updated = repo
        .update_profile(
            created.id,
            &RoutingProfileUpdate {
                name: Some("Streaming UK".to_owned()),
            },
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(updated.name, "Streaming UK");

    assert!(repo.delete_profile(created.id).await.unwrap());
    assert!(repo.list_profiles().await.unwrap().is_empty());
    // Deleting again reports no row.
    assert!(!repo.delete_profile(created.id).await.unwrap());
}

#[tokio::test]
async fn update_missing_profile_returns_none() {
    let repo = SqliteRoutingProfileRepository::new(test_pool().await);
    let out = repo
        .update_profile(
            Uuid::new_v4(),
            &RoutingProfileUpdate {
                name: Some("nope".to_owned()),
            },
        )
        .await
        .unwrap();
    assert!(out.is_none());
}

// ── Rules ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn rule_crud_with_tunnel_and_direct_targets() {
    let pool = test_pool().await;
    let repo = SqliteRoutingProfileRepository::new(pool.clone());
    let profile = repo.create_profile(&profile_row("P")).await.unwrap();
    let tunnel_id = Uuid::new_v4();
    insert_tunnel(&pool, &tunnel_id.to_string(), "wg-rulecrud0").await;

    let tunnel_rule = repo
        .create_rule(&RoutingRuleRow {
            id: Uuid::new_v4().to_string(),
            profile_id: profile.id,
            pattern: "*.netflix.com".to_owned(),
            target: DomainRoutingTarget::Tunnel { tunnel_id },
            enabled: true,
        })
        .await
        .unwrap();
    assert_eq!(
        tunnel_rule.target,
        DomainRoutingTarget::Tunnel { tunnel_id }
    );

    let direct_rule = repo
        .create_rule(&RoutingRuleRow {
            id: Uuid::new_v4().to_string(),
            profile_id: profile.id,
            pattern: "bank.example.com".to_owned(),
            target: DomainRoutingTarget::Direct,
            enabled: true,
        })
        .await
        .unwrap();
    assert_eq!(direct_rule.target, DomainRoutingTarget::Direct);

    // Both targets survive a read.
    let listed = repo.list_rules(profile.id).await.unwrap();
    assert_eq!(listed.len(), 2);
    assert_eq!(repo.list_all_rules().await.unwrap().len(), 2);

    // Flip the tunnel rule to direct and disable it.
    let updated = repo
        .update_rule(
            tunnel_rule.id,
            &RoutingRuleUpdate {
                target: Some(DomainRoutingTarget::Direct),
                enabled: Some(false),
                ..Default::default()
            },
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(updated.target, DomainRoutingTarget::Direct);
    assert!(!updated.enabled);
    assert_eq!(updated.pattern, "*.netflix.com");

    assert!(repo.delete_rule(direct_rule.id).await.unwrap());
    assert_eq!(repo.list_rules(profile.id).await.unwrap().len(), 1);
}

#[tokio::test]
async fn tunnel_rule_cascades_on_tunnel_delete() {
    let pool = test_pool().await;
    let repo = SqliteRoutingProfileRepository::new(pool.clone());
    let profile = repo.create_profile(&profile_row("P")).await.unwrap();
    let tunnel_id = Uuid::new_v4();
    insert_tunnel(&pool, &tunnel_id.to_string(), "wg-cascade0").await;

    repo.create_rule(&RoutingRuleRow {
        id: Uuid::new_v4().to_string(),
        profile_id: profile.id,
        pattern: "*.example.com".to_owned(),
        target: DomainRoutingTarget::Tunnel { tunnel_id },
        enabled: true,
    })
    .await
    .unwrap();
    assert_eq!(repo.list_rules(profile.id).await.unwrap().len(), 1);

    // Deleting the tunnel cascades its now-meaningless routing rule away, but
    // the profile itself survives.
    sqlx::query("DELETE FROM tunnels WHERE id = ?")
        .bind(tunnel_id.to_string())
        .execute(&pool)
        .await
        .unwrap();
    assert!(repo.list_rules(profile.id).await.unwrap().is_empty());
    assert!(repo.get_profile(profile.id).await.unwrap().is_some());
}

#[tokio::test]
async fn duplicate_pattern_in_profile_is_rejected() {
    let repo = SqliteRoutingProfileRepository::new(test_pool().await);
    let profile = repo.create_profile(&profile_row("P")).await.unwrap();
    let row = |pattern: &str| RoutingRuleRow {
        id: Uuid::new_v4().to_string(),
        profile_id: profile.id,
        pattern: pattern.to_owned(),
        target: DomainRoutingTarget::Direct,
        enabled: true,
    };
    repo.create_rule(&row("dup.com")).await.unwrap();
    assert!(repo.create_rule(&row("dup.com")).await.is_err());
}

#[tokio::test]
async fn deleting_profile_cascades_rules() {
    let repo = SqliteRoutingProfileRepository::new(test_pool().await);
    let profile = repo.create_profile(&profile_row("P")).await.unwrap();
    repo.create_rule(&RoutingRuleRow {
        id: Uuid::new_v4().to_string(),
        profile_id: profile.id,
        pattern: "a.com".to_owned(),
        target: DomainRoutingTarget::Direct,
        enabled: true,
    })
    .await
    .unwrap();

    assert!(repo.delete_profile(profile.id).await.unwrap());
    assert!(repo.list_all_rules().await.unwrap().is_empty());
}

// ── Device assignment (ordered) ──────────────────────────────────────────────

#[tokio::test]
async fn device_profiles_preserve_priority_order() {
    let pool = test_pool().await;
    let dev = "00000000-0000-0000-0000-0000000000d0";
    insert_device(&pool, dev, "192.168.1.50").await;
    let repo = SqliteRoutingProfileRepository::new(pool);
    let device_id: Uuid = dev.parse().unwrap();

    let p1 = repo.create_profile(&profile_row("first")).await.unwrap();
    let p2 = repo.create_profile(&profile_row("second")).await.unwrap();
    let p3 = repo.create_profile(&profile_row("third")).await.unwrap();

    // Assign in a deliberate non-alphabetical order.
    repo.set_device_profiles(device_id, &[p3.id, p1.id, p2.id])
        .await
        .unwrap();
    assert_eq!(
        repo.get_device_profiles(device_id).await.unwrap(),
        vec![p3.id, p1.id, p2.id]
    );

    // Re-assigning replaces atomically and re-orders.
    repo.set_device_profiles(device_id, &[p2.id, p3.id])
        .await
        .unwrap();
    assert_eq!(
        repo.get_device_profiles(device_id).await.unwrap(),
        vec![p2.id, p3.id]
    );

    // Clearing removes all rows.
    repo.set_device_profiles(device_id, &[]).await.unwrap();
    assert!(
        repo.get_device_profiles(device_id)
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn list_assignments_orders_by_position() {
    let pool = test_pool().await;
    let dev = "00000000-0000-0000-0000-0000000000e0";
    insert_device(&pool, dev, "10.0.0.9").await;
    let repo = SqliteRoutingProfileRepository::new(pool);
    let device_id: Uuid = dev.parse().unwrap();

    let p1 = repo.create_profile(&profile_row("a")).await.unwrap();
    let p2 = repo.create_profile(&profile_row("b")).await.unwrap();
    repo.set_device_profiles(device_id, &[p2.id, p1.id])
        .await
        .unwrap();

    let assignments = repo.list_device_assignments().await.unwrap();
    assert_eq!(assignments.len(), 1);
    assert_eq!(assignments[0].device_id, device_id);
    assert_eq!(assignments[0].profile_ids, vec![p2.id, p1.id]);
}

#[tokio::test]
async fn assignment_cascades_on_device_delete() {
    let pool = test_pool().await;
    let dev = "00000000-0000-0000-0000-0000000000f0";
    insert_device(&pool, dev, "10.0.0.10").await;
    let repo = SqliteRoutingProfileRepository::new(pool.clone());
    let device_id: Uuid = dev.parse().unwrap();

    let p1 = repo.create_profile(&profile_row("a")).await.unwrap();
    repo.set_device_profiles(device_id, &[p1.id]).await.unwrap();

    sqlx::query("DELETE FROM devices WHERE id = ?")
        .bind(dev)
        .execute(&pool)
        .await
        .unwrap();

    // The assignment row is gone; the profile itself survives.
    assert!(
        repo.get_device_profiles(device_id)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(repo.list_profiles().await.unwrap().len(), 1);
}

#[tokio::test]
async fn list_profile_devices_returns_assigned_devices() {
    let pool = test_pool().await;
    let dev_a = "00000000-0000-0000-0000-00000000ba01";
    let dev_b = "00000000-0000-0000-0000-00000000ba02";
    insert_device(&pool, dev_a, "10.0.0.21").await;
    insert_device(&pool, dev_b, "10.0.0.22").await;
    let repo = SqliteRoutingProfileRepository::new(pool);
    let device_a: Uuid = dev_a.parse().unwrap();
    let device_b: Uuid = dev_b.parse().unwrap();

    let p1 = repo.create_profile(&profile_row("shared")).await.unwrap();
    let p2 = repo.create_profile(&profile_row("lonely")).await.unwrap();

    // Both devices reference p1; only device_a references p2.
    repo.set_device_profiles(device_a, &[p1.id, p2.id])
        .await
        .unwrap();
    repo.set_device_profiles(device_b, &[p1.id]).await.unwrap();

    let mut p1_devices = repo.list_profile_devices(p1.id).await.unwrap();
    p1_devices.sort();
    let mut expected = vec![device_a, device_b];
    expected.sort();
    assert_eq!(p1_devices, expected);

    assert_eq!(
        repo.list_profile_devices(p2.id).await.unwrap(),
        vec![device_a]
    );

    // A profile with no assignments reverses to an empty list.
    let p3 = repo.create_profile(&profile_row("unused")).await.unwrap();
    assert!(repo.list_profile_devices(p3.id).await.unwrap().is_empty());
}
