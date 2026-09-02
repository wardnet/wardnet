use super::test_pool;
use crate::repository::SqliteDnsRepository;
use crate::repository::dns::{DnsRepository, QueryLogFilter, QueryLogRow};
use chrono::{DateTime, SubsecRound, Utc};
use wardnet_common::dns::DnsQueryResult;

fn ts_now() -> DateTime<Utc> {
    Utc::now().trunc_subsecs(0)
}

fn sample_row(client_ip: &str, domain: &str, result: &str) -> QueryLogRow {
    QueryLogRow {
        timestamp: ts_now(),
        client_ip: client_ip.to_owned(),
        domain: domain.to_owned(),
        query_type: "A".to_owned(),
        result: result.to_owned(),
        upstream: Some("8.8.8.8".to_owned()),
        latency_ms: 1.5,
        device_id: None,
        protocol: "udp".to_owned(),
    }
}

#[tokio::test]
async fn insert_and_query_log_batch() {
    let pool = test_pool().await;
    let repo = SqliteDnsRepository::new(pool);

    let entries = vec![
        sample_row("192.168.1.10", "example.com", "allowed"),
        sample_row("192.168.1.11", "test.org", "blocked"),
        sample_row("192.168.1.12", "foo.bar", "allowed"),
    ];
    repo.insert_query_log_batch(&entries).await.unwrap();

    let filter = QueryLogFilter::default();
    let rows = repo.query_log_paginated(10, 0, &filter).await.unwrap();
    assert_eq!(rows.len(), 3);

    let page2 = repo.query_log_paginated(10, 3, &filter).await.unwrap();
    assert!(page2.is_empty());

    let limited = repo.query_log_paginated(2, 0, &filter).await.unwrap();
    assert_eq!(limited.len(), 2);
}

#[tokio::test]
async fn query_log_filter_by_client_ip() {
    let pool = test_pool().await;
    let repo = SqliteDnsRepository::new(pool);

    let entries = vec![
        sample_row("192.168.1.10", "a.com", "allowed"),
        sample_row("192.168.1.20", "b.com", "allowed"),
        sample_row("192.168.1.10", "c.com", "blocked"),
    ];
    repo.insert_query_log_batch(&entries).await.unwrap();

    let filter = QueryLogFilter {
        client_ip: Some("192.168.1.10".to_owned()),
        ..Default::default()
    };
    let rows = repo.query_log_paginated(10, 0, &filter).await.unwrap();
    assert_eq!(rows.len(), 2);
    for row in &rows {
        assert_eq!(row.client_ip, "192.168.1.10");
    }

    // Substring semantics: a partial IP from the free-text filter narrows
    // to every client that contains it.
    let partial = QueryLogFilter {
        client_ip: Some("192.168.1".to_owned()),
        ..Default::default()
    };
    let rows = repo.query_log_paginated(10, 0, &partial).await.unwrap();
    assert_eq!(rows.len(), 3, "partial IP must match all 192.168.1.* rows");
}

/// The device filter matches on write-time attribution: the same device
/// stays findable after its IP changes, and an unattributed row (NULL
/// `device_id`) never matches a device filter.
#[tokio::test]
async fn query_log_filter_by_device_id() {
    let pool = test_pool().await;
    let repo = SqliteDnsRepository::new(pool);

    let device = "550e8400-e29b-41d4-a716-446655440000";
    let mut before_dhcp_churn = sample_row("192.168.1.10", "a.com", "allowed");
    before_dhcp_churn.device_id = Some(device.to_owned());
    let mut after_dhcp_churn = sample_row("192.168.1.55", "b.com", "allowed");
    after_dhcp_churn.device_id = Some(device.to_owned());
    // Different device later holding the first IP — must not match.
    let unattributed_on_same_ip = sample_row("192.168.1.10", "c.com", "blocked");

    repo.insert_query_log_batch(&[before_dhcp_churn, after_dhcp_churn, unattributed_on_same_ip])
        .await
        .unwrap();

    let filter = QueryLogFilter {
        device_id: Some(device.to_owned()),
        ..Default::default()
    };
    let rows = repo.query_log_paginated(10, 0, &filter).await.unwrap();
    assert_eq!(rows.len(), 2, "both IPs' rows attribute to the device");
    for row in &rows {
        assert_eq!(row.device_id.as_deref(), Some(device));
    }
}

#[tokio::test]
async fn query_log_filter_by_domain() {
    let pool = test_pool().await;
    let repo = SqliteDnsRepository::new(pool);

    let entries = vec![
        sample_row("10.0.0.1", "ads.tracker.com", "blocked"),
        sample_row("10.0.0.1", "example.com", "allowed"),
        sample_row("10.0.0.1", "tracker.net", "blocked"),
    ];
    repo.insert_query_log_batch(&entries).await.unwrap();

    let filter = QueryLogFilter {
        domain: Some("tracker".to_owned()),
        ..Default::default()
    };
    let rows = repo.query_log_paginated(10, 0, &filter).await.unwrap();
    assert_eq!(rows.len(), 2);
    for row in &rows {
        assert!(row.domain.contains("tracker"));
    }
}

#[tokio::test]
async fn query_log_filter_by_result() {
    let pool = test_pool().await;
    let repo = SqliteDnsRepository::new(pool);

    let entries = vec![
        sample_row("10.0.0.1", "a.com", "allowed"),
        sample_row("10.0.0.2", "b.com", "blocked"),
        sample_row("10.0.0.3", "c.com", "blocked"),
    ];
    repo.insert_query_log_batch(&entries).await.unwrap();

    let filter = QueryLogFilter {
        result: Some(DnsQueryResult::Blocked),
        ..Default::default()
    };
    let rows = repo.query_log_paginated(10, 0, &filter).await.unwrap();
    assert_eq!(rows.len(), 2);
    for row in &rows {
        assert_eq!(row.result, "blocked");
    }
}

#[tokio::test]
async fn query_log_filter_combines_conditions_with_and() {
    let pool = test_pool().await;
    let repo = SqliteDnsRepository::new(pool);

    let entries = vec![
        sample_row("10.0.0.1", "a.com", "allowed"),
        sample_row("10.0.0.2", "b.com", "blocked"),
        sample_row("10.0.0.3", "c.com", "blocked"),
        sample_row("10.0.0.4", "d.com", "allowed"),
    ];
    repo.insert_query_log_batch(&entries).await.unwrap();

    let filter = QueryLogFilter {
        result: Some(DnsQueryResult::Blocked),
        ..Default::default()
    };
    let rows = repo.query_log_paginated(10, 0, &filter).await.unwrap();
    assert_eq!(rows.len(), 2);

    let combined = QueryLogFilter {
        client_ip: Some("10.0.0.2".to_owned()),
        result: Some(DnsQueryResult::Blocked),
        ..Default::default()
    };
    let rows = repo.query_log_paginated(10, 0, &combined).await.unwrap();
    assert_eq!(rows.len(), 1, "conditions narrow rather than widen");
    assert_eq!(rows[0].client_ip, "10.0.0.2");
}

#[tokio::test]
async fn cleanup_query_log() {
    let pool = test_pool().await;
    let repo = SqliteDnsRepository::new(pool);

    let old_ts = DateTime::parse_from_rfc3339("2020-01-01T00:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    let recent_ts = ts_now();

    let entries = vec![
        QueryLogRow {
            timestamp: old_ts,
            client_ip: "10.0.0.1".to_owned(),
            domain: "old.com".to_owned(),
            query_type: "A".to_owned(),
            result: "allowed".to_owned(),
            upstream: None,
            latency_ms: 1.0,
            device_id: None,
            protocol: "udp".to_owned(),
        },
        QueryLogRow {
            timestamp: old_ts,
            client_ip: "10.0.0.2".to_owned(),
            domain: "ancient.com".to_owned(),
            query_type: "AAAA".to_owned(),
            result: "blocked".to_owned(),
            upstream: None,
            latency_ms: 2.0,
            device_id: None,
            protocol: "udp".to_owned(),
        },
        QueryLogRow {
            timestamp: recent_ts,
            client_ip: "10.0.0.3".to_owned(),
            domain: "fresh.com".to_owned(),
            query_type: "A".to_owned(),
            result: "allowed".to_owned(),
            upstream: Some("1.1.1.1".to_owned()),
            latency_ms: 0.5,
            device_id: None,
            protocol: "dot".to_owned(),
        },
    ];
    repo.insert_query_log_batch(&entries).await.unwrap();

    let deleted = repo.cleanup_query_log(30).await.unwrap();
    assert_eq!(deleted, 2);

    let rows = repo
        .query_log_paginated(10, 0, &QueryLogFilter::default())
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].domain, "fresh.com");
}

#[tokio::test]
async fn query_log_paginated_ordering() {
    let pool = test_pool().await;
    let repo = SqliteDnsRepository::new(pool);

    let entries = vec![
        sample_row("10.0.0.1", "first.com", "allowed"),
        sample_row("10.0.0.2", "second.com", "allowed"),
        sample_row("10.0.0.3", "third.com", "allowed"),
    ];
    repo.insert_query_log_batch(&entries).await.unwrap();

    let rows = repo
        .query_log_paginated(10, 0, &QueryLogFilter::default())
        .await
        .unwrap();
    assert_eq!(rows.len(), 3);

    assert_eq!(rows[0].domain, "third.com");
    assert_eq!(rows[1].domain, "second.com");
    assert_eq!(rows[2].domain, "first.com");
}

#[tokio::test]
async fn insert_empty_batch() {
    let pool = test_pool().await;
    let repo = SqliteDnsRepository::new(pool);

    repo.insert_query_log_batch(&[]).await.unwrap();

    let rows = repo
        .query_log_paginated(10, 0, &QueryLogFilter::default())
        .await
        .unwrap();
    assert!(rows.is_empty());
}

#[tokio::test]
async fn repeated_values_share_one_lookup_row() {
    let pool = test_pool().await;
    let repo = SqliteDnsRepository::new(pool.clone());

    let entries = vec![
        sample_row("192.168.1.10", "example.com", "allowed"),
        sample_row("192.168.1.10", "example.com", "allowed"),
        sample_row("192.168.1.10", "example.com", "blocked"),
    ];
    repo.insert_query_log_batch(&entries).await.unwrap();
    // A second batch re-resolves values the first batch already inserted.
    repo.insert_query_log_batch(&entries).await.unwrap();

    for (table, expected) in [
        ("lk_dns_domain", 1),
        ("lk_dns_client_ip", 1),
        ("lk_dns_result", 2),
        ("lk_dns_protocol", 1),
    ] {
        let (count,): (i64,) =
            sqlx::query_as(sqlx::AssertSqlSafe(format!("SELECT COUNT(*) FROM {table}")))
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(
            count, expected,
            "{table} should hold {expected} distinct values"
        );
    }

    let rows = repo
        .query_log_paginated(10, 0, &QueryLogFilter::default())
        .await
        .unwrap();
    assert_eq!(rows.len(), 6);
    assert!(rows.iter().all(|r| r.domain == "example.com"));
}

#[tokio::test]
async fn timestamp_survives_the_epoch_round_trip() {
    let pool = test_pool().await;
    let repo = SqliteDnsRepository::new(pool);

    let when = DateTime::parse_from_rfc3339("2026-05-05T12:34:56Z")
        .unwrap()
        .with_timezone(&Utc);
    let mut entry = sample_row("10.0.0.1", "example.com", "allowed");
    entry.timestamp = when;
    repo.insert_query_log_batch(&[entry]).await.unwrap();

    let rows = repo
        .query_log_paginated(10, 0, &QueryLogFilter::default())
        .await
        .unwrap();
    assert_eq!(rows[0].timestamp, when);
}

#[tokio::test]
async fn cleanup_prunes_orphaned_domains_and_keeps_live_ones() {
    let pool = test_pool().await;
    let repo = SqliteDnsRepository::new(pool.clone());

    let old = DateTime::parse_from_rfc3339("2020-01-01T00:00:00Z")
        .unwrap()
        .with_timezone(&Utc);

    let mut stale = sample_row("10.0.0.1", "expired.com", "allowed");
    stale.timestamp = old;
    let fresh = sample_row("10.0.0.2", "kept.com", "allowed");
    repo.insert_query_log_batch(&[stale, fresh]).await.unwrap();

    let domains: Vec<String> = sqlx::query_scalar("SELECT v FROM lk_dns_domain ORDER BY v")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(domains, vec!["expired.com", "kept.com"]);

    repo.cleanup_query_log(30).await.unwrap();

    // The retention delete orphaned `expired.com`; the prune in the same call
    // removes it. `kept.com` is still referenced and must survive.
    let domains: Vec<String> = sqlx::query_scalar("SELECT v FROM lk_dns_domain ORDER BY v")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(domains, vec!["kept.com"]);
}

/// The prune must stay a single scan of the log, in two independent respects.
///
/// The query plan covers one: a correlated `NOT EXISTS` rescans
/// `dns_query_log` per lookup row and measured 135 s against a real database.
///
/// The plan cannot cover the other. With `foreign_keys=ON` — which is how the
/// daemon runs — SQLite proves each parent DELETE safe by scanning the child
/// table, once per deleted row, unless the child key is indexed. Those scans
/// never appear in `EXPLAIN QUERY PLAN`, so the index is asserted directly:
/// measured 33.5 s without it versus 0.016 s with, on 500k rows.
#[tokio::test]
async fn prune_scans_the_log_once() {
    let pool = test_pool().await;
    let repo = SqliteDnsRepository::new(pool.clone());
    repo.insert_query_log_batch(&[sample_row("10.0.0.1", "example.com", "allowed")])
        .await
        .unwrap();

    let indexed: Vec<String> =
        sqlx::query_scalar("SELECT name FROM pragma_index_list('dns_query_log')")
            .fetch_all(&pool)
            .await
            .unwrap();
    let mut covers_domain_id = false;
    for index in indexed {
        let cols: Vec<String> = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
            "SELECT name FROM pragma_index_info('{index}')"
        )))
        .fetch_all(&pool)
        .await
        .unwrap();
        if cols.first().map(String::as_str) == Some("domain_id") {
            covers_domain_id = true;
        }
    }
    assert!(
        covers_domain_id,
        "dns_query_log.domain_id must be indexed or the FK check makes the \
         prune scan the whole log once per orphaned domain"
    );

    let rows = sqlx::query_as::<_, (i64, i64, i64, String)>(
        "EXPLAIN QUERY PLAN \
         DELETE FROM lk_dns_domain \
         WHERE id NOT IN (SELECT DISTINCT domain_id FROM dns_query_log)",
    )
    .fetch_all(&pool)
    .await
    .unwrap();

    let plan = rows
        .into_iter()
        .map(|(_, _, _, detail)| detail)
        .collect::<Vec<_>>()
        .join(" | ");
    assert!(
        plan.contains("LIST SUBQUERY"),
        "prune should resolve the log once into a list, got: {plan}"
    );
    assert!(
        !plan.contains("CORRELATED"),
        "prune must not become a correlated subquery, got: {plan}"
    );
}

/// The prune must not leave a log row pointing at a domain it deleted.
///
/// The first assertion is what stops the rest being vacuous: the prune deletes
/// only unreferenced rows by construction, so it could never orphan one on its
/// own — the check that a dangling id is *refused* is what ties this to the
/// declared constraints. It also pins the enforcement the prune's cost depends
/// on: see `prune_scans_the_log_once`.
#[tokio::test]
async fn prune_never_orphans_a_live_log_row() {
    let pool = test_pool().await;
    let repo = SqliteDnsRepository::new(pool.clone());

    let old = DateTime::parse_from_rfc3339("2020-01-01T00:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    let mut stale = sample_row("10.0.0.1", "expired.com", "allowed");
    stale.timestamp = old;
    repo.insert_query_log_batch(&[stale, sample_row("10.0.0.2", "kept.com", "allowed")])
        .await
        .unwrap();

    // A dangling id must be refused outright, not merely never produced.
    // sqlx enables `foreign_keys` by default and the daemon's pools set it
    // explicitly, so this asserts a property both rely on rather than one this
    // test arranges.
    let rejected = sqlx::query(
        "INSERT INTO dns_query_log \
         (timestamp, client_ip_id, domain_id, query_type_id, result_id, latency_ms, protocol_id) \
         VALUES (0, 1, 999999, 1, 1, 0, 1)",
    )
    .execute(&pool)
    .await;
    let error = rejected
        .expect_err("a dangling lookup id must be refused")
        .to_string();
    assert!(
        error.to_uppercase().contains("FOREIGN KEY"),
        "the insert must fail on the foreign key, not on something else — a \
         later NOT NULL column would otherwise keep this green while foreign \
         key enforcement silently regressed. Got: {error}"
    );

    repo.cleanup_query_log(30).await.unwrap();

    let (violations,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM dns_query_log q \
         LEFT JOIN lk_dns_domain d ON d.id = q.domain_id WHERE d.id IS NULL",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        violations, 0,
        "prune left a log row pointing at a deleted domain"
    );

    let rows = repo
        .query_log_paginated(10, 0, &QueryLogFilter::default())
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].domain, "kept.com");
}

/// A batch spanning several `RESOLVE_CHUNK` chunks resolves every value to its
/// own id — the failure guarded here is a later chunk's ids overwriting or
/// shadowing an earlier one's, which would file rows under the wrong domain
/// with nothing failing.
///
/// This does **not** prove the chunking is necessary: 700 binds is far under
/// SQLite's limit, so it passes with `RESOLVE_CHUNK` removed. The limit is why
/// chunking exists; this pins that chunking is *correct*. Reaching the real
/// limit needs >32,766 distinct values in one batch, which no caller produces
/// and which would trade a slow test for a case the type system already makes
/// unreachable in practice.
#[tokio::test]
async fn resolves_values_spanning_several_chunks() {
    let pool = test_pool().await;
    let repo = SqliteDnsRepository::new(pool.clone());

    let entries: Vec<QueryLogRow> = (0..700)
        .map(|i| {
            sample_row(
                &format!("10.0.{}.{}", i / 256, i % 256),
                &format!("d{i}.example"),
                "allowed",
            )
        })
        .collect();
    repo.insert_query_log_batch(&entries).await.unwrap();

    let (domains,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM lk_dns_domain")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(domains, 700);

    let rows = repo
        .query_log_paginated(1000, 0, &QueryLogFilter::default())
        .await
        .unwrap();
    assert_eq!(rows.len(), 700);
    // Every row must carry its own domain, not a neighbour's id.
    let mut seen: Vec<String> = rows.into_iter().map(|r| r.domain).collect();
    seen.sort();
    seen.dedup();
    assert_eq!(seen.len(), 700, "some rows resolved to the wrong lookup id");
}
