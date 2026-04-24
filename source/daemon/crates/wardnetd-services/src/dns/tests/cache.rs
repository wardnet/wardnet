use hickory_proto::op::{Message, OpCode};
use hickory_proto::rr::RecordType;

use crate::dns::cache::DnsCache;

fn make_response() -> Message {
    Message::response(0, OpCode::Query)
}

#[test]
fn insert_and_get() {
    let mut cache = DnsCache::new(100);
    let resp = make_response();
    cache.insert("example.com", RecordType::A, resp, 300, 0, 86400);
    assert_eq!(cache.len(), 1);
    assert!(cache.get("example.com", RecordType::A).is_some());
    assert!(cache.get("other.com", RecordType::A).is_none());
}

#[test]
fn case_insensitive() {
    let mut cache = DnsCache::new(100);
    cache.insert("Example.COM", RecordType::A, make_response(), 300, 0, 86400);
    assert!(cache.get("example.com", RecordType::A).is_some());
}

#[test]
fn zero_ttl_not_cached() {
    let mut cache = DnsCache::new(100);
    cache.insert("example.com", RecordType::A, make_response(), 0, 0, 86400);
    assert!(cache.is_empty());
}

#[test]
fn flush_clears_all() {
    let mut cache = DnsCache::new(100);
    cache.insert("a.com", RecordType::A, make_response(), 300, 0, 86400);
    cache.insert("b.com", RecordType::A, make_response(), 300, 0, 86400);
    assert_eq!(cache.flush(), 2);
    assert!(cache.is_empty());
}

#[test]
fn evicts_when_at_capacity() {
    let mut cache = DnsCache::new(2);
    cache.insert("a.com", RecordType::A, make_response(), 300, 0, 86400);
    cache.insert("b.com", RecordType::A, make_response(), 300, 0, 86400);
    cache.insert("c.com", RecordType::A, make_response(), 300, 0, 86400);
    assert_eq!(cache.len(), 2);
    // Oldest (a.com) should have been evicted.
    assert!(cache.get("a.com", RecordType::A).is_none());
}

#[test]
fn hit_rate_tracking() {
    let mut cache = DnsCache::new(100);
    cache.insert("a.com", RecordType::A, make_response(), 300, 0, 86400);
    cache.get("a.com", RecordType::A); // hit
    cache.get("b.com", RecordType::A); // miss
    assert_eq!(cache.hits(), 1);
    assert_eq!(cache.misses(), 1);
    assert!((cache.hit_rate() - 0.5).abs() < f64::EPSILON);
}

#[test]
fn ttl_min_clamp() {
    let mut cache = DnsCache::new(100);
    // TTL of 5 should be clamped up to min of 60.
    cache.insert("a.com", RecordType::A, make_response(), 5, 60, 86400);
    assert_eq!(cache.len(), 1);
}

#[test]
fn different_record_types_cached_separately() {
    let mut cache = DnsCache::new(100);
    cache.insert("a.com", RecordType::A, make_response(), 300, 0, 86400);
    cache.insert("a.com", RecordType::AAAA, make_response(), 300, 0, 86400);
    assert_eq!(cache.len(), 2);
    assert!(cache.get("a.com", RecordType::A).is_some());
    assert!(cache.get("a.com", RecordType::AAAA).is_some());
}
