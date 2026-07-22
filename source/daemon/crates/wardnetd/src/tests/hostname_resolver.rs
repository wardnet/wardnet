use std::net::{IpAddr, Ipv4Addr};

use crate::hostname_resolver::{SystemHostnameResolver, parse_getent_hosts};
use wardnetd_services::device::hostname_resolver::HostnameResolver;

#[tokio::test]
async fn resolve_loopback_does_not_panic() {
    let resolver = SystemHostnameResolver;

    // Reverse resolution of the loopback address depends on `/etc/hosts` and the
    // system resolver, which vary across CI runners — so the *value* is not
    // deterministic and is covered by `parse_getent_hosts` unit tests below.
    // This case only pins down that a live resolve returns without panicking and,
    // when it does yield a name, that the name is non-empty (never `Some("")`).
    if let Some(hostname) = resolver.resolve("127.0.0.1").await {
        assert!(
            !hostname.is_empty(),
            "a resolved hostname must not be empty"
        );
    }
}

// ── parse_getent_hosts: deterministic coverage of the parsing + guard ──────

const LOOPBACK: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);

#[test]
fn parse_getent_extracts_hostname() {
    // getent hosts output is "<ip>\t<hostname> [aliases...]".
    assert_eq!(
        parse_getent_hosts("127.0.0.1\tlocalhost\n", LOOPBACK),
        Some("localhost".to_owned())
    );
}

#[test]
fn parse_getent_takes_first_name_and_strips_trailing_dot() {
    // Only the first name is kept; a FQDN's trailing dot is trimmed. A regression
    // in the `nth(1)`/`trim_end_matches` logic would change this result.
    assert_eq!(
        parse_getent_hosts("192.168.1.10\thost.local. alias1 alias2\n", LOOPBACK),
        Some("host.local".to_owned())
    );
}

#[test]
fn parse_getent_rejects_ip_only_line() {
    // When getent echoes just the IP (no name), the guard must return None
    // rather than surface the IP as a bogus hostname.
    assert_eq!(parse_getent_hosts("127.0.0.1\n", LOOPBACK), None);
    assert_eq!(parse_getent_hosts("127.0.0.1 127.0.0.1\n", LOOPBACK), None);
}

#[test]
fn parse_getent_rejects_empty_output() {
    assert_eq!(parse_getent_hosts("", LOOPBACK), None);
}

#[tokio::test]
async fn resolve_invalid_ip_returns_none() {
    let resolver = SystemHostnameResolver;

    // An invalid IP address should return None without panicking.
    let result = resolver.resolve("not-an-ip").await;
    assert!(result.is_none());
}

#[tokio::test]
async fn resolve_unreachable_ip_returns_none() {
    let resolver = SystemHostnameResolver;

    // RFC 5737 documentation address, unlikely to resolve via DNS.
    let result = resolver.resolve("198.51.100.254").await;
    assert!(result.is_none());
}
