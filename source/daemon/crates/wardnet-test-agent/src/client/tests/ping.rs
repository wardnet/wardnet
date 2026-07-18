//! Tests for the ping-output parser regexes.

use crate::client::ping::{RTT_RE, SUMMARY_RE};

#[test]
fn parses_summary_and_rtt() {
    let raw = "PING 1.1.1.1 (1.1.1.1): 56 data bytes\n\
        64 bytes from 1.1.1.1: icmp_seq=0 ttl=58 time=8.123 ms\n\
        \n\
        --- 1.1.1.1 ping statistics ---\n\
        5 packets transmitted, 5 received, 0% packet loss, time 4006ms\n\
        rtt min/avg/max/mdev = 7.123/8.456/9.789/0.910 ms\n";
    let cap = SUMMARY_RE.captures(raw).expect("summary matched");
    assert_eq!(&cap[1], "5");
    assert_eq!(&cap[2], "5");
    assert_eq!(&cap[3], "0");

    let rtt = RTT_RE.captures(raw).expect("rtt matched");
    assert_eq!(&rtt[1], "8.456");
}

#[test]
fn parses_partial_loss() {
    let raw = "3 packets transmitted, 1 received, 66% packet loss, time 2003ms\n";
    let cap = SUMMARY_RE.captures(raw).expect("summary matched");
    assert_eq!(&cap[1], "3");
    assert_eq!(&cap[2], "1");
    assert_eq!(&cap[3], "66");
    assert!(RTT_RE.captures(raw).is_none());
}
