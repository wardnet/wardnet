//! End-to-end macro-benchmarks for the DNS resolution pipeline (#911's
//! `QueryPipeline`).
//!
//! Each benchmark drives `QueryPipeline::handle` down exactly one resolution
//! branch, so a regression shows up as its own line in the `cubit` trend
//! dashboard rather than being averaged into a single end-to-end number:
//!
//! * `cache_hit`     — answered from the response cache; no upstream touched.
//! * `cache_miss`    — a fresh domain forwarded to an in-process stub upstream.
//! * `blocked`       — filter verdict `Block` → NXDOMAIN, before cache/upstream.
//! * `rewrite`       — filter verdict `Rewrite` → synthetic A, before upstream.
//! * `authoritative` — answered from the local authoritative view.
//!
//! The seam lives in `wardnetd::dns::bench_support`, compiled only under the
//! `bench-internals` feature (hence the `required-features` on the `[[bench]]`).
//!
//! Run: `cargo bench -p wardnetd --bench dns_resolution --features bench-internals`

use std::hint::black_box;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use criterion::{Criterion, criterion_group, criterion_main};
use hickory_proto::rr::RecordType;
use tokio::runtime::Runtime;
use tokio::sync::mpsc::Receiver;
use wardnet_common::dns::FilterAction;
use wardnetd::dns::bench_support::{
    BenchPipeline, a_record, build_bench_pipeline, query_bytes, spawn_stub_upstream,
};
use wardnetd::dns::pipeline::{ClientIdentity, TransportProtocol};
use wardnetd::dns::reply_capture::ReplyCapture;
use wardnetd_services::dns::server::DnsSocket;

/// Transport-level peer address every bench query arrives from.
const SRC: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 5353);

fn client() -> ClientIdentity {
    ClientIdentity::Ip(SRC.ip())
}

/// A multi-thread tokio runtime — the daemon resolves on a multi-thread
/// runtime, and the forwarding branch's loopback round-trip needs the reactor
/// to make progress on the stub-upstream task concurrently with `handle`.
fn runtime() -> Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("build multi-thread runtime")
}

/// Stand up a bench pipeline whose upstream is a fresh in-process stub.
fn setup(rt: &Runtime) -> BenchPipeline {
    rt.block_on(async {
        let upstream = spawn_stub_upstream().await;
        build_bench_pipeline(upstream)
    })
}

/// One reusable capture socket + its drain. The pipeline sends at most one
/// frame per query into a capacity-1 channel, so draining after every `handle`
/// keeps the single slot free for the next iteration (a fresh channel per
/// iteration would only add allocation noise).
fn reply_pair() -> (Arc<dyn DnsSocket>, Receiver<Vec<u8>>) {
    let (capture, rx) = ReplyCapture::channel();
    (Arc::new(capture), rx)
}

/// Resolve one query and drain its captured reply — the timed unit of work.
fn drive(
    rt: &Runtime,
    bench: &BenchPipeline,
    reply: &Arc<dyn DnsSocket>,
    rx: &mut Receiver<Vec<u8>>,
    packet: &[u8],
) {
    rt.block_on(async {
        bench
            .pipeline()
            .handle(packet, SRC, reply, client(), TransportProtocol::Udp)
            .await
            .expect("pipeline handle");
    });
    black_box(rx.try_recv().ok());
}

fn bench_resolution(c: &mut Criterion) {
    let rt = runtime();
    let mut group = c.benchmark_group("dns_resolution");

    // cache_hit: warm the cache once, then every iteration is served from it —
    // the stub upstream exists but is never touched again.
    group.bench_function("cache_hit", |b| {
        let bench = setup(&rt);
        let (reply, mut rx) = reply_pair();
        let packet = query_bytes(0x0001, "cache.bench.example.", RecordType::A);
        drive(&rt, &bench, &reply, &mut rx, &packet); // warm
        b.iter(|| drive(&rt, &bench, &reply, &mut rx, &packet));
    });

    // cache_miss: a fresh domain every iteration forces a miss → forward to the
    // stub upstream, whose TTL-60 answer is written back to the cache. The
    // unique-domain scheme (a monotonic counter) keeps it a miss without a
    // cache flush; the per-iteration `format!` + encode is dwarfed by the
    // loopback round-trip, and the bounded cache (10k default) evicts FIFO.
    group.bench_function("cache_miss", |b| {
        let bench = setup(&rt);
        let (reply, mut rx) = reply_pair();
        let mut n: u64 = 0;
        b.iter(|| {
            n += 1;
            let packet = query_bytes(0x0002, &format!("m{n}.bench.example."), RecordType::A);
            drive(&rt, &bench, &reply, &mut rx, &packet);
        });
    });

    // blocked: filter returns Block → NXDOMAIN, short-circuiting before the
    // cache and upstream.
    group.bench_function("blocked", |b| {
        let bench = setup(&rt);
        bench.set_filter_action(FilterAction::Block);
        let (reply, mut rx) = reply_pair();
        let packet = query_bytes(0x0003, "blocked.bench.example.", RecordType::A);
        b.iter(|| drive(&rt, &bench, &reply, &mut rx, &packet));
    });

    // rewrite: filter returns Rewrite → a synthetic A answer, before upstream.
    group.bench_function("rewrite", |b| {
        let bench = setup(&rt);
        bench.set_filter_action(FilterAction::Rewrite {
            ip: IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)),
        });
        let (reply, mut rx) = reply_pair();
        let packet = query_bytes(0x0004, "rewrite.bench.example.", RecordType::A);
        b.iter(|| drive(&rt, &bench, &reply, &mut rx, &packet));
    });

    // authoritative: the name lives in the local authoritative view, answered
    // before the cache and filter.
    group.bench_function("authoritative", |b| {
        let bench = setup(&rt);
        bench.set_authoritative_records(vec![a_record("auth.bench.lan", "10.0.0.5")]);
        let (reply, mut rx) = reply_pair();
        let packet = query_bytes(0x0005, "auth.bench.lan.", RecordType::A);
        b.iter(|| drive(&rt, &bench, &reply, &mut rx, &packet));
    });

    group.finish();
}

criterion_group!(benches, bench_resolution);
criterion_main!(benches);
