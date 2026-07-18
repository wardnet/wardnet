//! Tests for the /proc-backed network inspector.

use std::net::Ipv4Addr;
use std::path::PathBuf;

use tempfile::TempDir;
use wardnet_common::api::DhcpSource;
use wardnetd_services::system::NetworkInspector;

use crate::system::proc_net_inspector::{
    ProcNetNetworkInspector, classify_dhcp_source, read_default_gateway,
};

use std::fs;

fn proc_route(iface: &str, gateway_hex: &str) -> String {
    format!(
        "Iface\tDestination\tGateway\tFlags\tRefCnt\tUse\tMetric\tMask\tMTU\tWindow\tIRTT\n\
         {iface}\t00000000\t{gateway_hex}\t0003\t0\t0\t0\t00000000\t0\t0\t0\n",
    )
}

fn write_route_file(dir: &TempDir, contents: &str) -> PathBuf {
    let p = dir.path().join("route");
    fs::write(&p, contents).unwrap();
    p
}

#[test]
fn parses_little_endian_gateway() {
    let dir = TempDir::new().unwrap();
    // 192.168.1.1 → little-endian bytes 01 01 A8 C0 → "0101A8C0".
    let route = write_route_file(&dir, &proc_route("eth0", "0101A8C0"));
    let gw = read_default_gateway(&route, "eth0").unwrap();
    assert_eq!(gw, Ipv4Addr::new(192, 168, 1, 1));
}

#[test]
fn returns_none_when_no_default_route() {
    let dir = TempDir::new().unwrap();
    let route = write_route_file(
        &dir,
        "Iface\tDestination\tGateway\tFlags\tRefCnt\tUse\tMetric\tMask\tMTU\tWindow\tIRTT\n",
    );
    assert!(read_default_gateway(&route, "eth0").is_none());
}

#[test]
fn returns_none_when_interface_does_not_match() {
    let dir = TempDir::new().unwrap();
    let route = write_route_file(&dir, &proc_route("wlan0", "0101A8C0"));
    assert!(read_default_gateway(&route, "eth0").is_none());
}

#[test]
fn returns_none_when_gateway_is_zero() {
    let dir = TempDir::new().unwrap();
    let route = write_route_file(&dir, &proc_route("eth0", "00000000"));
    assert!(read_default_gateway(&route, "eth0").is_none());
}

#[test]
fn classify_static_when_dropin_present() {
    let dir = TempDir::new().unwrap();
    let p = dir.path().join("wardnet.conf");
    fs::write(&p, "static ip_address=10.0.0.2/24\n").unwrap();
    assert_eq!(classify_dhcp_source(&p), DhcpSource::Static);
}

#[test]
fn classify_dhcp_when_dropin_missing() {
    let dir = TempDir::new().unwrap();
    let p = dir.path().join("wardnet.conf");
    assert_eq!(classify_dhcp_source(&p), DhcpSource::Dhcp);
}

#[tokio::test]
async fn inspect_returns_full_snapshot() {
    let dir = TempDir::new().unwrap();
    let route = write_route_file(&dir, &proc_route("eth0", "0101A8C0"));
    let dropin = dir.path().join("wardnet.conf");
    fs::write(&dropin, "static ip_address=10.0.0.2/24\n").unwrap();

    let inspector = ProcNetNetworkInspector::with_paths(
        "eth0".to_owned(),
        Ipv4Addr::new(10, 0, 0, 2),
        dropin,
        route,
    );

    let snap = inspector.inspect().await.unwrap();
    assert_eq!(snap.interface, "eth0");
    assert_eq!(snap.ip, Ipv4Addr::new(10, 0, 0, 2));
    assert_eq!(snap.gateway, Some(Ipv4Addr::new(192, 168, 1, 1)));
    assert_eq!(snap.dhcp_source, DhcpSource::Static);
}