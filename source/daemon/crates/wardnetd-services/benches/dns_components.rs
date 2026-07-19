//! Micro-benchmarks for the hot DNS components.
//!
//! These isolate individual pieces of the resolution pipeline so a change's
//! cost shows up as its own line in the `cubit` trend dashboard, rather than
//! being averaged into an end-to-end number. criterion writes the timings to
//! `target/criterion/**/estimates.json`, which `cubit` ingests.
//!
//! Run: `cargo bench -p wardnetd-services --bench dns_components`

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use hickory_proto::op::{Message, OpCode};
use hickory_proto::rr::RecordType;
use wardnet_common::dns::UpstreamId;
use wardnetd_services::dns::DnsCache;

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

criterion_group!(benches, bench_cache);
criterion_main!(benches);
