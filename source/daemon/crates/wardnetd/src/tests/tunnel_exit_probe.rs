//! Tests for the tunnel exit-probe trace-body parser.

use wardnetd_services::tunnel::exit_probe::ProbeError;

use crate::tunnel_exit_probe::parse_trace_body;

#[test]
fn parse_trace_body_extracts_ip_and_loc() {
    let body = "fl=53f1\nh=1.1.1.1\nip=203.0.113.42\nts=1714000000.0\nvisit_scheme=https\nuag=test\ncolo=AMS\nsliver=none\nhttp=http/2\nloc=NL\ntls=TLSv1.3\nsni=plaintext\nwarp=off\ngateway=off\nrbi=off\nkex=X25519\n";
    let info = parse_trace_body(body).expect("parse ok");
    assert_eq!(info.ip, "203.0.113.42");
    assert_eq!(info.country_code, "NL");
}

#[test]
fn parse_trace_body_missing_ip_errors() {
    let body = "loc=DE\n";
    assert!(matches!(parse_trace_body(body), Err(ProbeError::Parse(_))));
}

#[test]
fn parse_trace_body_missing_loc_errors() {
    let body = "ip=203.0.113.1\n";
    assert!(matches!(parse_trace_body(body), Err(ProbeError::Parse(_))));
}