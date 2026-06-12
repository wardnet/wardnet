use axum::http::{HeaderMap, HeaderValue};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

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
    assert!(!verify_pow(
        nonce,
        name,
        public_key,
        proof.wrapping_add(1),
        difficulty + 16
    ));
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

/// Behind the L4 proxy the listener injects the PROXY-supplied client address as
/// `ConnectInfo` (a non-loopback peer), so two distinct real clients key the
/// per-IP rate limit independently and neither can spoof the other via a forged
/// `X-Forwarded-For`. This is the property that keeps the registration limits
/// per-client rather than collapsing to the proxy's single address.
#[test]
fn proxy_supplied_ips_are_independent_and_unspoofable() {
    let client_a = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(203, 0, 113, 7)), 51000);
    let client_b = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(198, 51, 100, 9)), 51000);

    // Each real client keys on its own (proxy-supplied) address …
    let key_a = client_ip(&HeaderMap::new(), client_a);
    let key_b = client_ip(&HeaderMap::new(), client_b);
    assert_eq!(key_a, "203.0.113.7");
    assert_eq!(key_b, "198.51.100.9");
    assert_ne!(
        key_a, key_b,
        "distinct clients must get distinct rate-limit keys"
    );

    // … and a forged X-Forwarded-For cannot collapse them onto one budget.
    assert_eq!(client_ip(&xff("203.0.113.7"), client_b), "198.51.100.9");
}
