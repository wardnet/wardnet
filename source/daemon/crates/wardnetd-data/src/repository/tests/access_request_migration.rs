//! Tests for the rule-requests → access-requests rebuild in
//! `20260814000000_device_access_requests.sql` (issue #919).
//!
//! `SQLite` cannot relax a `NOT NULL` or add a `CHECK` in place, so the table is
//! rebuilt and the rows copied. That copy runs once per box on real data —
//! every request a household has ever filed, and every decision an admin has
//! ever made — so losing or mangling it is unrecoverable.
//!
//! Like `managed_backfill`, these execute the **shipped statement** read out of
//! the migration file rather than a copy pasted here, so the test stops
//! matching loudly if the real one drifts.

use std::sync::LazyLock;

use sqlx::SqlitePool;

use super::test_pool;

const MIGRATION: &str =
    include_str!("../../../migrations/20260814000000_device_access_requests.sql");

const DEV: &str = "00000000-0000-0000-0000-000000000001";

/// The subset of a copied row these tests assert on.
#[derive(sqlx::FromRow)]
struct CopiedRow {
    #[expect(dead_code, reason = "selected so the ORDER BY column is explicit")]
    id: String,
    kind: String,
    domain: Option<String>,
    status: String,
    decided_by: Option<String>,
}

/// Extract the row-copy `INSERT ... SELECT` from the migration file.
static COPY: LazyLock<String> = LazyLock::new(|| {
    let stmt = MIGRATION
        .split(';')
        .map(|s| {
            s.lines()
                .filter(|l| !l.trim_start().starts_with("--"))
                .collect::<Vec<_>>()
                .join("\n")
        })
        .find(|s| {
            s.trim_start()
                .starts_with("INSERT INTO device_access_requests")
        })
        .expect("migration must contain the row copy");
    assert!(
        stmt.contains("FROM device_rule_requests"),
        "extracted statement looks truncated: {stmt}"
    );
    stmt
});

/// Recreate the pre-migration table so the shipped copy has a source. The
/// migration already dropped it, and `test_pool` has run every migration.
async fn recreate_legacy_table(pool: &SqlitePool) {
    sqlx::query(
        "CREATE TABLE device_rule_requests (
            id          TEXT PRIMARY KEY,
            device_id   TEXT NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
            kind        TEXT NOT NULL,
            domain      TEXT NOT NULL,
            reason      TEXT,
            status      TEXT NOT NULL DEFAULT 'pending',
            created_at  TEXT NOT NULL,
            decided_at  TEXT,
            decided_by  TEXT
        )",
    )
    .execute(pool)
    .await
    .unwrap();
}

async fn seed_device(pool: &SqlitePool) {
    sqlx::query(
        "INSERT INTO devices (id, mac, last_ip, device_type, first_seen, last_seen, zone_id) \
         VALUES (?, 'aa:bb:cc:dd:ee:01', '192.168.1.10', 'unknown', \
                 '2026-03-07T00:00:00Z', '2026-03-07T00:00:00Z', \
                 '00000000-0000-0000-0000-000000000201')",
    )
    .bind(DEV)
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn the_rebuild_preserves_rows_and_decisions() {
    let pool = test_pool().await;
    seed_device(&pool).await;
    recreate_legacy_table(&pool).await;

    sqlx::query(
        "INSERT INTO device_rule_requests \
         (id, device_id, kind, domain, reason, status, created_at, decided_at, decided_by) \
         VALUES ('r1', ?, 'block', 'ads.example.com', 'annoying', 'pending', \
                 '2026-06-18T00:00:00Z', NULL, NULL), \
                ('r2', ?, 'allow', 'good.example.com', NULL, 'approved', \
                 '2026-06-18T00:01:00Z', '2026-06-18T01:00:00Z', 'admin-1')",
    )
    .bind(DEV)
    .bind(DEV)
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(sqlx::AssertSqlSafe(COPY.clone()))
        .execute(&pool)
        .await
        .expect("the shipped copy must succeed");

    let rows: Vec<CopiedRow> = sqlx::query_as(
        "SELECT id, kind, domain, status, decided_by \
         FROM device_access_requests ORDER BY id",
    )
    .fetch_all(&pool)
    .await
    .unwrap();

    assert_eq!(rows.len(), 2, "both legacy rows must survive");

    // `kind` values are already correct under the new vocabulary — the copy
    // deliberately rewrites nothing.
    assert_eq!(rows[0].kind, "block");
    assert_eq!(rows[0].domain.as_deref(), Some("ads.example.com"));
    assert_eq!(rows[0].status, "pending");

    // A past decision, including who made it, survives the rebuild.
    assert_eq!(rows[1].kind, "allow");
    assert_eq!(rows[1].status, "approved");
    assert_eq!(rows[1].decided_by.as_deref(), Some("admin-1"));
}

/// The copied rows must satisfy the new `CHECK` — if they did not, the
/// migration would fail on a real box mid-upgrade and leave an unbootable
/// daemon.
#[tokio::test]
async fn the_check_constraint_accepts_every_legacy_kind() {
    let pool = test_pool().await;
    seed_device(&pool).await;
    recreate_legacy_table(&pool).await;

    sqlx::query(
        "INSERT INTO device_rule_requests (id, device_id, kind, domain, status, created_at) \
         VALUES ('r1', ?, 'block', 'a.com', 'pending', '2026-06-18T00:00:00Z'), \
                ('r2', ?, 'allow', 'b.com', 'rejected', '2026-06-18T00:01:00Z')",
    )
    .bind(DEV)
    .bind(DEV)
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(sqlx::AssertSqlSafe(COPY.clone()))
        .execute(&pool)
        .await
        .expect("legacy rows must satisfy the new CHECK");
}

/// The `CHECK` is the database half of `AccessRequestKind::requires_domain` —
/// assert it actually bites, in both directions.
#[tokio::test]
async fn the_check_constraint_enforces_the_payload_pairing() {
    let pool = test_pool().await;
    seed_device(&pool).await;

    let domainless_rule = sqlx::query(
        "INSERT INTO device_access_requests (id, device_id, kind, created_at) \
         VALUES ('bad1', ?, 'block', '2026-08-14T00:00:00Z')",
    )
    .bind(DEV)
    .execute(&pool)
    .await;
    assert!(
        domainless_rule.is_err(),
        "a block request without a domain must be refused"
    );

    let private_dns_with_domain = sqlx::query(
        "INSERT INTO device_access_requests (id, device_id, kind, domain, created_at) \
         VALUES ('bad2', ?, 'private_dns', 'a.com', '2026-08-14T00:00:00Z')",
    )
    .bind(DEV)
    .execute(&pool)
    .await;
    assert!(
        private_dns_with_domain.is_err(),
        "a private_dns request naming a domain must be refused"
    );
}
