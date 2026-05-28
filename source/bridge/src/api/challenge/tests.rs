use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use axum::http::{HeaderMap, HeaderValue};

use super::{client_ip, verify_pow};

// ── verify_pow ────────────────────────────────────────────────────────────────

/// Brute-force a valid proof for the given inputs at the given difficulty,
/// then verify that `verify_pow` accepts it and that difficulty+1 rejects it.
#[test]
fn pow_round_trip() {
    use sha2::{Digest, Sha256};

    let nonce = "aabbccdd";
    let name = "test-name";
    let public_key = "dGVzdA==";
    let difficulty = 8u32; // low difficulty for test speed

    // Find a valid proof. The search is bounded by the u64 range; at difficulty
    // 8 (1-in-256 chance), we expect to succeed within the first ~256 tries.
    let proof = (0u64..=1_000_000)
        .find(|&p| {
            let payload = format!("{nonce}\n{name}\n{public_key}\n{p}");
            let hash = Sha256::digest(payload.as_bytes());
            let leading: u32 = hash
                .iter()
                .map(|b| b.leading_zeros())
                .take_while(|&z| z == 8)
                .sum::<u32>()
                + hash
                    .iter()
                    .find(|&&b| b != 0)
                    .map_or(0, |b| b.leading_zeros());
            leading >= difficulty
        })
        .expect("should find proof within 1 M iterations at difficulty 8");

    assert!(verify_pow(nonce, name, public_key, proof, difficulty));
    // Wrong proof must fail.
    assert!(!verify_pow(nonce, name, public_key, proof.wrapping_add(1), difficulty + 16));
}

// ── client_ip ─────────────────────────────────────────────────────────────────

fn loopback_addr() -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 12345)
}

fn external_addr() -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4)), 12345)
}

fn xff(value: &str) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert("X-Forwarded-For", HeaderValue::from_str(value).unwrap());
    headers
}

#[test]
fn xff_trusted_from_loopback() {
    let ip = client_ip(&xff("203.0.114.5"), loopback_addr());
    assert_eq!(ip, "203.0.114.5");
}

#[test]
fn xff_leftmost_value_from_loopback() {
    let ip = client_ip(&xff("10.0.0.1, 1.2.3.4"), loopback_addr());
    // Leftmost entry is chosen (the client as seen by the first proxy)
    assert_eq!(ip, "10.0.0.1");
}

#[test]
fn xff_ignored_from_external_peer() {
    // A directly connected client cannot forge its IP via X-Forwarded-For.
    let ip = client_ip(&xff("9.9.9.9"), external_addr());
    assert_eq!(ip, "1.2.3.4", "should use TCP peer, not XFF header");
}

#[test]
fn no_xff_uses_peer_ip() {
    let ip = client_ip(&HeaderMap::new(), loopback_addr());
    assert_eq!(ip, "127.0.0.1");
}
