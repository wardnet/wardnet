//! Micro-benchmarks for the hot DNS components.
//!
//! These isolate individual pieces of the resolution pipeline so a change's
//! cost shows up as its own line in the `cubit` trend dashboard, rather than
//! being averaged into an end-to-end number. criterion writes the timings to
//! `target/criterion/**/estimates.json`, which `cubit` ingests.
//!
//! Coverage: the `DnsCache` hot path (single-threaded and under a concurrency
//! sweep), the `AuthoritativeView` lookups, the DNS filter (rule parsing +
//! matching), and hickory `Message` parse/encode.
//!
//! Run: `cargo bench -p wardnetd-services --bench dns_components`

use std::hint::black_box;
use std::net::{IpAddr, Ipv4Addr};
use std::sync::{Arc, Barrier, RwLock};
use std::thread;
use std::time::Instant;

use arc_swap::ArcSwap;
use chrono::Utc;
use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use hickory_proto::op::{Message, OpCode, Query};
use hickory_proto::rr::rdata::A;
use hickory_proto::rr::{Name, RData, Record, RecordType};
use hickory_proto::serialize::binary::{BinDecodable, BinEncodable};
use uuid::Uuid;
use wardnet_common::dns::{
    ConditionalForwardingRule, CustomDnsRecord, DnsRecordSource, DnsRecordType, UpstreamId,
};
use wardnetd_data::repository::QueryLogRow;
use wardnetd_services::dns::authoritative::AuthoritativeView;
use wardnetd_services::dns::filter_parser::parse_line;
use wardnetd_services::dns::{DnsCache, row_to_event};
use wardnetd_services::dns_filter::{DnsFilter, DnsFilterInputs, RuntimeDnsFilterProfile};

fn response() -> Message {
    Message::response(0, OpCode::Query)
}

/// `DnsCache` lookup + insert — the per-query cache path. Hit and miss are
/// benched separately because they exercise different branches (the miss path
/// also evicts the absent-and-expired key).
fn bench_cache(c: &mut Criterion) {
    let mut group = c.benchmark_group("dns_cache");

    group.bench_function("get_hit", |b| {
        let mut cache = DnsCache::new(1024);
        cache.insert(
            UpstreamId::Default,
            "example.com",
            RecordType::A,
            response(),
            300,
            0,
            86400,
        );
        b.iter(|| {
            black_box(cache.get(UpstreamId::Default, black_box("example.com"), RecordType::A));
        });
    });

    group.bench_function("get_miss", |b| {
        // `get` takes `&self` (hit/miss counters + TTL aging use interior
        // mutability), so this cache needs no `mut` — only `insert` does.
        let cache = DnsCache::new(1024);
        b.iter(|| {
            black_box(cache.get(
                UpstreamId::Default,
                black_box("absent.example.com"),
                RecordType::A,
            ));
        });
    });

    group.bench_function("insert", |b| {
        b.iter_batched(
            || DnsCache::new(1024),
            |mut cache| {
                cache.insert(
                    UpstreamId::Default,
                    black_box("example.com"),
                    RecordType::A,
                    response(),
                    300,
                    0,
                    86400,
                );
                cache
            },
            criterion::BatchSize::SmallInput,
        );
    });

    group.finish();
}

/// Concurrency sweep on `DnsCache::get` — the daemon shares one cache behind a
/// read lock across every per-query task, so a change that makes the hit path
/// contend (e.g. a lock added around the atomic counters) shows up here.
///
/// `get` takes `&self`, so a `std::sync::RwLock` read guard models the shared
/// hot path faithfully. Each of the N threads runs `iters` lookups; a
/// [`Barrier`] releases them together and only the parallel region is timed.
/// The per-measurement thread spawn/join is amortized across `iters`.
fn bench_cache_concurrent(c: &mut Criterion) {
    let mut group = c.benchmark_group("dns_cache");

    for threads in [1usize, 4, 8, 16] {
        group.bench_function(
            BenchmarkId::new("get_hit", format!("threads={threads}")),
            |b| {
                let mut cache = DnsCache::new(1024);
                cache.insert(
                    UpstreamId::Default,
                    "example.com",
                    RecordType::A,
                    response(),
                    300,
                    0,
                    86400,
                );
                let cache = Arc::new(RwLock::new(cache));

                b.iter_custom(|iters| {
                    let barrier = Arc::new(Barrier::new(threads + 1));
                    let handles: Vec<_> = (0..threads)
                        .map(|_| {
                            let cache = Arc::clone(&cache);
                            let barrier = Arc::clone(&barrier);
                            thread::spawn(move || {
                                barrier.wait();
                                for _ in 0..iters {
                                    let guard = cache.read().expect("cache read lock");
                                    black_box(guard.get(
                                        UpstreamId::Default,
                                        black_box("example.com"),
                                        RecordType::A,
                                    ));
                                }
                            })
                        })
                        .collect();

                    barrier.wait();
                    let start = Instant::now();
                    for h in handles {
                        h.join().expect("worker thread");
                    }
                    start.elapsed()
                });
            },
        );
    }

    group.finish();
}

/// Build `n` distinct enabled A records under `zone`, named `host{i}.{zone}`.
fn a_records(zone: &str, n: usize) -> Vec<CustomDnsRecord> {
    (0..n)
        .map(|i| CustomDnsRecord {
            id: Uuid::new_v4(),
            zone_id: None,
            domain: format!("host{i}.{zone}"),
            record_type: DnsRecordType::A,
            value: format!("10.0.{}.{}", i / 256, i % 256),
            ttl: 120,
            enabled: true,
            source: DnsRecordSource::Manual,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        })
        .collect()
}

/// `AuthoritativeView` lookups — the pre-cache local-record path taken on every
/// query. Swept over view sizes so a change from O(1) hashing to anything
/// worse is visible as the size grows.
fn bench_authoritative(c: &mut Criterion) {
    let mut group = c.benchmark_group("authoritative");

    for size in [10usize, 100, 1000] {
        let mut records = a_records("zone.lan", size);
        // One CNAME so `lookup_cname` has something to find.
        records.push(CustomDnsRecord {
            id: Uuid::new_v4(),
            zone_id: None,
            domain: "alias.zone.lan".into(),
            record_type: DnsRecordType::Cname,
            value: "host0.zone.lan".into(),
            ttl: 120,
            enabled: true,
            source: DnsRecordSource::Manual,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        });
        let rules: Vec<ConditionalForwardingRule> = (0..size)
            .map(|i| ConditionalForwardingRule {
                id: Uuid::new_v4(),
                domain: format!("corp{i}.example"),
                upstream: "10.1.2.3:53".into(),
                enabled: true,
                created_at: Utc::now(),
            })
            .collect();
        let view = AuthoritativeView::build(&[], records, rules);

        group.bench_with_input(BenchmarkId::new("lookup_hit", size), &size, |b, _| {
            b.iter(|| {
                black_box(view.lookup(black_box("host0.zone.lan"), DnsRecordType::A));
            });
        });
        group.bench_with_input(BenchmarkId::new("lookup_miss", size), &size, |b, _| {
            b.iter(|| {
                black_box(view.lookup(black_box("absent.zone.lan"), DnsRecordType::A));
            });
        });
        group.bench_with_input(BenchmarkId::new("lookup_cname", size), &size, |b, _| {
            b.iter(|| {
                black_box(view.lookup_cname(black_box("alias.zone.lan")));
            });
        });
        group.bench_with_input(
            BenchmarkId::new("match_forwarding_rule", size),
            &size,
            |b, _| {
                // A subdomain of the last (longest to reach) rule, so the
                // suffix scan does real work.
                let needle = format!("svc.corp{}.example", size.saturating_sub(1));
                b.iter(|| {
                    black_box(view.match_forwarding_rule(black_box(&needle)));
                });
            },
        );
    }

    group.finish();
}

/// DNS filter: rule-text parsing throughput and query matching against an
/// N-rule set. Both are on the blocklist-compile / hot paths.
fn bench_filter(c: &mut Criterion) {
    let mut group = c.benchmark_group("dns_filter");

    // `parse_line` — called once per rule when a blocklist/custom-rules source
    // is compiled. Benched on a representative adblock-syntax domain rule.
    group.bench_function("parse_line", |b| {
        b.iter(|| {
            black_box(parse_line(black_box("||ads.example.com^")).ok());
        });
    });

    let client = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 50));
    for size in [100usize, 1000, 10_000] {
        let blocked: Vec<String> = (0..size)
            .map(|i| format!("blocked{i}.example.com"))
            .collect();
        let filter = DnsFilter::build(DnsFilterInputs {
            blocked_domains: blocked,
            allowlist: vec![],
            custom_rules: vec![],
        });

        // Hit: a name in the block set (worst case — the full match runs).
        group.bench_with_input(BenchmarkId::new("check_block", size), &size, |b, _| {
            let needle = format!("blocked{}.example.com", size / 2);
            b.iter(|| {
                black_box(filter.check(black_box(&needle), RecordType::A, client));
            });
        });
        // Miss: a name absent from every set (the common pass-through case).
        group.bench_with_input(BenchmarkId::new("check_pass", size), &size, |b, _| {
            b.iter(|| {
                black_box(filter.check(black_box("allowed.example.org"), RecordType::A, client));
            });
        });
    }

    let build_10k = || {
        DnsFilter::build(DnsFilterInputs {
            blocked_domains: (0..10_000).map(|i| format!("blocked{i}.example.com")).collect(),
            allowlist: vec![],
            custom_rules: vec![],
        })
    };

    // Owned normalize path: a mixed-case, trailing-dot name (the one shape
    // `check` still has to allocate and lowercase, vs the borrow it takes for
    // the canonical names the pipeline normally hands down).
    group.bench_function("check_uncanonical", |b| {
        let filter = build_10k();
        b.iter(|| {
            black_box(filter.check(
                black_box("Allowed.Example.Org."),
                RecordType::A,
                client,
            ));
        });
    });

    // The real profile path: `RuntimeDnsFilterProfile::check` fans one query
    // across each source, loading every one through its `ArcSwap` and
    // aggregating outcomes — the cost a query actually pays when a profile
    // stacks several blocklists, not `DnsFilter::check` in isolation.
    group.bench_function("profile_check_pass", |b| {
        let filters: Vec<Arc<ArcSwap<DnsFilter>>> = (0..4)
            .map(|_| Arc::new(ArcSwap::from_pointee(build_10k())))
            .collect();
        let profile = RuntimeDnsFilterProfile::new(Uuid::nil(), "bench".to_string(), filters);
        b.iter(|| {
            black_box(profile.check(black_box("allowed.example.org"), RecordType::A, client));
        });
    });

    group.finish();
}

/// hickory `Message` decode/encode — the wire (de)serialization every query and
/// response pays once. `from_bytes` is the first thing `handle` does; `to_vec`
/// is the last thing every response path does.
fn bench_message(c: &mut Criterion) {
    let mut group = c.benchmark_group("dns_message");

    let mut query = Message::query();
    query.metadata.id = 0x1234;
    query.metadata.recursion_desired = true;
    query.add_queries(vec![Query::query(
        Name::from_str_relaxed("www.example.com.").expect("name"),
        RecordType::A,
    )]);
    let query_bytes = query.to_bytes().expect("encode query");

    let mut resp = Message::response(0x1234, OpCode::Query);
    resp.metadata.recursion_desired = true;
    resp.metadata.recursion_available = true;
    resp.add_queries(query.queries.clone());
    resp.add_answer(Record::from_rdata(
        Name::from_str_relaxed("www.example.com.").expect("name"),
        60,
        RData::A(A(Ipv4Addr::new(93, 184, 216, 34))),
    ));

    group.bench_function("parse_query", |b| {
        b.iter(|| {
            black_box(Message::from_bytes(black_box(&query_bytes)).expect("parse"));
        });
    });
    group.bench_function("encode_response", |b| {
        b.iter(|| {
            black_box(resp.to_bytes().expect("encode"));
        });
    });

    group.finish();
}

/// `row_to_event` — the WS-event projection `DnsLogSink::record` builds per
/// query (~6 string clones). It's pure waste when no live-log viewer is
/// connected (the normal state); `record` now skips it when there are no
/// subscribers, so this tracks the cost that gating elides.
fn bench_log_sink(c: &mut Criterion) {
    let mut group = c.benchmark_group("dns_log_sink");

    let row = QueryLogRow {
        timestamp: "2026-05-05T00:00:00Z".to_owned(),
        client_ip: "10.0.0.1".to_owned(),
        domain: "metrics.analytics-node.example".to_owned(),
        query_type: "A".to_owned(),
        result: "forwarded".to_owned(),
        upstream: Some("1.1.1.1:53".to_owned()),
        latency_ms: 1.0,
        device_id: None,
        protocol: "udp".to_owned(),
    };

    group.bench_function("row_to_event", |b| {
        b.iter(|| {
            black_box(row_to_event(black_box(&row)));
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_cache,
    bench_cache_concurrent,
    bench_authoritative,
    bench_filter,
    bench_message,
    bench_log_sink,
);
criterion_main!(benches);
