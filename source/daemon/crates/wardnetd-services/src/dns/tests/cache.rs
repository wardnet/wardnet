use std::net::Ipv4Addr;
use std::time::Duration;

use hickory_proto::op::{Message, OpCode};
use hickory_proto::rr::rdata::{A, SOA};
use hickory_proto::rr::{Name, RData, Record, RecordType};
use hickory_proto::serialize::binary::{BinDecodable, BinEncodable};
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

/// Encode a response to the wire bytes the cache now stores.
fn wire(msg: &Message) -> Vec<u8> {
    msg.to_bytes().expect("encode response to wire")
}

/// Look a response up and decode the returned wire buffer back into a
/// `Message`, so the TTL/section assertions below can inspect it. Uses a
/// fixed transaction id — the tests that care assert on it explicitly.
fn get_msg(
    cache: &DnsCache,
    upstream: UpstreamId,
    domain: &str,
    rtype: RecordType,
) -> Option<Message> {
    cache
        .get(upstream, domain, rtype, 0)
        .map(|b| Message::from_bytes(&b).expect("decode cached wire response"))
}

const DEFAULT: UpstreamId = UpstreamId::Default;

#[test]
fn insert_and_get() {
    let mut cache = DnsCache::new(100);
    let resp = make_response();
    cache.insert(
        DEFAULT,
        "example.com",
        RecordType::A,
        wire(&resp),
        300,
        0,
        86400,
    );
    assert_eq!(cache.len(), 1);
    assert!(get_msg(&cache, DEFAULT, "example.com", RecordType::A).is_some());
    assert!(get_msg(&cache, DEFAULT, "other.com", RecordType::A).is_none());
}

#[test]
fn case_insensitive() {
    let mut cache = DnsCache::new(100);
    cache.insert(
        DEFAULT,
        "Example.COM",
        RecordType::A,
        wire(&make_response()),
        300,
        0,
        86400,
    );
    assert!(get_msg(&cache, DEFAULT, "example.com", RecordType::A).is_some());
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
        wire(&make_response()),
        300,
        0,
        86400,
    );
    assert!(
        get_msg(&cache, DEFAULT, "example.com", RecordType::A).is_some(),
        "bare-name lookup must hit the FQDN-inserted entry"
    );
    assert_eq!(
        cache.invalidate_subtree("example.com"),
        1,
        "bare-name invalidation must evict the FQDN-inserted entry"
    );
    assert!(cache.is_empty());
}

#[test]
fn invalidate_subtree_evicts_the_name_and_its_subdomains() {
    let mut cache = DnsCache::new(100);
    for domain in [
        "example.com",
        "mqtt.example.com",
        "a.b.example.com",
        // Neither of these is below `example.com`: a sibling that merely
        // ends with the same characters, and an unrelated name.
        "notexample.com",
        "example.com.evil.net",
    ] {
        cache.insert(
            DEFAULT,
            domain,
            RecordType::A,
            wire(&make_response()),
            300,
            0,
            86400,
        );
    }

    assert_eq!(cache.invalidate_subtree("example.com"), 3);
    assert!(get_msg(&cache, DEFAULT, "notexample.com", RecordType::A).is_some());
    assert!(get_msg(&cache, DEFAULT, "example.com.evil.net", RecordType::A).is_some());
    assert!(get_msg(&cache, DEFAULT, "mqtt.example.com", RecordType::A).is_none());
}

#[test]
fn invalidate_subtree_of_a_wildcard_evicts_its_suffix() {
    // A wildcard record is stored as `*.<suffix>` but serves the subtree
    // below the suffix, so that subtree is what has to be evicted.
    let mut cache = DnsCache::new(100);
    for domain in ["host.lab.internal", "other.net"] {
        cache.insert(
            DEFAULT,
            domain,
            RecordType::A,
            wire(&make_response()),
            300,
            0,
            86400,
        );
    }

    assert_eq!(cache.invalidate_subtree("*.lab.internal"), 1);
    assert!(get_msg(&cache, DEFAULT, "other.net", RecordType::A).is_some());
}

#[test]
fn invalidate_subtree_spans_upstreams_and_record_types() {
    let mut cache = DnsCache::new(100);
    for (upstream, rtype) in [
        (DEFAULT, RecordType::A),
        (DEFAULT, RecordType::AAAA),
        (UpstreamId::Tunnel(Uuid::nil()), RecordType::A),
    ] {
        cache.insert(
            upstream,
            "mqtt.example.com",
            rtype,
            wire(&make_response()),
            300,
            0,
            86400,
        );
    }

    assert_eq!(cache.invalidate_subtree("example.com"), 3);
    assert!(cache.is_empty());
}

#[test]
fn zero_ttl_not_cached() {
    let mut cache = DnsCache::new(100);
    cache.insert(
        DEFAULT,
        "example.com",
        RecordType::A,
        wire(&make_response()),
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
        wire(&make_response()),
        300,
        0,
        86400,
    );
    cache.insert(
        DEFAULT,
        "b.com",
        RecordType::A,
        wire(&make_response()),
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
        wire(&make_response()),
        300,
        0,
        86400,
    );
    cache.insert(
        DEFAULT,
        "b.com",
        RecordType::A,
        wire(&make_response()),
        300,
        0,
        86400,
    );
    cache.insert(
        DEFAULT,
        "c.com",
        RecordType::A,
        wire(&make_response()),
        300,
        0,
        86400,
    );
    assert_eq!(cache.len(), 2);
    // Oldest (a.com) should have been evicted.
    assert!(get_msg(&cache, DEFAULT, "a.com", RecordType::A).is_none());
}

#[test]
fn hit_rate_tracking() {
    let mut cache = DnsCache::new(100);
    cache.insert(
        DEFAULT,
        "a.com",
        RecordType::A,
        wire(&make_response()),
        300,
        0,
        86400,
    );
    let _ = get_msg(&cache, DEFAULT, "a.com", RecordType::A); // hit
    let _ = get_msg(&cache, DEFAULT, "b.com", RecordType::A); // miss
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
        wire(&make_response()),
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
        wire(&make_response()),
        300,
        0,
        86400,
    );
    cache.insert(
        DEFAULT,
        "a.com",
        RecordType::AAAA,
        wire(&make_response()),
        300,
        0,
        86400,
    );
    assert_eq!(cache.len(), 2);
    assert!(get_msg(&cache, DEFAULT, "a.com", RecordType::A).is_some());
    assert!(get_msg(&cache, DEFAULT, "a.com", RecordType::AAAA).is_some());
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
    cache.insert(
        DEFAULT,
        "example.com",
        RecordType::A,
        wire(&resp),
        300,
        0,
        86400,
    );
    cache.backdate(
        DEFAULT,
        "example.com",
        RecordType::A,
        Duration::from_mins(2),
    );

    let hit = get_msg(&cache, DEFAULT, "example.com", RecordType::A)
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
        wire(&make_answer_response("example.com.", 300)),
        300,
        0,
        86400,
    );
    let hit = get_msg(&cache, DEFAULT, "example.com", RecordType::A).expect("hit");
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
        wire(&make_answer_response("example.com.", 30)),
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

    let hit = get_msg(&cache, DEFAULT, "example.com", RecordType::A)
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
        wire(&make_answer_response("example.com.", 300)),
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

    assert!(get_msg(&cache, DEFAULT, "example.com", RecordType::A).is_none());
    assert_eq!(cache.misses(), 1);

    // A fresh insert for the same key overwrites the expired entry and
    // serves again.
    cache.insert(
        DEFAULT,
        "example.com",
        RecordType::A,
        wire(&make_answer_response("example.com.", 300)),
        300,
        0,
        86400,
    );
    assert_eq!(cache.len(), 1);
    assert!(get_msg(&cache, DEFAULT, "example.com", RecordType::A).is_some());
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
        wire(&make_response()),
        300,
        0,
        86400,
    );
    for domain in ["e1.com", "e2.com", "e3.com"] {
        cache.insert(
            DEFAULT,
            domain,
            RecordType::A,
            wire(&make_response()),
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
        wire(&make_response()),
        300,
        0,
        86400,
    );

    assert!(
        get_msg(&cache, DEFAULT, "keep.com", RecordType::A).is_some(),
        "the oldest live entry must survive while expired entries exist"
    );
    assert!(get_msg(&cache, DEFAULT, "new.com", RecordType::A).is_some());
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
        wire(&make_answer_response("example.com.", 86400)),
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

    let hit = get_msg(&cache, DEFAULT, "example.com", RecordType::A)
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
    cache.insert(
        DEFAULT,
        "example.com",
        RecordType::A,
        wire(&resp),
        300,
        0,
        86400,
    );

    let hit = get_msg(&cache, DEFAULT, "example.com", RecordType::A).expect("hit");
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
    cache.insert(
        DEFAULT,
        "example.com",
        RecordType::A,
        wire(&resp),
        300,
        0,
        86400,
    );
    cache.backdate(
        DEFAULT,
        "example.com",
        RecordType::A,
        Duration::from_mins(2),
    );

    let hit = get_msg(&cache, DEFAULT, "example.com", RecordType::A).expect("hit");
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
        wire(&make_response()),
        300,
        0,
        86400,
    );
    cache.insert(
        DEFAULT,
        "b.com",
        RecordType::A,
        wire(&make_response()),
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
        wire(&make_response()),
        300,
        0,
        86400,
    );
    cache.insert(
        DEFAULT,
        "c.com",
        RecordType::A,
        wire(&make_response()),
        300,
        0,
        86400,
    );

    assert_eq!(cache.len(), 2);
    assert!(
        get_msg(&cache, DEFAULT, "a.com", RecordType::A).is_some(),
        "re-inserted entry must not be evicted through its stale slot"
    );
    assert!(get_msg(&cache, DEFAULT, "b.com", RecordType::A).is_none());
    assert!(get_msg(&cache, DEFAULT, "c.com", RecordType::A).is_some());
}

#[test]
fn eviction_skips_slots_of_invalidated_entries() {
    let mut cache = DnsCache::new(2);
    cache.insert(
        DEFAULT,
        "a.com",
        RecordType::A,
        wire(&make_response()),
        300,
        0,
        86400,
    );
    cache.insert(
        DEFAULT,
        "b.com",
        RecordType::A,
        wire(&make_response()),
        300,
        0,
        86400,
    );
    assert_eq!(cache.invalidate_subtree("a.com"), 1);
    cache.insert(
        DEFAULT,
        "c.com",
        RecordType::A,
        wire(&make_response()),
        300,
        0,
        86400,
    );
    cache.insert(
        DEFAULT,
        "d.com",
        RecordType::A,
        wire(&make_response()),
        300,
        0,
        86400,
    );

    assert_eq!(cache.len(), 2);
    assert!(
        get_msg(&cache, DEFAULT, "b.com", RecordType::A).is_none(),
        "b.com is the oldest live entry once a.com's slot is stale"
    );
    assert!(get_msg(&cache, DEFAULT, "c.com", RecordType::A).is_some());
    assert!(get_msg(&cache, DEFAULT, "d.com", RecordType::A).is_some());
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
            wire(&make_response()),
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
            wire(&make_response()),
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
        wire(&make_response()),
        300,
        0,
        86400,
    );
    cache.insert(
        UpstreamId::Tunnel(tunnel_id),
        "example.com",
        RecordType::A,
        wire(&make_response()),
        300,
        0,
        86400,
    );
    assert_eq!(cache.len(), 2);
    assert!(get_msg(&cache, UpstreamId::Default, "example.com", RecordType::A).is_some());
    assert!(
        get_msg(
            &cache,
            UpstreamId::Tunnel(tunnel_id),
            "example.com",
            RecordType::A
        )
        .is_some()
    );
}

#[test]
fn hit_stamps_the_querying_transaction_id() {
    let mut cache = DnsCache::new(100);
    cache.insert(
        DEFAULT,
        "example.com",
        RecordType::A,
        wire(&make_answer_response("example.com.", 300)),
        300,
        0,
        86400,
    );
    // Every client that hits this entry supplies its own transaction id; the
    // hit must carry it, not the id baked into the cached bytes.
    let bytes = cache
        .get(DEFAULT, "example.com", RecordType::A, 0xBEEF)
        .expect("hit");
    assert_eq!(
        &bytes[..2],
        &0xBEEF_u16.to_be_bytes(),
        "the first two wire bytes are the client's txid"
    );
    assert_eq!(
        Message::from_bytes(&bytes).expect("decode").metadata.id,
        0xBEEF
    );
}

#[test]
fn hit_ages_records_but_leaves_the_edns_opt_untouched() {
    use hickory_proto::op::Edns;
    // The tunnel/conditional paths cache the raw upstream datagram, which
    // routinely carries an EDNS OPT record in the additional section. Its
    // "TTL" slot is really extended-rcode + version + flags, so the aging
    // walk must skip it (RR type 41) or it corrupts EDNS.
    let mut cache = DnsCache::new(100);
    let mut resp = make_answer_response("example.com.", 300);
    let mut edns = Edns::new();
    // Version 1 puts a nonzero byte in the OPT record's "TTL" slot, so a walk
    // that failed to skip type-41 records would actually corrupt it (a
    // version-0 slot is all-zero and the `ttl != 0` guard would spare it
    // anyway, testing nothing).
    edns.set_version(1);
    edns.set_max_payload(4096);
    resp.set_edns(edns);
    cache.insert(
        DEFAULT,
        "example.com",
        RecordType::A,
        wire(&resp),
        300,
        0,
        86400,
    );
    cache.backdate(
        DEFAULT,
        "example.com",
        RecordType::A,
        Duration::from_mins(2),
    );

    let hit = get_msg(&cache, DEFAULT, "example.com", RecordType::A).expect("hit");
    assert_eq!(hit.answers[0].ttl, 180, "the real answer TTL still ages");
    assert_eq!(
        hit.version(),
        1,
        "the OPT version byte (in the TTL slot) must be left untouched"
    );
    assert_eq!(
        hit.max_payload(),
        4096,
        "the OPT record must survive intact"
    );
}
