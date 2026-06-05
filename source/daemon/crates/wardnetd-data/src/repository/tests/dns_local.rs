//! Tests for [`SqliteDnsLocalRepository`] against a fresh in-memory DB.
//!
//! The migration seeds one `.lan` zone (id `…0010`); these tests exercise
//! CRUD round-trips for zones, custom records, and conditional forwarding
//! rules, plus the `UNIQUE(domain, record_type)` constraint and the
//! `ON DELETE SET NULL` behaviour of `dns_custom_records.zone_id`.

use uuid::Uuid;

use super::test_pool;
use crate::repository::SqliteDnsLocalRepository;
use crate::repository::dns_local::{
    DnsLocalRepository, RecordRow, RecordUpdate, RuleRow, RuleUpdate, UpsertRecordRow, ZoneRow,
    ZoneUpdate,
};
use wardnet_common::dns::{DnsRecordSource, DnsRecordType};

const SEEDED_LAN_ZONE: &str = "00000000-0000-0000-0000-000000000010";

fn new_repo(pool: sqlx::SqlitePool) -> SqliteDnsLocalRepository {
    SqliteDnsLocalRepository::new(pool)
}

// ── Migration / seed ──────────────────────────────────────────────────────

#[tokio::test]
async fn migration_seeds_lan_zone() {
    let repo = new_repo(test_pool().await);

    let zones = repo.list_zones().await.unwrap();
    let lan = zones
        .iter()
        .find(|z| z.name == "lan")
        .expect("seeded .lan zone present");
    assert_eq!(lan.id.to_string(), SEEDED_LAN_ZONE);
    assert!(lan.enabled);
}

// ── Zones ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn zone_crud_round_trip() {
    let repo = new_repo(test_pool().await);
    let id = Uuid::new_v4();

    let created = repo
        .create_zone(&ZoneRow {
            id: id.to_string(),
            name: "lab".to_owned(),
            enabled: true,
        })
        .await
        .unwrap();
    assert_eq!(created.id, id);
    assert_eq!(created.name, "lab");
    assert!(created.enabled);

    let fetched = repo.get_zone(id).await.unwrap().expect("zone exists");
    assert_eq!(fetched.name, "lab");

    let updated = repo
        .update_zone(
            id,
            &ZoneUpdate {
                name: Some("homelab".to_owned()),
                enabled: Some(false),
            },
        )
        .await
        .unwrap()
        .expect("updated zone returned");
    assert_eq!(updated.name, "homelab");
    assert!(!updated.enabled);

    assert!(repo.delete_zone(id).await.unwrap());
    assert!(repo.get_zone(id).await.unwrap().is_none());
}

#[tokio::test]
async fn delete_missing_zone_returns_false() {
    let repo = new_repo(test_pool().await);
    assert!(!repo.delete_zone(Uuid::new_v4()).await.unwrap());
}

// ── Records ───────────────────────────────────────────────────────────────

fn record_row(id: Uuid, zone_id: Option<Uuid>, domain: &str) -> RecordRow {
    RecordRow {
        id: id.to_string(),
        zone_id,
        domain: domain.to_owned(),
        record_type: DnsRecordType::A,
        value: "10.0.0.1".to_owned(),
        ttl: 300,
        enabled: true,
        source: DnsRecordSource::Manual,
    }
}

#[tokio::test]
async fn record_crud_round_trip() {
    let repo = new_repo(test_pool().await);
    let zone_id: Uuid = SEEDED_LAN_ZONE.parse().unwrap();
    let id = Uuid::new_v4();

    let created = repo
        .create_record(&record_row(id, Some(zone_id), "nas.lan"))
        .await
        .unwrap();
    assert_eq!(created.id, id);
    assert_eq!(created.zone_id, Some(zone_id));
    assert_eq!(created.record_type, DnsRecordType::A);
    assert_eq!(created.ttl, 300);

    let fetched = repo.get_record(id).await.unwrap().expect("record exists");
    assert_eq!(fetched.domain, "nas.lan");

    assert!(
        repo.list_records()
            .await
            .unwrap()
            .iter()
            .any(|r| r.id == id)
    );
    assert!(
        repo.list_records_by_zone(zone_id)
            .await
            .unwrap()
            .iter()
            .any(|r| r.id == id)
    );

    let updated = repo
        .update_record(
            id,
            &RecordUpdate {
                record_type: Some(DnsRecordType::Aaaa),
                value: Some("fd00::1".to_owned()),
                ttl: Some(600),
                enabled: Some(false),
                ..Default::default()
            },
        )
        .await
        .unwrap()
        .expect("updated record returned");
    assert_eq!(updated.record_type, DnsRecordType::Aaaa);
    assert_eq!(updated.value, "fd00::1");
    assert_eq!(updated.ttl, 600);
    assert!(!updated.enabled);

    assert!(repo.delete_record(id).await.unwrap());
    assert!(repo.get_record(id).await.unwrap().is_none());
}

#[tokio::test]
async fn duplicate_domain_record_type_rejected() {
    let repo = new_repo(test_pool().await);
    let zone_id: Uuid = SEEDED_LAN_ZONE.parse().unwrap();

    repo.create_record(&record_row(Uuid::new_v4(), Some(zone_id), "dup.lan"))
        .await
        .unwrap();

    // Same (domain, record_type) → violates UNIQUE index.
    let err = repo
        .create_record(&record_row(Uuid::new_v4(), Some(zone_id), "dup.lan"))
        .await;
    assert!(err.is_err());
}

#[tokio::test]
async fn deleting_zone_nulls_record_zone_id() {
    let pool = test_pool().await;
    // `ON DELETE SET NULL` only fires with FK enforcement on the connection;
    // `test_pool` does not enable it by default (see `fk_cascade_on_device_delete`).
    sqlx::query("PRAGMA foreign_keys = ON")
        .execute(&pool)
        .await
        .unwrap();
    let repo = new_repo(pool);
    let zone_id = Uuid::new_v4();
    repo.create_zone(&ZoneRow {
        id: zone_id.to_string(),
        name: "tmp".to_owned(),
        enabled: true,
    })
    .await
    .unwrap();

    let record_id = Uuid::new_v4();
    repo.create_record(&record_row(record_id, Some(zone_id), "host.tmp"))
        .await
        .unwrap();

    assert!(repo.delete_zone(zone_id).await.unwrap());

    // Record survives, with its zone link cleared (ON DELETE SET NULL).
    let fetched = repo.get_record(record_id).await.unwrap().expect("survives");
    assert_eq!(fetched.zone_id, None);
    assert!(repo.list_records_by_zone(zone_id).await.unwrap().is_empty());
}

#[tokio::test]
async fn update_record_can_clear_zone_id() {
    let repo = new_repo(test_pool().await);
    let zone_id: Uuid = SEEDED_LAN_ZONE.parse().unwrap();
    let id = Uuid::new_v4();
    repo.create_record(&record_row(id, Some(zone_id), "clear.lan"))
        .await
        .unwrap();

    let updated = repo
        .update_record(
            id,
            &RecordUpdate {
                zone_id: Some(None),
                ..Default::default()
            },
        )
        .await
        .unwrap()
        .expect("updated record returned");
    assert_eq!(updated.zone_id, None);
}

#[tokio::test]
async fn delete_missing_record_returns_false() {
    let repo = new_repo(test_pool().await);
    assert!(!repo.delete_record(Uuid::new_v4()).await.unwrap());
}

// ── Upsert (DHCP .lan auto-registration) ────────────────────────────────────

fn upsert_row(value: &str) -> UpsertRecordRow {
    let zone_id: Uuid = SEEDED_LAN_ZONE.parse().unwrap();
    UpsertRecordRow {
        zone_id: Some(zone_id),
        value: value.to_owned(),
        ttl: 600,
        enabled: true,
        source: DnsRecordSource::Dhcp,
    }
}

#[tokio::test]
async fn upsert_record_inserts_on_first_call() {
    let repo = new_repo(test_pool().await);

    let record = repo
        .upsert_record("mypc.lan", DnsRecordType::A, &upsert_row("10.0.0.5"))
        .await
        .unwrap()
        .expect("insert returns the row");

    assert_eq!(record.domain, "mypc.lan");
    assert_eq!(record.value, "10.0.0.5");
    assert_eq!(record.record_type, DnsRecordType::A);
    assert_eq!(record.source, DnsRecordSource::Dhcp);

    let fetched = repo.get_record(record.id).await.unwrap().expect("exists");
    assert_eq!(fetched.value, "10.0.0.5");
}

#[tokio::test]
async fn upsert_record_updates_value_on_conflict() {
    let repo = new_repo(test_pool().await);

    repo.upsert_record("mypc.lan", DnsRecordType::A, &upsert_row("10.0.0.5"))
        .await
        .unwrap()
        .expect("insert returns the row");
    let updated = repo
        .upsert_record("mypc.lan", DnsRecordType::A, &upsert_row("10.0.0.9"))
        .await
        .unwrap()
        .expect("same-source conflict updates and returns the row");

    assert_eq!(updated.value, "10.0.0.9");
    // Still a single row for that (domain, record_type).
    let all = repo.list_records().await.unwrap();
    let matching: Vec<_> = all
        .iter()
        .filter(|r| r.domain == "mypc.lan" && r.record_type == DnsRecordType::A)
        .collect();
    assert_eq!(matching.len(), 1);
}

#[tokio::test]
async fn upsert_record_preserves_id_on_conflict() {
    let repo = new_repo(test_pool().await);

    let first = repo
        .upsert_record("mypc.lan", DnsRecordType::A, &upsert_row("10.0.0.5"))
        .await
        .unwrap()
        .expect("insert returns the row");
    let second = repo
        .upsert_record("mypc.lan", DnsRecordType::A, &upsert_row("10.0.0.9"))
        .await
        .unwrap()
        .expect("same-source conflict updates and returns the row");

    assert_eq!(
        first.id, second.id,
        "conflicting upsert must keep the existing row id"
    );
    assert_eq!(first.created_at, second.created_at);
}

#[tokio::test]
async fn upsert_record_source_stored_and_returned() {
    let repo = new_repo(test_pool().await);

    let record = repo
        .upsert_record("nas.lan", DnsRecordType::A, &upsert_row("10.0.0.7"))
        .await
        .unwrap()
        .expect("insert returns the row");
    assert_eq!(record.source, DnsRecordSource::Dhcp);

    // Survives a round-trip through the read path.
    let fetched = repo.get_record(record.id).await.unwrap().expect("exists");
    assert_eq!(fetched.source, DnsRecordSource::Dhcp);
}

#[tokio::test]
async fn upsert_record_does_not_clobber_other_source() {
    let repo = new_repo(test_pool().await);
    let zone_id: Uuid = SEEDED_LAN_ZONE.parse().unwrap();

    // Admin creates a manual record for `nas.lan`.
    repo.create_record(&record_row(Uuid::new_v4(), Some(zone_id), "nas.lan"))
        .await
        .unwrap();

    // A DHCP upsert for the same name must NOT overwrite the manual record.
    let blocked = repo
        .upsert_record("nas.lan", DnsRecordType::A, &upsert_row("66.66.66.66"))
        .await
        .unwrap();
    assert!(
        blocked.is_none(),
        "DHCP upsert must not clobber a manual record"
    );

    // The original manual record is untouched.
    let all = repo.list_records().await.unwrap();
    let existing = all
        .iter()
        .find(|r| r.domain == "nas.lan" && r.record_type == DnsRecordType::A)
        .expect("manual record still present");
    assert_eq!(existing.value, "10.0.0.1");
    assert_eq!(existing.source, DnsRecordSource::Manual);
}

// ── Conditional forwarding rules ────────────────────────────────────────────

#[tokio::test]
async fn rule_crud_round_trip() {
    let repo = new_repo(test_pool().await);
    let id = Uuid::new_v4();

    let created = repo
        .create_rule(&RuleRow {
            id: id.to_string(),
            domain: "corp.example".to_owned(),
            upstream: "10.0.0.53".to_owned(),
            enabled: true,
        })
        .await
        .unwrap();
    assert_eq!(created.id, id);
    assert_eq!(created.domain, "corp.example");

    let fetched = repo.get_rule(id).await.unwrap().expect("rule exists");
    assert_eq!(fetched.upstream, "10.0.0.53");
    assert!(repo.list_rules().await.unwrap().iter().any(|r| r.id == id));

    let updated = repo
        .update_rule(
            id,
            &RuleUpdate {
                upstream: Some("10.0.0.54".to_owned()),
                enabled: Some(false),
                ..Default::default()
            },
        )
        .await
        .unwrap()
        .expect("updated rule returned");
    assert_eq!(updated.upstream, "10.0.0.54");
    assert!(!updated.enabled);

    assert!(repo.delete_rule(id).await.unwrap());
    assert!(repo.get_rule(id).await.unwrap().is_none());
}

#[tokio::test]
async fn delete_missing_rule_returns_false() {
    let repo = new_repo(test_pool().await);
    assert!(!repo.delete_rule(Uuid::new_v4()).await.unwrap());
}
