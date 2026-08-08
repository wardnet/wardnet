//! Recording doubles for the three backends the teardown path touches.
//!
//! Each one appends a description of every call it receives to a shared log,
//! so tests can assert both *what* was called and *in what order* — the same
//! pattern the routing service tests use for `FirewallManager`.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use uuid::Uuid;
use wardnet_common::api::{
    CreateTunnelRequest, CreateTunnelResponse, DeleteTunnelResponse, ListTunnelsResponse,
    TunnelDevicesResponse, TunnelTestResult,
};
use wardnet_common::jobs::JobDispatchedResponse;
use wardnet_common::speed_test::TunnelSpeedTestHistoryResponse;
use wardnet_common::tunnel::{Tunnel, TunnelStatus};
use wardnetd_services::error::AppError;
use wardnetd_services::inbound_wg::InboundWgInterface;
use wardnetd_services::inbound_wg::interface::{
    InboundWgPeerConfig, InboundWgPeerStats, InboundWgServerConfig,
};
use wardnetd_services::routing::FirewallManager;
use wardnetd_services::routing::firewall::{ZoneIsolationRules, ZoneRules};
use wardnetd_services::tunnel::TunnelInterface;
use wardnetd_services::tunnel::TunnelService;
use wardnetd_services::tunnel::interface::{CreateTunnelParams, TunnelStats};

/// Shared, ordered record of every backend call made during a test.
pub type CallLog = Arc<Mutex<Vec<String>>>;

/// Create an empty call log.
pub fn call_log() -> CallLog {
    Arc::new(Mutex::new(Vec::new()))
}

/// Read the recorded calls back out.
pub fn recorded(log: &CallLog) -> Vec<String> {
    log.lock().expect("call log poisoned").clone()
}

fn record(log: &CallLog, call: impl Into<String>) {
    log.lock().expect("call log poisoned").push(call.into());
}

/// `FirewallManager` double. Only `destroy_wardnet_table` is exercised by the
/// teardown path; the rest satisfy the trait and record nothing.
pub struct RecordingFirewall {
    log: CallLog,
    /// When true, `destroy_wardnet_table` returns an error so tests can prove
    /// teardown continues past a firewall failure.
    fail_destroy: bool,
}

impl RecordingFirewall {
    pub fn new(log: CallLog) -> Self {
        Self {
            log,
            fail_destroy: false,
        }
    }

    pub fn failing(log: CallLog) -> Self {
        Self {
            log,
            fail_destroy: true,
        }
    }
}

#[async_trait]
impl FirewallManager for RecordingFirewall {
    async fn init_wardnet_table(&self) -> anyhow::Result<()> {
        Ok(())
    }
    async fn ensure_isolation_jumps(&self) -> anyhow::Result<()> {
        Ok(())
    }
    async fn flush_wardnet_table(&self) -> anyhow::Result<()> {
        Ok(())
    }
    async fn add_masquerade(&self, _interface: &str) -> anyhow::Result<()> {
        Ok(())
    }
    async fn remove_masquerade(&self, _interface: &str) -> anyhow::Result<()> {
        Ok(())
    }
    async fn add_inbound_wg_accept(&self, _port: u16) -> anyhow::Result<()> {
        Ok(())
    }
    async fn remove_inbound_wg_accept(&self) -> anyhow::Result<()> {
        Ok(())
    }
    async fn cleanup_legacy_dns_redirects(&self) -> anyhow::Result<()> {
        Ok(())
    }
    async fn add_tcp_reset_reject(&self, _device_ip: &str) -> anyhow::Result<()> {
        Ok(())
    }
    async fn remove_tcp_reset_reject(&self, _device_ip: &str) -> anyhow::Result<()> {
        Ok(())
    }
    async fn apply_zone_rules(
        &self,
        _device_ip: &str,
        _rules: ZoneRules,
        _lan_interface: &str,
    ) -> anyhow::Result<()> {
        Ok(())
    }
    async fn remove_zone_rules(&self, _device_ip: &str) -> anyhow::Result<()> {
        Ok(())
    }
    async fn list_zone_rule_ips(&self) -> anyhow::Result<Vec<String>> {
        Ok(Vec::new())
    }
    async fn apply_zone_isolation(&self, _rules: ZoneIsolationRules) -> anyhow::Result<()> {
        Ok(())
    }
    async fn check_tools_available(&self) -> anyhow::Result<()> {
        Ok(())
    }

    async fn destroy_wardnet_table(&self) -> anyhow::Result<()> {
        record(&self.log, "destroy_wardnet_table");
        if self.fail_destroy {
            anyhow::bail!("simulated netlink failure");
        }
        Ok(())
    }
}

/// `TunnelInterface` double returning a fixed device list from `list()`.
pub struct RecordingTunnelInterface {
    log: CallLog,
    interfaces: Vec<String>,
    /// When true, `list` fails so tests can prove teardown skips the sweep
    /// rather than panicking.
    fail_list: bool,
}

impl RecordingTunnelInterface {
    pub fn new(log: CallLog, interfaces: &[&str]) -> Self {
        Self {
            log,
            interfaces: interfaces.iter().map(|s| (*s).to_owned()).collect(),
            fail_list: false,
        }
    }

    pub fn failing_list(log: CallLog) -> Self {
        Self {
            log,
            interfaces: Vec::new(),
            fail_list: true,
        }
    }
}

#[async_trait]
impl TunnelInterface for RecordingTunnelInterface {
    async fn create(&self, _params: CreateTunnelParams) -> anyhow::Result<()> {
        Ok(())
    }
    async fn bring_up(&self, _interface_name: &str) -> anyhow::Result<()> {
        Ok(())
    }
    async fn tear_down(&self, _interface_name: &str) -> anyhow::Result<()> {
        Ok(())
    }
    async fn get_stats(&self, _interface_name: &str) -> anyhow::Result<Option<TunnelStats>> {
        Ok(None)
    }

    async fn remove(&self, interface_name: &str) -> anyhow::Result<()> {
        record(&self.log, format!("remove:{interface_name}"));
        Ok(())
    }

    async fn list(&self) -> anyhow::Result<Vec<String>> {
        record(&self.log, "list");
        if self.fail_list {
            anyhow::bail!("simulated wireguard enumeration failure");
        }
        Ok(self.interfaces.clone())
    }
}

/// `TunnelService` double for the shutdown path.
///
/// Only `list_tunnels` and `tear_down_internal` are reachable from teardown;
/// the rest panic rather than returning plausible-looking defaults, so a future
/// change that starts calling one fails loudly instead of silently testing
/// nothing.
pub struct RecordingTunnelService {
    log: CallLog,
    tunnels: Vec<Tunnel>,
    /// Interface names whose teardown should fail.
    failing: Vec<String>,
    fail_list: bool,
}

impl RecordingTunnelService {
    pub fn new(log: CallLog, tunnels: Vec<Tunnel>) -> Self {
        Self {
            log,
            tunnels,
            failing: Vec::new(),
            fail_list: false,
        }
    }

    pub fn failing_teardown(log: CallLog, tunnels: Vec<Tunnel>, failing: &[&str]) -> Self {
        Self {
            log,
            tunnels,
            failing: failing.iter().map(|s| (*s).to_owned()).collect(),
            fail_list: false,
        }
    }

    pub fn failing_list(log: CallLog) -> Self {
        Self {
            log,
            tunnels: Vec::new(),
            failing: Vec::new(),
            fail_list: true,
        }
    }
}

/// Build a tunnel row with just the fields teardown reads.
pub fn tunnel(interface_name: &str, status: TunnelStatus) -> Tunnel {
    Tunnel {
        id: Uuid::new_v4(),
        label: interface_name.to_owned(),
        country_code: "NL".to_owned(),
        provider: None,
        interface_name: interface_name.to_owned(),
        endpoint: "198.51.100.1:51820".to_owned(),
        status,
        last_handshake: None,
        bytes_tx: 0,
        bytes_rx: 0,
        created_at: chrono::Utc::now(),
        override_default_dns: false,
        server_selector: None,
        resolved_server_name: None,
        endpoint_resolved_at: None,
    }
}

#[async_trait]
impl TunnelService for RecordingTunnelService {
    async fn list_tunnels(&self) -> Result<ListTunnelsResponse, AppError> {
        record(&self.log, "list_tunnels");
        if self.fail_list {
            return Err(AppError::Internal(anyhow::anyhow!(
                "simulated tunnel listing failure"
            )));
        }
        Ok(ListTunnelsResponse {
            tunnels: self.tunnels.clone(),
        })
    }

    async fn tear_down_internal(&self, id: Uuid, reason: &str) -> Result<(), AppError> {
        let name = self
            .tunnels
            .iter()
            .find(|t| t.id == id)
            .map_or_else(|| id.to_string(), |t| t.interface_name.clone());
        record(&self.log, format!("tear_down_internal:{name}:{reason}"));
        if self.failing.contains(&name) {
            return Err(AppError::Internal(anyhow::anyhow!(
                "simulated teardown failure"
            )));
        }
        Ok(())
    }

    async fn import_tunnel(
        &self,
        _req: CreateTunnelRequest,
    ) -> Result<CreateTunnelResponse, AppError> {
        unimplemented!("not reachable from shutdown teardown")
    }
    async fn get_tunnel(&self, _id: Uuid) -> Result<Tunnel, AppError> {
        unimplemented!("not reachable from shutdown teardown")
    }
    async fn test_tunnel(&self, _id: Uuid) -> Result<TunnelTestResult, AppError> {
        unimplemented!("not reachable from shutdown teardown")
    }
    async fn list_tunnel_devices(&self, _id: Uuid) -> Result<TunnelDevicesResponse, AppError> {
        unimplemented!("not reachable from shutdown teardown")
    }
    async fn set_dns_override(&self, _id: Uuid, _value: bool) -> Result<Tunnel, AppError> {
        unimplemented!("not reachable from shutdown teardown")
    }
    async fn rebuild(&self, _id: Uuid) -> Result<(), AppError> {
        unimplemented!("not reachable from shutdown teardown")
    }
    async fn bring_up(&self, _id: Uuid) -> Result<(), AppError> {
        unimplemented!("not reachable from shutdown teardown")
    }
    async fn tear_down(&self, _id: Uuid, _reason: &str) -> Result<(), AppError> {
        unimplemented!("not reachable from shutdown teardown")
    }
    async fn delete_tunnel(&self, _id: Uuid) -> Result<DeleteTunnelResponse, AppError> {
        unimplemented!("not reachable from shutdown teardown")
    }
    async fn bring_up_internal(&self, _id: Uuid) -> Result<(), AppError> {
        unimplemented!("not reachable from shutdown teardown")
    }
    async fn restore_tunnels(&self) -> Result<(), AppError> {
        unimplemented!("not reachable from shutdown teardown")
    }
    async fn collect_stats(&self) -> Result<(), AppError> {
        unimplemented!("not reachable from shutdown teardown")
    }
    async fn run_health_check(&self) -> Result<(), AppError> {
        unimplemented!("not reachable from shutdown teardown")
    }
    async fn probe_latencies(&self) -> Result<(), AppError> {
        unimplemented!("not reachable from shutdown teardown")
    }
    async fn start_speed_test(
        self: Arc<Self>,
        _id: Uuid,
    ) -> Result<JobDispatchedResponse, AppError> {
        unimplemented!("not reachable from shutdown teardown")
    }
    async fn list_speed_tests(
        &self,
        _id: Uuid,
    ) -> Result<TunnelSpeedTestHistoryResponse, AppError> {
        unimplemented!("not reachable from shutdown teardown")
    }
}

/// `InboundWgInterface` double recording only the server teardown.
pub struct RecordingInboundWg {
    log: CallLog,
}

impl RecordingInboundWg {
    pub fn new(log: CallLog) -> Self {
        Self { log }
    }
}

#[async_trait]
impl InboundWgInterface for RecordingInboundWg {
    async fn ensure_server(&self, _config: InboundWgServerConfig) -> anyhow::Result<()> {
        Ok(())
    }
    async fn add_peer(
        &self,
        _interface_name: &str,
        _peer: InboundWgPeerConfig,
    ) -> anyhow::Result<()> {
        Ok(())
    }
    async fn remove_peer(
        &self,
        _interface_name: &str,
        _public_key: [u8; 32],
    ) -> anyhow::Result<()> {
        Ok(())
    }
    async fn peer_stats(&self, _interface_name: &str) -> anyhow::Result<Vec<InboundWgPeerStats>> {
        Ok(Vec::new())
    }

    async fn tear_down_server(&self, interface_name: &str) -> anyhow::Result<()> {
        record(&self.log, format!("tear_down_server:{interface_name}"));
        Ok(())
    }
}
