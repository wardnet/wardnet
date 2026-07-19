use std::net::Ipv4Addr;
use std::time::Duration;

use hickory_proto::op::{Message, OpCode};
use hickory_proto::rr::rdata::{A, SOA};
use hickory_proto::rr::{Name, RData, Record, RecordType};
use uuid::Uuid;
use wardnet_common::dns::UpstreamId;

use crate::dns::cache::DnsCache;

fn make_response() -> Message {
    Message::response(0, OpCode::Query)
}

fn make_answer_response(domain: &str, ttl: u32) -> Message {
    let mut resp = make_response();
    let name = Name::from_str_relaxed(domain).expect("answer name");
    resp.add_answer(Record::from_rdata(
        name,
        ttl,
        RData::A(A(Ipv4Addr::new(192, 0, 2, 1))),
    ));
    resp
}

const DEFAULT: UpstreamId = UpstreamId::Default;

#[test]
fn insert_and_get() {
    let mut cache = DnsCache::new(100);
    let resp = make_response();
    cache.insert(DEFAULT, "example.com", RecordType::A, resp, 300, 0, 86400);
    assert_eq!(cache.len(), 1);
    assert!(cache.get(DEFAULT, "example.com", RecordType::A).is_some());
    assert!(cache.get(DEFAULT, "other.com", RecordType::A).is_none());
}

#[test]
fn case_insensitive() {
    let mut cache = DnsCache::new(100);
    cache.insert(
        DEFAULT,
        "Example.COM",
        RecordType::A,
        make_response(),
        300,
        0,
        86400,
    );
    assert!(cache.get(DEFAULT, "example.com", RecordType::A).is_some());
}

#[test]
fn trailing_dot_normalized_across_insert_get_and_invalidate() {
    // The DNS server inserts under the wire-format FQDN ("foo.com.") while
    // eviction callers pass the bare name ("foo.com"); both forms must hit
    // the same entry or per-domain invalidation silently removes nothing.
    let mut cache = DnsCache::new(100);
    cache.insert(
        DEFAULT,
        "example.com.",
        RecordType::A,
        make_response(),
        300,
        0,
        86400,
    );
    assert!(
        cache.get(DEFAULT, "example.com", RecordType::A).is_some(),
        "bare-name lookup must hit the FQDN-inserted entry"
    );
    assert_eq!(
        cache.invalidate_domain("example.com"),
        1,
        "bare-name invalidation must evict the FQDN-inserted entry"
    );
    assert!(cache.is_empty());
}

#[test]
fn zero_ttl_not_cached() {
    let mut cache = DnsCache::new(100);
    cache.insert(
        DEFAULT,
        "example.com",
        RecordType::A,
        make_response(),
        0,
        0,
        86400,
    );
    assert!(cache.is_empty());
}

#[test]
fn flush_clears_all() {
    let mut cache = DnsCache::new(100);
    cache.insert(
        DEFAULT,
        "a.com",
        RecordType::A,
        make_response(),
        300,
        0,
        86400,
    );
    cache.insert(
        DEFAULT,
        "b.com",
        RecordType::A,
        make_response(),
        300,
        0,
        86400,
    );
    assert_eq!(cache.flush(), 2);
    assert!(cache.is_empty());
}

#[test]
fn evicts_when_at_capacity() {
    let mut cache = DnsCache::new(2);
    cache.insert(
        DEFAULT,
        "a.com",
        RecordType::A,
        make_response(),
        300,
        0,
        86400,
    );
    cache.insert(
        DEFAULT,
        "b.com",
        RecordType::A,
        make_response(),
        300,
        0,
        86400,
    );
    cache.insert(
        DEFAULT,
        "c.com",
        RecordType::A,
        make_response(),
        300,
        0,
        86400,
    );
    assert_eq!(cache.len(), 2);
    // Oldest (a.com) should have been evicted.
    assert!(cache.get(DEFAULT, "a.com", RecordType::A).is_none());
}

#[test]
fn hit_rate_tracking() {
    let mut cache = DnsCache::new(100);
    cache.insert(
        DEFAULT,
        "a.com",
        RecordType::A,
        make_response(),
        300,
        0,
        86400,
    );
    cache.get(DEFAULT, "a.com", RecordType::A); // hit
    cache.get(DEFAULT, "b.com", RecordType::A); // miss
    assert_eq!(cache.hits(), 1);
    assert_eq!(cache.misses(), 1);
    assert!((cache.hit_rate() - 0.5).abs() < f64::EPSILON);
}

#[test]
fn ttl_min_clamp() {
    let mut cache = DnsCache::new(100);
    // TTL of 5 should be clamped up to min of 60.
    cache.insert(
        DEFAULT,
        "a.com",
        RecordType::A,
        make_response(),
        5,
        60,
        86400,
    );
    assert_eq!(cache.len(), 1);
}

#[test]
fn different_record_types_cached_separately() {
    let mut cache = DnsCache::new(100);
    cache.insert(
        DEFAULT,
        "a.com",
        RecordType::A,
        make_response(),
        300,
        0,
        86400,
    );
    cache.insert(
        DEFAULT,
        "a.com",
        RecordType::AAAA,
        make_response(),
        300,
        0,
        86400,
    );
    assert_eq!(cache.len(), 2);
    assert!(cache.get(DEFAULT, "a.com", RecordType::A).is_some());
    assert!(cache.get(DEFAULT, "a.com", RecordType::AAAA).is_some());
}

#[test]
fn hit_ages_answer_and_authority_ttls() {
    let mut cache = DnsCache::new(100);
    let mut resp = make_answer_response("example.com.", 300);
    let soa = SOA::new(
        Name::from_str_relaxed("ns.example.com.").expect("mname"),
        Name::from_str_relaxed("hostmaster.example.com.").expect("rname"),
        1,
        3600,
        600,
        86400,
        60,
    );
    resp.add_authority(Record::from_rdata(
        Name::from_str_relaxed("example.com.").expect("soa name"),
        300,
        RData::SOA(soa),
    ));
    cache.insert(DEFAULT, "example.com", RecordType::A, resp, 300, 0, 86400);
    cache.backdate(
        DEFAULT,
        "example.com",
        RecordType::A,
        Duration::from_mins(2),
    );

    let hit = cache
        .get(DEFAULT, "example.com", RecordType::A)
        .expect("entry is still within its 300s lifetime");
    assert_eq!(
        hit.answers[0].ttl, 180,
        "answer TTL must shrink by time spent in cache"
    );
    assert_eq!(
        hit.authorities[0].ttl, 180,
        "authority TTL must shrink by time spent in cache"
    );
}

#[test]
fn fresh_hit_serves_ttl_no_larger_than_inserted() {
    let mut cache = DnsCache::new(100);
    cache.insert(
        DEFAULT,
        "example.com",
        RecordType::A,
        make_answer_response("example.com.", 300),
        300,
        0,
        86400,
    );
    let hit = cache
        .get(DEFAULT, "example.com", RecordType::A)
        .expect("hit");
    assert!(hit.answers[0].ttl <= 300);
    assert!(hit.answers[0].ttl >= 1);
}

#[test]
fn aged_ttl_floors_at_one_and_never_wraps() {
    let mut cache = DnsCache::new(100);
    // The record carries a 30s TTL but the admin floor keeps the entry
    // alive for 300s, so a late hit ages the record past zero.
    cache.insert(
        DEFAULT,
        "example.com",
        RecordType::A,
        make_answer_response("example.com.", 30),
        30,
        300,
        86400,
    );
    cache.backdate(
        DEFAULT,
        "example.com",
        RecordType::A,
        Duration::from_mins(2),
    );

    let hit = cache
        .get(DEFAULT, "example.com", RecordType::A)
        .expect("entry is still within its clamped 300s lifetime");
    assert_eq!(
        hit.answers[0].ttl, 1,
        "TTL must floor at 1, not go to zero or wrap"
    );
}

#[test]
fn expired_entry_is_not_served() {
    let mut cache = DnsCache::new(100);
    cache.insert(
        DEFAULT,
        "example.com",
        RecordType::A,
        make_answer_response("example.com.", 300),
        300,
        0,
        86400,
    );
    cache.backdate(
        DEFAULT,
        "example.com",
        RecordType::A,
        Duration::from_mins(5),
    );

    assert!(cache.get(DEFAULT, "example.com", RecordType::A).is_none());
    assert_eq!(cache.misses(), 1);

    // A fresh insert for the same key overwrites the expired entry and
    // serves again.
    cache.insert(
        DEFAULT,
        "example.com",
        RecordType::A,
        make_answer_response("example.com.", 300),
        300,
        0,
        86400,
    );
    assert_eq!(cache.len(), 1);
    assert!(cache.get(DEFAULT, "example.com", RecordType::A).is_some());
}

#[test]
fn sweep_reclaims_expired_entries_before_evicting_live_ones() {
    // At capacity, expired entries must be swept out rather than letting
    // FIFO eviction remove the oldest still-live entry while dead ones
    // keep occupying capacity.
    let mut cache = DnsCache::new(4);
    cache.insert(
        DEFAULT,
        "keep.com",
        RecordType::A,
        make_response(),
        300,
        0,
        86400,
    );
    for domain in ["e1.com", "e2.com", "e3.com"] {
        cache.insert(
            DEFAULT,
            domain,
            RecordType::A,
            make_response(),
            30,
            0,
            86400,
        );
        cache.backdate(DEFAULT, domain, RecordType::A, Duration::from_mins(1));
    }
    assert_eq!(cache.len(), 4);

    cache.insert(
        DEFAULT,
        "new.com",
        RecordType::A,
        make_response(),
        300,
        0,
        86400,
    );

    assert!(
        cache.get(DEFAULT, "keep.com", RecordType::A).is_some(),
        "the oldest live entry must survive while expired entries exist"
    );
    assert!(cache.get(DEFAULT, "new.com", RecordType::A).is_some());
    assert_eq!(
        cache.len(),
        2,
        "the sweep must reclaim all expired entries, correcting len()"
    );
}

#[test]
fn served_ttl_capped_by_remaining_entry_lifetime() {
    let mut cache = DnsCache::new(100);
    // Record TTL far above the admin's ttl_max clamp: the entry lives for
    // 300s, and served TTLs must never promise more than what's left of
    // that, or downstream caches outlive the cap.
    cache.insert(
        DEFAULT,
        "example.com",
        RecordType::A,
        make_answer_response("example.com.", 86400),
        86400,
        0,
        300,
    );
    cache.backdate(
        DEFAULT,
        "example.com",
        RecordType::A,
        Duration::from_secs(100),
    );

    let hit = cache
        .get(DEFAULT, "example.com", RecordType::A)
        .expect("entry is still within its clamped 300s lifetime");
    assert_eq!(
        hit.answers[0].ttl, 200,
        "served TTL must be capped at the entry's remaining lifetime"
    );
}

#[test]
fn zero_ttl_record_is_not_inflated() {
    let mut cache = DnsCache::new(100);
    let mut resp = make_answer_response("example.com.", 300);
    // An upstream TTL of 0 means "do not cache this record"; aging must
    // never raise it to 1.
    resp.add_authority(Record::from_rdata(
        Name::from_str_relaxed("example.com.").expect("authority name"),
        0,
        RData::A(A(Ipv4Addr::new(192, 0, 2, 2))),
    ));
    cache.insert(DEFAULT, "example.com", RecordType::A, resp, 300, 0, 86400);

    let hit = cache
        .get(DEFAULT, "example.com", RecordType::A)
        .expect("hit");
    assert_eq!(hit.authorities[0].ttl, 0, "TTL 0 must be preserved");
    assert_eq!(hit.answers[0].ttl, 300);
}

#[test]
fn hit_ages_additional_record_ttls() {
    // The forwarding paths cache the full upstream message, so glue
    // records in the additionals section must age like everything else.
    let mut cache = DnsCache::new(100);
    let mut resp = make_answer_response("example.com.", 300);
    resp.add_additional(Record::from_rdata(
        Name::from_str_relaxed("glue.example.com.").expect("glue name"),
        300,
        RData::A(A(Ipv4Addr::new(192, 0, 2, 3))),
    ));
    cache.insert(DEFAULT, "example.com", RecordType::A, resp, 300, 0, 86400);
    cache.backdate(
        DEFAULT,
        "example.com",
        RecordType::A,
        Duration::from_mins(2),
    );

    let hit = cache
        .get(DEFAULT, "example.com", RecordType::A)
        .expect("hit");
    assert_eq!(
        hit.additionals[0].ttl, 180,
        "additional-section TTLs must shrink by time spent in cache"
    );
}

#[test]
fn overwritten_entry_survives_its_stale_queue_slot() {
    let mut cache = DnsCache::new(2);
    cache.insert(
        DEFAULT,
        "a.com",
        RecordType::A,
        make_response(),
        300,
        0,
        86400,
    );
    cache.insert(
        DEFAULT,
        "b.com",
        RecordType::A,
        make_response(),
        300,
        0,
        86400,
    );
    // Overwrite a.com: its original queue slot goes stale, b.com becomes
    // the oldest live entry.
    cache.insert(
        DEFAULT,
        "a.com",
        RecordType::A,
        make_response(),
        300,
        0,
        86400,
    );
    cache.insert(
        DEFAULT,
        "c.com",
        RecordType::A,
        make_response(),
        300,
        0,
        86400,
    );

    assert_eq!(cache.len(), 2);
    assert!(
        cache.get(DEFAULT, "a.com", RecordType::A).is_some(),
        "re-inserted entry must not be evicted through its stale slot"
    );
    assert!(cache.get(DEFAULT, "b.com", RecordType::A).is_none());
    assert!(cache.get(DEFAULT, "c.com", RecordType::A).is_some());
}

#[test]
fn eviction_skips_slots_of_invalidated_entries() {
    let mut cache = DnsCache::new(2);
    cache.insert(
        DEFAULT,
        "a.com",
        RecordType::A,
        make_response(),
        300,
        0,
        86400,
    );
    cache.insert(
        DEFAULT,
        "b.com",
        RecordType::A,
        make_response(),
        300,
        0,
        86400,
    );
    assert_eq!(cache.invalidate_domain("a.com"), 1);
    cache.insert(
        DEFAULT,
        "c.com",
        RecordType::A,
        make_response(),
        300,
        0,
        86400,
    );
    cache.insert(
        DEFAULT,
        "d.com",
        RecordType::A,
        make_response(),
        300,
        0,
        86400,
    );

    assert_eq!(cache.len(), 2);
    assert!(
        cache.get(DEFAULT, "b.com", RecordType::A).is_none(),
        "b.com is the oldest live entry once a.com's slot is stale"
    );
    assert!(cache.get(DEFAULT, "c.com", RecordType::A).is_some());
    assert!(cache.get(DEFAULT, "d.com", RecordType::A).is_some());
}

#[test]
fn insert_at_capacity_keeps_eviction_work_bounded() {
    // Every insert past capacity should retire exactly one queue slot, so
    // the queue tracks the live-entry count instead of growing with the
    // total number of inserts (the old implementation re-scanned the whole
    // map per insert).
    let mut cache = DnsCache::new(64);
    for i in 0..5_000 {
        let domain = format!("host{i}.example.com");
        cache.insert(
            DEFAULT,
            &domain,
            RecordType::A,
            make_response(),
            300,
            0,
            86400,
        );
        assert!(cache.len() <= 64);
        assert!(
            cache.insertion_order_len() <= 128,
            "eviction queue must stay proportional to capacity, not inserts"
        );
    }
}

#[test]
fn repeated_overwrites_compact_stale_queue_slots() {
    let mut cache = DnsCache::new(1000);
    for _ in 0..10_000 {
        cache.insert(
            DEFAULT,
            "example.com",
            RecordType::A,
            make_response(),
            300,
            0,
            86400,
        );
    }
    assert_eq!(cache.len(), 1);
    assert!(
        cache.insertion_order_len() <= 64,
        "stale slots from overwrites must be compacted away"
    );
}

#[test]
fn upstream_id_separates_cache_entries() {
    let mut cache = DnsCache::new(100);
    let tunnel_id = Uuid::nil();
    cache.insert(
        UpstreamId::Default,
        "example.com",
        RecordType::A,
        make_response(),
        300,
        0,
        86400,
    );
    cache.insert(
        UpstreamId::Tunnel(tunnel_id),
        "example.com",
        RecordType::A,
        make_response(),
        300,
        0,
        86400,
    );
    assert_eq!(cache.len(), 2);
    assert!(
        cache
            .get(UpstreamId::Default, "example.com", RecordType::A)
            .is_some()
    );
    assert!(
        cache
            .get(UpstreamId::Tunnel(tunnel_id), "example.com", RecordType::A)
            .is_some()
    );
}
