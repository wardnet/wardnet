use super::test_pool;
use crate::repository::SqliteDnsRepository;
use crate::repository::dns::{DnsRepository, QueryLogFilter, QueryLogRow};
use crate::repository::sqlite::dns::{ClientIp, build_where, page_sql};
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
    let rows = repo.query_log_paginated(10, None, &filter).await.unwrap();
    assert_eq!(rows.len(), 3);

    let limited = repo.query_log_paginated(2, None, &filter).await.unwrap();
    assert_eq!(limited.len(), 2);

    // The cursor is the oldest id on the page just read, so the next page
    // starts strictly below it and the two never overlap.
    let page2 = repo
        .query_log_paginated(10, Some(limited[1].id), &filter)
        .await
        .unwrap();
    assert_eq!(page2.len(), 1);
    assert!(page2[0].id < limited[1].id);

    let past_the_end = repo
        .query_log_paginated(10, Some(page2[0].id), &filter)
        .await
        .unwrap();
    assert!(past_the_end.is_empty());
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
    let rows = repo.query_log_paginated(10, None, &filter).await.unwrap();
    assert_eq!(rows.len(), 2);
    for row in &rows {
        assert_eq!(row.entry.client_ip, "192.168.1.10");
    }

    // Substring semantics: a partial IP from the free-text filter narrows
    // to every client that contains it.
    let partial = QueryLogFilter {
        client_ip: Some("192.168.1".to_owned()),
        ..Default::default()
    };
    let rows = repo.query_log_paginated(10, None, &partial).await.unwrap();
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
    let rows = repo.query_log_paginated(10, None, &filter).await.unwrap();
    assert_eq!(rows.len(), 2, "both IPs' rows attribute to the device");
    for row in &rows {
        assert_eq!(row.entry.device_id.as_deref(), Some(device));
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
    let rows = repo.query_log_paginated(10, None, &filter).await.unwrap();
    assert_eq!(rows.len(), 2);
    for row in &rows {
        assert!(row.entry.domain.contains("tracker"));
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
    let rows = repo.query_log_paginated(10, None, &filter).await.unwrap();
    assert_eq!(rows.len(), 2);
    for row in &rows {
        assert_eq!(row.entry.result, "blocked");
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
    let rows = repo.query_log_paginated(10, None, &filter).await.unwrap();
    assert_eq!(rows.len(), 2);

    let combined = QueryLogFilter {
        client_ip: Some("10.0.0.2".to_owned()),
        result: Some(DnsQueryResult::Blocked),
        ..Default::default()
    };
    let rows = repo.query_log_paginated(10, None, &combined).await.unwrap();
    assert_eq!(rows.len(), 1, "conditions narrow rather than widen");
    assert_eq!(rows[0].entry.client_ip, "10.0.0.2");
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
        .query_log_paginated(10, None, &QueryLogFilter::default())
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].entry.domain, "fresh.com");
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
        .query_log_paginated(10, None, &QueryLogFilter::default())
        .await
        .unwrap();
    assert_eq!(rows.len(), 3);

    assert_eq!(rows[0].entry.domain, "third.com");
    assert_eq!(rows[1].entry.domain, "second.com");
    assert_eq!(rows[2].entry.domain, "first.com");
}

#[tokio::test]
async fn insert_empty_batch() {
    let pool = test_pool().await;
    let repo = SqliteDnsRepository::new(pool);

    repo.insert_query_log_batch(&[]).await.unwrap();

    let rows = repo
        .query_log_paginated(10, None, &QueryLogFilter::default())
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
        .query_log_paginated(10, None, &QueryLogFilter::default())
        .await
        .unwrap();
    assert_eq!(rows.len(), 6);
    assert!(rows.iter().all(|r| r.entry.domain == "example.com"));
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
        .query_log_paginated(10, None, &QueryLogFilter::default())
        .await
        .unwrap();
    assert_eq!(rows[0].entry.timestamp, when);
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
        .query_log_paginated(10, None, &QueryLogFilter::default())
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].entry.domain, "kept.com");
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
        .query_log_paginated(1000, None, &QueryLogFilter::default())
        .await
        .unwrap();
    assert_eq!(rows.len(), 700);
    // Every row must carry its own domain, not a neighbour's id.
    let mut seen: Vec<String> = rows.into_iter().map(|r| r.entry.domain).collect();
    seen.sort();
    seen.dedup();
    assert_eq!(seen.len(), 700, "some rows resolved to the wrong lookup id");
}

/// A tick that deletes nothing skips the prune scan entirely. Only the
/// retention delete can orphan a domain, so there is nothing to collect — and
/// on a box younger than its retention window that is every tick.
#[tokio::test]
async fn cleanup_skips_the_prune_when_nothing_expired() {
    let pool = test_pool().await;
    let repo = SqliteDnsRepository::new(pool.clone());
    repo.insert_query_log_batch(&[sample_row("10.0.0.1", "kept.com", "allowed")])
        .await
        .unwrap();

    // An orphan that no retention delete produced. The prune would collect it;
    // the guard means this tick does not look.
    sqlx::query("INSERT INTO lk_dns_domain (v) VALUES ('never-referenced.example')")
        .execute(&pool)
        .await
        .unwrap();

    let deleted = repo.cleanup_query_log(30).await.unwrap();
    assert_eq!(deleted, 0);

    let domains: Vec<String> = sqlx::query_scalar("SELECT v FROM lk_dns_domain ORDER BY v")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(domains, vec!["kept.com", "never-referenced.example"]);
}

/// `client_ip_id` is indexed, and the single-client filter is the shape that
/// index can seek — asserted against the statement the repository builds.
///
/// The pair matters more than either half. The index is inert if the filter
/// reaches SQLite as something it cannot constrain on, and the filter shape
/// buys nothing without the index; either way the page becomes a full pass over
/// `dns_query_log`. Specifically, an `IN (SELECT …)` predicate over the same
/// indexed column sorts instead of seeking, because `ORDER BY q.id DESC` gives
/// the planner a backwards primary-key walk to prefer.
///
/// The plan is taken from `page_sql` rather than a copy of it, because the joins
/// and the ordering are what decide whether the index is used at all — a
/// hand-written approximation stays green while the real statement stops
/// seeking.
#[tokio::test]
async fn client_ip_filter_seeks_its_index() {
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
    let mut covers_client_ip_id = false;
    for index in indexed {
        let cols: Vec<String> = sqlx::query_scalar("SELECT name FROM pragma_index_info(?)")
            .bind(&index)
            .fetch_all(&pool)
            .await
            .unwrap();
        if cols.first().map(String::as_str) == Some("client_ip_id") {
            covers_client_ip_id = true;
        }
    }
    assert!(
        covers_client_ip_id,
        "dns_query_log.client_ip_id must be indexed or the client filter scans the log"
    );

    let plan = page_plan(&pool, Some(&ClientIp::One(1)), None).await;
    assert!(
        plan.contains("SEARCH q") && plan.contains("idx_dns_query_log_client_ip_id"),
        "the client filter should seek its index, got: {plan}"
    );
    // Under an equality constraint the index's entries are already in rowid
    // order, so the ordering costs nothing. A sort here would mean every row
    // for that client is visited before `LIMIT` applies.
    assert!(
        !plan.contains("TEMP B-TREE"),
        "the index must serve the ordering too, got: {plan}"
    );
}

/// The same seek survives a cursor: paging deeper must not cost the index.
///
/// `q.id < ?` and `q.client_ip_id = ?` compete for the same rowid ordering, so
/// a planner that took the cursor as its driving constraint would go back to
/// testing every row for the client filter.
#[tokio::test]
async fn a_cursor_does_not_cost_the_client_index() {
    let pool = test_pool().await;
    let repo = SqliteDnsRepository::new(pool.clone());
    repo.insert_query_log_batch(&[sample_row("10.0.0.1", "example.com", "allowed")])
        .await
        .unwrap();

    let plan = page_plan(&pool, Some(&ClientIp::One(1)), Some(42)).await;
    assert!(
        plan.contains("SEARCH q") && plan.contains("idx_dns_query_log_client_ip_id"),
        "a cursor must not displace the client index, got: {plan}"
    );
    assert!(
        !plan.contains("TEMP B-TREE"),
        "the index must still serve the ordering, got: {plan}"
    );
}

/// `EXPLAIN QUERY PLAN` over the statement `query_log_paginated` builds, for a
/// filter already resolved to `client_ip`.
async fn page_plan(
    pool: &sqlx::SqlitePool,
    client_ip: Option<&ClientIp>,
    before: Option<i64>,
) -> String {
    let (where_clause, _) = build_where(&QueryLogFilter::default(), client_ip, before);
    let rows = sqlx::query_as::<_, (i64, i64, i64, String)>(sqlx::AssertSqlSafe(format!(
        "EXPLAIN QUERY PLAN {}",
        page_sql(&where_clause)
    )))
    .fetch_all(pool)
    .await
    .unwrap();
    rows.into_iter()
        .map(|(_, _, _, detail)| detail)
        .collect::<Vec<_>>()
        .join(" | ")
}

/// A client the log has never recorded returns an empty page.
///
/// The substring is resolved against `lk_dns_client_ip` before the log is
/// queried, so "no such client" is answered from a few thousand lookup rows.
/// Left to SQLite the same verdict costs a pass over everything the other
/// filters admit, because there is no matching row for `LIMIT` to stop at.
#[tokio::test]
async fn an_unknown_client_yields_an_empty_page() {
    let pool = test_pool().await;
    let repo = SqliteDnsRepository::new(pool);
    repo.insert_query_log_batch(&[
        sample_row("192.168.1.10", "a.com", "allowed"),
        sample_row("192.168.1.20", "b.com", "allowed"),
    ])
    .await
    .unwrap();

    let filter = QueryLogFilter {
        client_ip: Some("10.9.9.9".to_owned()),
        ..Default::default()
    };
    let rows = repo.query_log_paginated(10, None, &filter).await.unwrap();
    assert!(rows.is_empty());
}

/// A substring broad enough to match more clients than the repository will
/// name individually still returns their rows.
///
/// Past that threshold the filter reverts to handing SQLite the pattern, so
/// this pins the fallback rather than the seek — the two paths must agree on
/// what the filter means.
#[tokio::test]
async fn a_broad_client_substring_matches_every_client_it_names() {
    let pool = test_pool().await;
    let repo = SqliteDnsRepository::new(pool);

    // 100 distinct clients, all sharing the "10.0." prefix, which is more than
    // the repository binds individually.
    let entries: Vec<QueryLogRow> = (0..100)
        .map(|i| sample_row(&format!("10.0.{i}.1"), "example.com", "allowed"))
        .collect();
    repo.insert_query_log_batch(&entries).await.unwrap();
    repo.insert_query_log_batch(&[sample_row("172.16.0.1", "other.com", "allowed")])
        .await
        .unwrap();

    let filter = QueryLogFilter {
        client_ip: Some("10.0.".to_owned()),
        ..Default::default()
    };
    let rows = repo.query_log_paginated(200, None, &filter).await.unwrap();
    assert_eq!(rows.len(), 100);
    assert!(rows.iter().all(|r| r.entry.client_ip.starts_with("10.0.")));
}

/// Paging with a cursor visits every row exactly once, in descending id order,
/// and the last page reports itself by coming back short.
#[tokio::test]
async fn cursor_paging_walks_the_log_without_gaps_or_repeats() {
    let pool = test_pool().await;
    let repo = SqliteDnsRepository::new(pool);

    let entries: Vec<QueryLogRow> = (0..25)
        .map(|i| sample_row("10.0.0.1", &format!("d{i}.com"), "allowed"))
        .collect();
    repo.insert_query_log_batch(&entries).await.unwrap();

    let mut seen: Vec<i64> = Vec::new();
    let mut cursor: Option<i64> = None;
    loop {
        let page = repo
            .query_log_paginated(10, cursor, &QueryLogFilter::default())
            .await
            .unwrap();
        if page.is_empty() {
            break;
        }
        seen.extend(page.iter().map(|r| r.id));
        cursor = page.last().map(|r| r.id);
    }

    assert_eq!(seen.len(), 25, "every row is visited exactly once");
    let mut sorted = seen.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), 25, "no row is served on two pages");
    assert!(
        seen.windows(2).all(|w| w[0] > w[1]),
        "the walk stays in descending id order across page boundaries"
    );
}

/// A cursor and a filter compose: the page is the filtered rows below the
/// cursor, not the cursor applied to an unfiltered page.
#[tokio::test]
async fn a_cursor_narrows_within_a_filter() {
    let pool = test_pool().await;
    let repo = SqliteDnsRepository::new(pool);

    repo.insert_query_log_batch(&[
        sample_row("10.0.0.1", "a.com", "allowed"),
        sample_row("10.0.0.2", "b.com", "allowed"),
        sample_row("10.0.0.1", "c.com", "allowed"),
        sample_row("10.0.0.2", "d.com", "allowed"),
    ])
    .await
    .unwrap();

    let filter = QueryLogFilter {
        client_ip: Some("10.0.0.1".to_owned()),
        ..Default::default()
    };
    let first = repo.query_log_paginated(1, None, &filter).await.unwrap();
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].entry.domain, "c.com");

    let second = repo
        .query_log_paginated(10, Some(first[0].id), &filter)
        .await
        .unwrap();
    assert_eq!(second.len(), 1, "only the other 10.0.0.1 row is below it");
    assert_eq!(second[0].entry.domain, "a.com");
}
