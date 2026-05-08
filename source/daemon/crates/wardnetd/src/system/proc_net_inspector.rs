//! Real Linux [`NetworkInspector`] implementation.
//!
//! Reads `/proc/net/route` for the default gateway and looks for
//! `/etc/dhcpcd.conf.d/wardnet.conf` (written by `install.sh
//! --static-ip`) to classify the IP source. Both reads are cheap and
//! synchronous so we run them on a `spawn_blocking` thread to keep
//! the async runtime clear.
//!
//! The LAN interface name and IP are passed in at construction time
//! since they're derived from the daemon's bootstrap (`config.toml`
//! plus IP detection in `main.rs`); re-deriving them here would
//! duplicate that logic.

use std::net::Ipv4Addr;
use std::path::PathBuf;

use async_trait::async_trait;
use wardnet_common::api::DhcpSource;
use wardnetd_services::system::{NetworkInspector, NetworkSnapshot};

/// Path written by `install.sh --static-ip`. Presence is treated as
/// the canonical signal that the operator pinned a static IP.
const WARDNET_DHCPCD_DROPIN: &str = "/etc/dhcpcd.conf.d/wardnet.conf";

/// Path the kernel exposes for the system route table.
const PROC_NET_ROUTE: &str = "/proc/net/route";

/// Real `NetworkInspector` backed by `/proc` + the dhcpcd drop-in.
pub struct ProcNetNetworkInspector {
    interface: String,
    ip: Ipv4Addr,
    dropin_path: PathBuf,
    proc_route_path: PathBuf,
}

impl ProcNetNetworkInspector {
    #[must_use]
    pub fn new(interface: String, ip: Ipv4Addr) -> Self {
        Self {
            interface,
            ip,
            dropin_path: PathBuf::from(WARDNET_DHCPCD_DROPIN),
            proc_route_path: PathBuf::from(PROC_NET_ROUTE),
        }
    }

    /// Test-only constructor with overridable file paths.
    #[cfg(test)]
    pub(crate) fn with_paths(
        interface: String,
        ip: Ipv4Addr,
        dropin_path: PathBuf,
        proc_route_path: PathBuf,
    ) -> Self {
        Self {
            interface,
            ip,
            dropin_path,
            proc_route_path,
        }
    }
}

#[async_trait]
impl NetworkInspector for ProcNetNetworkInspector {
    async fn inspect(&self) -> anyhow::Result<NetworkSnapshot> {
        let interface = self.interface.clone();
        let ip = self.ip;
        let dropin_path = self.dropin_path.clone();
        let proc_route_path = self.proc_route_path.clone();

        // Move the blocking file reads off the async runtime.
        let (gateway, dhcp_source) = tokio::task::spawn_blocking(move || {
            let gateway = read_default_gateway(&proc_route_path, &interface);
            let dhcp_source = classify_dhcp_source(&dropin_path);
            (gateway, dhcp_source)
        })
        .await
        .map_err(|e| anyhow::anyhow!("network inspector blocking task failed: {e}"))?;

        Ok(NetworkSnapshot {
            interface: self.interface.clone(),
            ip,
            gateway,
            dhcp_source,
        })
    }
}

/// Parse `/proc/net/route` to find the default gateway for the named
/// interface.
///
/// Format (tab-separated, one header line then one row per route):
///
/// ```text
/// Iface Destination Gateway Flags RefCnt Use Metric Mask MTU Window IRTT
/// eth0  00000000    0102A8C0 0003  0     0   0      00000000 0   0      0
/// ```
///
/// The default route has `Destination == 00000000`. `Gateway` is a
/// little-endian hex `u32` (the kernel writes raw bytes in network
/// order, but `/proc/net/route` swaps to host order first which on
/// little-endian boxes — the only architectures we ship for — means
/// the printed bytes are reversed relative to dotted-quad notation).
fn read_default_gateway(path: &std::path::Path, interface: &str) -> Option<Ipv4Addr> {
    let contents = std::fs::read_to_string(path).ok()?;
    for line in contents.lines().skip(1) {
        let mut cols = line.split_whitespace();
        let iface = cols.next()?;
        let destination = cols.next()?;
        let gateway = cols.next()?;
        if iface != interface || destination != "00000000" {
            continue;
        }
        let raw = u32::from_str_radix(gateway, 16).ok()?;
        if raw == 0 {
            return None;
        }
        // /proc/net/route is little-endian: bytes are [d, c, b, a]
        // for "a.b.c.d", so swap before constructing the Ipv4Addr.
        return Some(Ipv4Addr::from(raw.swap_bytes()));
    }
    None
}

/// Treat the presence of `install.sh`'s drop-in as the static-IP
/// signal. We deliberately don't try to parse dhcpcd state — the
/// drop-in is the operator's intent, and matching against it tells
/// the wizard what to surface in the remediation panel.
fn classify_dhcp_source(dropin_path: &std::path::Path) -> DhcpSource {
    if dropin_path.exists() {
        DhcpSource::Static
    } else {
        DhcpSource::Dhcp
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

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
}
