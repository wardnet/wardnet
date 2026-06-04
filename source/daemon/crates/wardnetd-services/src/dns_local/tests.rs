//! Service-layer tests for [`DnsLocalServiceImpl`].
//!
//! Builds a real `SqliteDnsLocalRepository` over an in-memory pool (same
//! migration set as the rest of the workspace) and exercises the CRUD
//! contract: round-trips, validation, the 404-on-missing guards, and the
//! admin auth gate.

use std::future::Future;
use std::sync::Arc;

use sqlx::SqlitePool;
use sqlx::sqlite::SqlitePoolOptions;
use uuid::Uuid;
use wardnet_common::api::{
    CreateForwardingRuleRequest, CreateRecordRequest, CreateZoneRequest,
    UpdateForwardingRuleRequest, UpdateRecordRequest, UpdateZoneRequest,
};
use wardnet_common::auth::AuthContext;
use wardnet_common::dns::DnsRecordType;
use wardnetd_data::repository::SqliteDnsLocalRepository;

use crate::auth_context;
use crate::dns_local::service::{DnsLocalService, DnsLocalServiceImpl};
use crate::error::AppError;

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

async fn build() -> DnsLocalServiceImpl {
    let pool = test_pool().await;
    let repo = Arc::new(SqliteDnsLocalRepository::new(pool));
    DnsLocalServiceImpl::new(repo)
}

/// Run a future inside an `Admin` task-local context. Every service method
/// opens with `require_admin()?`, so unscoped calls return `Forbidden`.
async fn as_admin<F: Future>(fut: F) -> F::Output {
    auth_context::with_context(
        AuthContext::Admin {
            admin_id: Uuid::nil(),
        },
        fut,
    )
    .await
}

fn create_zone_req(name: &str) -> CreateZoneRequest {
    CreateZoneRequest {
        name: name.to_owned(),
        enabled: true,
    }
}

fn create_record_req(zone_id: Option<Uuid>, domain: &str) -> CreateRecordRequest {
    CreateRecordRequest {
        zone_id,
        domain: domain.to_owned(),
        record_type: DnsRecordType::A,
        value: "192.168.1.10".to_owned(),
        ttl: 300,
        enabled: true,
    }
}

fn create_rule_req(domain: &str) -> CreateForwardingRuleRequest {
    CreateForwardingRuleRequest {
        domain: domain.to_owned(),
        upstream: "10.0.0.53".to_owned(),
        enabled: true,
    }
}

// ── Zones ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn create_and_get_zone_round_trips() {
    let svc = build().await;
    as_admin(async {
        let created = svc.create_zone(create_zone_req("lab")).await.unwrap();
        assert_eq!(created.zone.name, "lab");
        assert!(created.zone.enabled);

        let fetched = svc.get_zone(created.zone.id).await.unwrap();
        assert_eq!(fetched.zone.id, created.zone.id);
        assert_eq!(fetched.zone.name, "lab");
    })
    .await;
}

#[tokio::test]
async fn list_zones_includes_created() {
    let svc = build().await;
    as_admin(async {
        let before = svc.list_zones().await.unwrap().zones.len();
        svc.create_zone(create_zone_req("home")).await.unwrap();
        let after = svc.list_zones().await.unwrap().zones.len();
        assert_eq!(after, before + 1);
    })
    .await;
}

#[tokio::test]
async fn create_zone_rejects_empty_name() {
    let svc = build().await;
    as_admin(async {
        let err = svc.create_zone(create_zone_req("  ")).await.unwrap_err();
        assert!(matches!(err, AppError::BadRequest(_)));
    })
    .await;
}

#[tokio::test]
async fn get_zone_missing_is_not_found() {
    let svc = build().await;
    as_admin(async {
        let err = svc.get_zone(Uuid::new_v4()).await.unwrap_err();
        assert!(matches!(err, AppError::NotFound(_)));
    })
    .await;
}

#[tokio::test]
async fn update_zone_changes_fields() {
    let svc = build().await;
    as_admin(async {
        let created = svc.create_zone(create_zone_req("lab")).await.unwrap();
        let updated = svc
            .update_zone(
                created.zone.id,
                UpdateZoneRequest {
                    name: Some("lab2".to_owned()),
                    enabled: Some(false),
                },
            )
            .await
            .unwrap();
        assert_eq!(updated.zone.name, "lab2");
        assert!(!updated.zone.enabled);
    })
    .await;
}

/// The C1 repo returns `Ok(true)` from `update_zone` even for a missing id,
/// so the service MUST guard with an explicit existence check → 404.
#[tokio::test]
async fn update_zone_missing_is_not_found() {
    let svc = build().await;
    as_admin(async {
        let err = svc
            .update_zone(Uuid::new_v4(), UpdateZoneRequest::default())
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::NotFound(_)));
    })
    .await;
}

#[tokio::test]
async fn delete_zone_then_missing_is_not_found() {
    let svc = build().await;
    as_admin(async {
        let created = svc.create_zone(create_zone_req("lab")).await.unwrap();
        svc.delete_zone(created.zone.id).await.unwrap();
        let err = svc.delete_zone(created.zone.id).await.unwrap_err();
        assert!(matches!(err, AppError::NotFound(_)));
    })
    .await;
}

#[tokio::test]
async fn list_zone_records_missing_zone_is_not_found() {
    let svc = build().await;
    as_admin(async {
        let err = svc.list_zone_records(Uuid::new_v4()).await.unwrap_err();
        assert!(matches!(err, AppError::NotFound(_)));
    })
    .await;
}

// ── Records ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn create_record_round_trips_and_lists_by_zone() {
    let svc = build().await;
    as_admin(async {
        let zone = svc.create_zone(create_zone_req("lab")).await.unwrap();
        let created = svc
            .create_record(create_record_req(Some(zone.zone.id), "nas.lab"))
            .await
            .unwrap();
        assert_eq!(created.record.domain, "nas.lab");
        assert_eq!(created.record.zone_id, Some(zone.zone.id));

        let in_zone = svc.list_zone_records(zone.zone.id).await.unwrap();
        assert_eq!(in_zone.records.len(), 1);
        assert_eq!(in_zone.records[0].id, created.record.id);
    })
    .await;
}

#[tokio::test]
async fn create_record_rejects_domain_without_dot() {
    let svc = build().await;
    as_admin(async {
        let err = svc
            .create_record(create_record_req(None, "nodot"))
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::BadRequest(_)));
    })
    .await;
}

#[tokio::test]
async fn create_record_with_unknown_zone_is_not_found() {
    let svc = build().await;
    as_admin(async {
        let err = svc
            .create_record(create_record_req(Some(Uuid::new_v4()), "nas.lab"))
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::NotFound(_)));
    })
    .await;
}

#[tokio::test]
async fn update_record_clears_zone_link() {
    let svc = build().await;
    as_admin(async {
        let zone = svc.create_zone(create_zone_req("lab")).await.unwrap();
        let rec = svc
            .create_record(create_record_req(Some(zone.zone.id), "nas.lab"))
            .await
            .unwrap();

        // `Some(None)` clears the zone link to NULL.
        let updated = svc
            .update_record(
                rec.record.id,
                UpdateRecordRequest {
                    zone_id: Some(None),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        assert_eq!(updated.record.zone_id, None);
    })
    .await;
}

#[tokio::test]
async fn update_record_missing_is_not_found() {
    let svc = build().await;
    as_admin(async {
        let err = svc
            .update_record(Uuid::new_v4(), UpdateRecordRequest::default())
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::NotFound(_)));
    })
    .await;
}

#[tokio::test]
async fn delete_record_missing_is_not_found() {
    let svc = build().await;
    as_admin(async {
        let err = svc.delete_record(Uuid::new_v4()).await.unwrap_err();
        assert!(matches!(err, AppError::NotFound(_)));
    })
    .await;
}

// ── Forwarding rules ───────────────────────────────────────────────────

#[tokio::test]
async fn create_and_list_forwarding_rule() {
    let svc = build().await;
    as_admin(async {
        let created = svc
            .create_forwarding_rule(create_rule_req("corp.example.com"))
            .await
            .unwrap();
        assert_eq!(created.rule.domain, "corp.example.com");
        assert_eq!(created.rule.upstream, "10.0.0.53");

        let listed = svc.list_forwarding_rules().await.unwrap();
        assert!(listed.rules.iter().any(|r| r.id == created.rule.id));
    })
    .await;
}

#[tokio::test]
async fn create_forwarding_rule_rejects_empty_upstream() {
    let svc = build().await;
    as_admin(async {
        let mut req = create_rule_req("corp.example.com");
        req.upstream = "  ".to_owned();
        let err = svc.create_forwarding_rule(req).await.unwrap_err();
        assert!(matches!(err, AppError::BadRequest(_)));
    })
    .await;
}

#[tokio::test]
async fn update_forwarding_rule_missing_is_not_found() {
    let svc = build().await;
    as_admin(async {
        let err = svc
            .update_forwarding_rule(Uuid::new_v4(), UpdateForwardingRuleRequest::default())
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::NotFound(_)));
    })
    .await;
}

#[tokio::test]
async fn delete_forwarding_rule_missing_is_not_found() {
    let svc = build().await;
    as_admin(async {
        let err = svc
            .delete_forwarding_rule(Uuid::new_v4())
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::NotFound(_)));
    })
    .await;
}

// ── Duplicate (UNIQUE constraint) → 409 Conflict ───────────────────────

#[tokio::test]
async fn create_zone_duplicate_name_is_conflict() {
    let svc = build().await;
    as_admin(async {
        svc.create_zone(create_zone_req("lab")).await.unwrap();
        let err = svc.create_zone(create_zone_req("lab")).await.unwrap_err();
        assert!(matches!(err, AppError::Conflict(_)));
    })
    .await;
}

#[tokio::test]
async fn create_record_duplicate_domain_type_is_conflict() {
    let svc = build().await;
    as_admin(async {
        svc.create_record(create_record_req(None, "nas.lab"))
            .await
            .unwrap();
        // Same (domain, record_type) — A record for nas.lab again.
        let err = svc
            .create_record(create_record_req(None, "nas.lab"))
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::Conflict(_)));
    })
    .await;
}

#[tokio::test]
async fn create_forwarding_rule_duplicate_domain_is_conflict() {
    let svc = build().await;
    as_admin(async {
        svc.create_forwarding_rule(create_rule_req("corp.example.com"))
            .await
            .unwrap();
        let err = svc
            .create_forwarding_rule(create_rule_req("corp.example.com"))
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::Conflict(_)));
    })
    .await;
}

// ── Auth gate ──────────────────────────────────────────────────────────

#[tokio::test]
async fn methods_require_admin() {
    let svc = build().await;
    // No admin context → Forbidden, before any repo access.
    let err = svc.list_zones().await.unwrap_err();
    assert!(matches!(err, AppError::Forbidden(_)));

    let err = svc.list_forwarding_rules().await.unwrap_err();
    assert!(matches!(err, AppError::Forbidden(_)));
}
