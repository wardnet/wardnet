//! Infrastructure tests for [`TcpDeviceProber`] (issue #1116).
//!
//! Driven against real loopback sockets rather than a mock: the whole point of
//! the prober is what a TCP connect does, so a test that stubbed the connect
//! would assert nothing.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::{Duration, Instant};

use futures::future::BoxFuture;
use futures::stream::FuturesUnordered;
use tokio::net::TcpListener;

use crate::device::prober::{DeviceProber, TcpDeviceProber, collect_until};

/// Bind a listener on an ephemeral loopback port and return the port. The
/// listener is returned alongside so the caller keeps it alive — dropping it
/// closes the socket and the probe would find nothing.
async fn listening_port() -> (TcpListener, u16) {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
    let port = listener.local_addr().unwrap().port();
    (listener, port)
}

/// A port nothing listens on, so loopback answers with RST rather than
/// dropping the SYN.
///
/// Deliberately NOT an ephemeral port that was bound and released: these tests
/// run concurrently in one process, and the sibling `listening_port()` calls
/// bind port 0 — the OS is free to hand one of them the port we just freed,
/// at which point the "closed" port answers and the assertion flips. Port 1
/// (tcpmux) is outside the ephemeral range and is not something anything in
/// the test image binds.
const CLOSED_PORT: u16 = 1;

#[tokio::test]
async fn an_answering_port_is_reported_and_a_closed_one_is_not() {
    let (_listener, open) = listening_port().await;

    let answering = TcpDeviceProber
        .probe(IpAddr::V4(Ipv4Addr::LOCALHOST), &[open, CLOSED_PORT])
        .await;

    assert_eq!(answering, vec![open]);
}

#[tokio::test]
async fn answering_ports_come_back_sorted() {
    // The ports are probed concurrently, so completion order is arbitrary. The
    // UI renders them as a list, and a list that reshuffles between probes of
    // the same device reads as a change that did not happen.
    let (_a, first) = listening_port().await;
    let (_b, second) = listening_port().await;

    let mut expected = vec![first, second];
    expected.sort_unstable();

    let answering = TcpDeviceProber
        .probe(IpAddr::V4(Ipv4Addr::LOCALHOST), &[second, first])
        .await;

    assert_eq!(answering, expected);
}

#[tokio::test]
async fn probing_nothing_contacts_nothing() {
    let answering = TcpDeviceProber
        .probe(IpAddr::V4(Ipv4Addr::LOCALHOST), &[])
        .await;

    assert!(answering.is_empty());
}

#[tokio::test]
async fn a_black_holed_address_is_bounded_rather_than_hanging() {
    // 240.0.0.0/4 is reserved for future use (RFC 1112 §4): never allocated,
    // never routed, so the SYN is dropped or the host is unreachable — either
    // way nothing answers. Deliberately *not* one of the TEST-NET blocks: those
    // are documentation ranges that captive portals and some ISP resolvers
    // happily route to an interception box, which answers and fails the test.
    //
    // Without the per-port timeout this connect would sit on the OS default
    // (~75 s on Linux) and hold the synchronous handler open.
    let started = Instant::now();

    let answering = TcpDeviceProber
        .probe(IpAddr::V4(Ipv4Addr::new(240, 0, 0, 1)), &[4003, 6053])
        .await;

    assert!(answering.is_empty());
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "probe should be bounded, took {:?}",
        started.elapsed()
    );
}

#[tokio::test]
async fn every_requested_port_is_contacted() {
    // The service passes the catalog's full port list and reports it back to
    // the admin as `ports_probed`; a prober that quit after the first hit would
    // make that claim false.
    let (_a, first) = listening_port().await;
    let (_b, second) = listening_port().await;
    let (_c, third) = listening_port().await;

    let mut expected = vec![first, second, third];
    expected.sort_unstable();

    let answering = TcpDeviceProber
        .probe(IpAddr::V4(Ipv4Addr::LOCALHOST), &expected)
        .await;

    assert_eq!(answering, expected);
}

#[tokio::test]
async fn a_v6_loopback_listener_answers_too() {
    // Devices reachable over IPv6 are probed the same way; the address is a
    // parameter, not a v4 assumption.
    let listener = TcpListener::bind("[::1]:0".parse::<SocketAddr>().unwrap())
        .await
        .unwrap();
    let port = listener.local_addr().unwrap().port();

    let answering = TcpDeviceProber.probe("::1".parse().unwrap(), &[port]).await;

    assert_eq!(answering, vec![port]);
}

// ---------------------------------------------------------------------------
// The wall-clock cap, exercised through `collect_until`
//
// Driven here rather than through `probe` because loopback resolves every
// connect immediately — there is no socket that can be made to hang on demand,
// so a `probe`-level test could never get an attempt still in flight when the
// cap fires, which is the only case that matters.
// ---------------------------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn the_cap_keeps_the_ports_that_already_answered() {
    // Regression: the cap used to wrap `join_all`, which is all-or-nothing —
    // firing it dropped the whole future and returned an empty vec, so a device
    // whose Tuya port answered in milliseconds was reported as "no known ports
    // answered" purely because some other port was still waiting on a dropped
    // SYN. Silent, and indistinguishable from a genuine negative.
    let mut attempts: FuturesUnordered<BoxFuture<'static, Option<u16>>> = FuturesUnordered::new();
    attempts.push(Box::pin(async {
        tokio::time::sleep(Duration::from_millis(10)).await;
        Some(6668u16)
    }));
    attempts.push(Box::pin(async {
        // Still in flight when the cap fires.
        tokio::time::sleep(Duration::from_mins(1)).await;
        Some(4003u16)
    }));

    let deadline = tokio::time::Instant::now() + Duration::from_millis(300);
    let answering = collect_until(deadline, &mut attempts).await;

    assert_eq!(
        answering,
        vec![6668],
        "the port that answered before the cap must survive it"
    );
}

#[tokio::test(start_paused = true)]
async fn the_cap_still_bounds_a_probe_where_nothing_answers() {
    let mut attempts: FuturesUnordered<BoxFuture<'static, Option<u16>>> = FuturesUnordered::new();
    attempts.push(Box::pin(async {
        tokio::time::sleep(Duration::from_mins(1)).await;
        Some(4003u16)
    }));

    let started = tokio::time::Instant::now();
    let deadline = started + Duration::from_millis(300);
    let answering = collect_until(deadline, &mut attempts).await;

    assert!(answering.is_empty());
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "the cap must cut the probe off, took {:?}",
        started.elapsed()
    );
}

#[tokio::test(start_paused = true)]
async fn every_attempt_is_kept_when_they_all_finish_in_time() {
    let mut attempts: FuturesUnordered<BoxFuture<'static, Option<u16>>> = FuturesUnordered::new();
    attempts.push(Box::pin(async { Some(6668u16) }));
    attempts.push(Box::pin(async { None }));
    attempts.push(Box::pin(async { Some(8009u16) }));

    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    let mut answering = collect_until(deadline, &mut attempts).await;
    answering.sort_unstable();

    assert_eq!(answering, vec![6668, 8009]);
}
