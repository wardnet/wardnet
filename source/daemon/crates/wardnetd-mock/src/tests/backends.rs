//! Tests for the no-op backend implementations.
//!
//! These verify that every trait method returns `Ok(())` (or a sensible
//! default) and that the stateful ones (DHCP/DNS servers) update their
//! state correctly. Coverage for the files themselves is excluded by
//! the workspace-level `noop_*` regex, but we still want to confirm the
//! traits compile and don't panic.

use tokio_util::sync::CancellationToken;
use wardnetd_services::device::{HostnameResolver, PacketCapture};
use wardnetd_services::dhcp::server::DhcpServer;
use wardnetd_services::dns::server::DnsServer;
use wardnetd_services::routing::{FirewallManager, PolicyRouter};
use wardnetd_services::tunnel::{CreateTunnelParams, TunnelConfig, TunnelInterface};

use crate::backends::noop_device::{NoopHostnameResolver, NoopPacketCapture};
use crate::backends::noop_dhcp::NoopDhcpServer;
use crate::backends::noop_dns::NoopDnsServer;
use crate::backends::noop_routing::{NoopFirewallManager, NoopPolicyRouter};
use crate::backends::noop_tunnel::NoopTunnelInterface;

#[tokio::test]
async fn noop_tunnel_interface_all_methods_ok() {
    let t = NoopTunnelInterface;
    t.create(CreateTunnelParams {
        interface_name: "wg_mock0".into(),
        config: TunnelConfig::WireGuard {
            address: vec!["10.0.0.2/32".parse().unwrap()],
            private_key: [0u8; 32],
            listen_port: None,
            peer_public_key: [1u8; 32],
            peer_endpoint: Some("1.2.3.4:51820".parse().unwrap()),
            peer_allowed_ips: vec!["0.0.0.0/0".parse().unwrap()],
            peer_preshared_key: None,
            persistent_keepalive: None,
        },
    })
    .await
    .unwrap();
    t.bring_up("wg_mock0").await.unwrap();
    t.get_stats("wg_mock0").await.unwrap().unwrap();
    t.tear_down("wg_mock0").await.unwrap();
    t.remove("wg_mock0").await.unwrap();
    assert!(t.list().await.unwrap().is_empty());
}

#[tokio::test]
async fn noop_firewall_manager_all_methods_ok() {
    let f = NoopFirewallManager;
    f.init_wardnet_table().await.unwrap();
    f.flush_wardnet_table().await.unwrap();
    f.add_masquerade("wg_mock0").await.unwrap();
    f.remove_masquerade("wg_mock0").await.unwrap();
    f.cleanup_legacy_dns_redirects().await.unwrap();
    f.add_tcp_reset_reject("10.0.0.2").await.unwrap();
    f.remove_tcp_reset_reject("10.0.0.2").await.unwrap();
    f.check_tools_available().await.unwrap();
    f.destroy_wardnet_table().await.unwrap();
}

#[tokio::test]
async fn noop_policy_router_all_methods_ok() {
    let p = NoopPolicyRouter;
    p.enable_ip_forwarding().await.unwrap();
    p.add_route_table("wg_mock0", 100).await.unwrap();
    assert!(p.has_route_table(100).await.unwrap());
    p.add_ip_rule("10.0.0.2", 100).await.unwrap();
    assert!(p.list_wardnet_rules().await.unwrap().is_empty());
    p.remove_ip_rule("10.0.0.2", 100).await.unwrap();
    p.remove_route_table(100).await.unwrap();
    p.flush_conntrack("10.0.0.2").await.unwrap();
    p.flush_route_cache().await.unwrap();
    p.check_tools_available().await.unwrap();
}

#[tokio::test]
async fn noop_packet_capture_cancellation_returns_ok() {
    let cap = NoopPacketCapture;
    let cancel = CancellationToken::new();
    let (tx, _rx) = tokio::sync::mpsc::channel(8);

    let cancel_child = cancel.clone();
    let handle = tokio::spawn(async move { cap.capture_loop("mock0", tx, cancel_child).await });

    // Cancel immediately; capture_loop should return Ok.
    cancel.cancel();
    let result = handle.await.unwrap();
    result.unwrap();

    let cap = NoopPacketCapture;
    cap.arp_scan("mock0").await.unwrap();
}

#[tokio::test]
async fn noop_hostname_resolver_returns_none() {
    let r = NoopHostnameResolver;
    assert!(r.resolve("10.0.0.2").await.is_none());
}

#[tokio::test]
async fn noop_dhcp_server_toggles_running_state() {
    let s = NoopDhcpServer::default();
    assert!(!s.is_running());
    s.start().await.unwrap();
    assert!(s.is_running());
    s.stop().await.unwrap();
    assert!(!s.is_running());
}

#[tokio::test]
async fn noop_dns_server_toggles_and_returns_zero_metrics() {
    let s = NoopDnsServer::default();
    assert!(!s.is_running());
    s.start().await.unwrap();
    assert!(s.is_running());
    assert_eq!(s.cache_size().await, 0);
    assert!((s.cache_hit_rate().await - 0.0).abs() < f64::EPSILON);
    assert_eq!(s.flush_cache().await, 0);
    s.update_config(wardnet_common::dns::DnsConfig::default())
        .await;
    s.stop().await.unwrap();
    assert!(!s.is_running());
}

// ── synthetic exit-probe / latency values ───────────────────────────────

use crate::backends::noop_exit_probe::synthetic_ip;
use crate::backends::noop_latency_prober::synthetic_rtt;

#[test]
fn synthetic_ip_is_in_test_net_2_range() {
    for _ in 0..32 {
        let id = uuid::Uuid::new_v4();
        let ip = synthetic_ip(&id);
        assert!(ip.starts_with("198.51.100."));
        let last: u32 = ip.rsplit('.').next().unwrap().parse().unwrap();
        assert!((1..=254).contains(&last));
    }
}

#[test]
fn synthetic_ip_is_deterministic_per_id() {
    let id = uuid::Uuid::new_v4();
    assert_eq!(synthetic_ip(&id), synthetic_ip(&id));
}
#[test]
fn synthetic_rtt_is_in_expected_range() {
    for name in ["wg_ward0", "wg_ward1", "wg_ward2", "ifaceX"] {
        let rtt = synthetic_rtt(name);
        assert!((25..=80).contains(&rtt), "got {rtt} for {name}");
    }
}

#[test]
fn synthetic_rtt_is_deterministic() {
    assert_eq!(synthetic_rtt("wg_ward0"), synthetic_rtt("wg_ward0"));
}

// ── noop web push logging ───────────────────────────────────────────────

use wardnetd_services::push::sender::{PushTarget, VapidKey, WebPushSender};

use crate::backends::noop_web_push::NoopWebPushSender;

#[tokio::test]
async fn noop_web_push_log_names_the_endpoint_and_payload() {
    // The mock's delivery line stands in for real push logs during dev runs,
    // so it must name the endpoint and payload it "sent", not just carry them
    // as structured fields.
    let capture = wardnet_test_support::capture_logs(tracing::Level::INFO);

    let vapid = VapidKey::generate();
    let target = PushTarget {
        endpoint: "https://push.example.com/mock",
        p256dh: "p256dh",
        auth: "auth",
    };
    NoopWebPushSender
        .send(&vapid, target, b"hello-mock".to_vec())
        .await;

    let logs = capture.contents();
    assert!(
        logs.contains("endpoint=https://push.example.com/mock"),
        "mock log must interpolate the endpoint, got: {logs}"
    );
    assert!(
        logs.contains("payload=hello-mock"),
        "mock log must interpolate the payload, got: {logs}"
    );
}
