use std::collections::{HashSet, VecDeque};
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;
use dhcproto::v4::{DhcpOption, Message, MessageType, Opcode, OptionCode};
use dhcproto::{Decodable, Decoder, Encodable, Encoder};
use tokio::sync::Mutex;
use uuid::Uuid;
use wardnet_common::api::{
    CreateDhcpReservationRequest, CreateDhcpReservationResponse, DeleteDhcpReservationResponse,
    DhcpConfigResponse, DhcpStatusResponse, ListDhcpLeasesResponse, ListDhcpReservationsResponse,
    PreviewDhcpConfigRequest, PreviewDhcpConfigResponse, RevokeDhcpLeaseResponse,
    ToggleDhcpRequest, UpdateDhcpConfigRequest,
};
use wardnet_common::device::DeviceSignalKind;
use wardnet_common::dhcp::{DhcpConfig, DhcpLease, DhcpLeaseStatus, DhcpScope};

use crate::dhcp::server::{self, UdpDhcpServer};
use wardnetd_services::dhcp::DhcpService;
use wardnetd_services::dhcp::server::{DhcpServer, DhcpSocket};
use wardnetd_services::error::AppError;

// ---------------------------------------------------------------------------
// MockDhcpSocket
// ---------------------------------------------------------------------------

/// Mock socket that stores sent packets and returns pre-loaded received packets.
///
/// When the incoming queue is empty, `recv_from` blocks forever (the test
/// cancels the token to break the loop).
struct MockDhcpSocket {
    /// Packets to return from `recv_from`, popped in order.
    incoming: Mutex<VecDeque<(Vec<u8>, SocketAddr)>>,
    /// Packets that were sent via `send_to`.
    outgoing: Mutex<Vec<(Vec<u8>, SocketAddr)>>,
    /// Wakes `recv_from` when a packet is available.
    notify: tokio::sync::Notify,
}

impl MockDhcpSocket {
    fn new() -> Self {
        Self {
            incoming: Mutex::new(VecDeque::new()),
            outgoing: Mutex::new(Vec::new()),
            notify: tokio::sync::Notify::new(),
        }
    }

    /// Push a raw packet into the incoming queue.
    async fn push_packet(&self, data: Vec<u8>, src: SocketAddr) {
        self.incoming.lock().await.push_back((data, src));
        self.notify.notify_one();
    }

    /// Push an encoded DHCP message into the incoming queue.
    async fn push_message(&self, msg: &Message, src: SocketAddr) {
        self.push_packet(encode_message(msg), src).await;
    }

    /// Return all packets sent via `send_to`.
    async fn sent_packets(&self) -> Vec<(Vec<u8>, SocketAddr)> {
        self.outgoing.lock().await.clone()
    }

    /// Decode outgoing packets as DHCP messages.
    async fn sent_messages(&self) -> Vec<(Message, SocketAddr)> {
        let packets = self.outgoing.lock().await.clone();
        packets
            .into_iter()
            .filter_map(|(data, addr)| {
                Message::decode(&mut Decoder::new(&data))
                    .ok()
                    .map(|m| (m, addr))
            })
            .collect()
    }
}

#[async_trait]
impl DhcpSocket for MockDhcpSocket {
    async fn recv_from(&self, buf: &mut [u8]) -> std::io::Result<(usize, SocketAddr)> {
        loop {
            {
                let mut queue = self.incoming.lock().await;
                if let Some((data, addr)) = queue.pop_front() {
                    let len = data.len().min(buf.len());
                    buf[..len].copy_from_slice(&data[..len]);
                    return Ok((len, addr));
                }
            }
            // Block until a packet is pushed or the test cancels the token.
            self.notify.notified().await;
        }
    }

    async fn send_to(&self, buf: &[u8], target: SocketAddr) -> std::io::Result<usize> {
        let data = buf.to_vec();
        let len = data.len();
        self.outgoing.lock().await.push((data, target));
        Ok(len)
    }
}

// ---------------------------------------------------------------------------
// Mock DhcpService for server tests
// ---------------------------------------------------------------------------

/// Tracks calls to `assign_lease` and `renew_lease` for test assertions.
struct MockDhcpService {
    /// The lease to return from `assign_lease` / `renew_lease`.
    lease: DhcpLease,
    /// Records `(method_name, mac)` calls.
    calls: Mutex<Vec<(String, String)>>,
    /// When set, `active_lease` returns an error instead of a lookup result.
    active_lease_errors: bool,
    /// When set, `release_lease` records the call then returns an error.
    release_errors: bool,
}

impl MockDhcpService {
    fn new(lease: DhcpLease) -> Self {
        Self {
            lease,
            calls: Mutex::new(Vec::new()),
            active_lease_errors: false,
            release_errors: false,
        }
    }

    /// Make `active_lease` fail, to exercise the release runtime's lookup-error path.
    fn failing_active_lease(mut self) -> Self {
        self.active_lease_errors = true;
        self
    }

    /// Make `release_lease` fail after recording the call, to exercise the
    /// release runtime's release-error path.
    fn failing_release(mut self) -> Self {
        self.release_errors = true;
        self
    }

    async fn recorded_calls(&self) -> Vec<(String, String)> {
        self.calls.lock().await.clone()
    }
}

#[async_trait]
impl DhcpService for MockDhcpService {
    async fn get_config(&self) -> Result<DhcpConfigResponse, AppError> {
        unimplemented!()
    }
    async fn update_config(
        &self,
        _r: UpdateDhcpConfigRequest,
    ) -> Result<DhcpConfigResponse, AppError> {
        unimplemented!()
    }
    async fn preview_config(
        &self,
        _req: PreviewDhcpConfigRequest,
    ) -> Result<PreviewDhcpConfigResponse, AppError> {
        Ok(PreviewDhcpConfigResponse {
            affected: Vec::new(),
        })
    }
    async fn toggle(&self, _r: ToggleDhcpRequest) -> Result<DhcpConfigResponse, AppError> {
        unimplemented!()
    }
    async fn list_leases(&self) -> Result<ListDhcpLeasesResponse, AppError> {
        unimplemented!()
    }
    async fn revoke_lease(&self, _id: Uuid) -> Result<RevokeDhcpLeaseResponse, AppError> {
        unimplemented!()
    }
    async fn list_reservations(&self) -> Result<ListDhcpReservationsResponse, AppError> {
        unimplemented!()
    }
    async fn create_reservation(
        &self,
        _r: CreateDhcpReservationRequest,
    ) -> Result<CreateDhcpReservationResponse, AppError> {
        unimplemented!()
    }
    async fn delete_reservation(
        &self,
        _id: Uuid,
    ) -> Result<DeleteDhcpReservationResponse, AppError> {
        unimplemented!()
    }
    async fn status(&self) -> Result<DhcpStatusResponse, AppError> {
        unimplemented!()
    }

    async fn assign_lease(
        &self,
        mac: &str,
        _hostname: Option<&str>,
    ) -> Result<DhcpLease, AppError> {
        self.calls
            .lock()
            .await
            .push(("assign_lease".to_owned(), mac.to_owned()));
        Ok(self.lease.clone())
    }

    async fn renew_lease(&self, mac: &str, hostname: Option<&str>) -> Result<DhcpLease, AppError> {
        let detail = hostname.map_or_else(String::new, |h| format!(":{h}"));
        self.calls
            .lock()
            .await
            .push(("renew_lease".to_owned(), format!("{mac}{detail}")));
        Ok(self.lease.clone())
    }

    async fn release_lease(&self, mac: &str) -> Result<(), AppError> {
        self.calls
            .lock()
            .await
            .push(("release_lease".to_owned(), mac.to_owned()));
        if self.release_errors {
            return Err(AppError::Internal(anyhow::anyhow!("mock release failure")));
        }
        Ok(())
    }

    /// Returns the configured lease when its MAC is queried, mirroring an
    /// active lease recorded for that device. Intentionally does NOT record a
    /// call so ownership lookups don't perturb `release_lease` assertions.
    async fn active_lease(&self, mac: &str) -> Result<Option<DhcpLease>, AppError> {
        if self.active_lease_errors {
            return Err(AppError::Internal(anyhow::anyhow!(
                "mock active_lease failure"
            )));
        }
        if mac == self.lease.mac_address {
            Ok(Some(self.lease.clone()))
        } else {
            Ok(None)
        }
    }

    async fn cleanup_expired(&self) -> Result<u64, AppError> {
        Ok(0)
    }

    async fn get_dhcp_config(&self) -> Result<DhcpConfig, AppError> {
        Ok(test_config())
    }

    async fn scope_for_mac(&self, _mac: &str) -> Result<DhcpScope, AppError> {
        Ok(test_scope())
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn test_config() -> DhcpConfig {
    DhcpConfig {
        enabled: true,
        gateway_ip: Ipv4Addr::new(192, 168, 1, 1),
        pool_start: Ipv4Addr::new(192, 168, 1, 100),
        pool_end: Ipv4Addr::new(192, 168, 1, 200),
        subnet_mask: Ipv4Addr::new(255, 255, 255, 0),
        upstream_dns: vec![Ipv4Addr::new(1, 1, 1, 1)],
        lease_duration_secs: 86400,
        router_ip: Some(Ipv4Addr::new(192, 168, 1, 1)),
    }
}

/// The base-pool scope mirroring [`test_config`], used to render responses in
/// server tests (#737 moved rendering from `DhcpConfig` to `DhcpScope`).
fn test_scope() -> DhcpScope {
    DhcpScope {
        gateway_ip: Ipv4Addr::new(192, 168, 1, 1),
        pool_start: Ipv4Addr::new(192, 168, 1, 100),
        pool_end: Ipv4Addr::new(192, 168, 1, 200),
        subnet_mask: Ipv4Addr::new(255, 255, 255, 0),
        dns: vec![Ipv4Addr::new(1, 1, 1, 1)],
        lease_duration_secs: 86400,
        router_ip: Some(Ipv4Addr::new(192, 168, 1, 1)),
        member_isolation: false,
        subnet_prefix: None,
    }
}

fn test_lease() -> DhcpLease {
    DhcpLease {
        id: Uuid::new_v4(),
        mac_address: "aa:bb:cc:dd:ee:ff".to_owned(),
        ip_address: Ipv4Addr::new(192, 168, 1, 100),
        hostname: Some("test-host".to_owned()),
        lease_start: chrono::Utc::now(),
        lease_end: chrono::Utc::now() + chrono::Duration::seconds(86400),
        status: DhcpLeaseStatus::Active,
        device_id: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    }
}

/// Build a DHCPDISCOVER message with the given MAC address.
fn build_discover(mac: [u8; 6]) -> Message {
    let mut msg = Message::default();
    msg.set_opcode(Opcode::BootRequest).set_chaddr(&mac);
    msg.opts_mut()
        .insert(DhcpOption::MessageType(MessageType::Discover));
    msg.opts_mut()
        .insert(DhcpOption::Hostname("test-host".to_owned()));
    msg
}

/// Build a DHCPREQUEST message with the given MAC address.
fn build_request(mac: [u8; 6]) -> Message {
    let mut msg = Message::default();
    msg.set_opcode(Opcode::BootRequest).set_chaddr(&mac);
    msg.opts_mut()
        .insert(DhcpOption::MessageType(MessageType::Request));
    msg
}

/// Build a DHCPRELEASE message with the given MAC address.
fn build_release(mac: [u8; 6]) -> Message {
    let mut msg = Message::default();
    msg.set_opcode(Opcode::BootRequest).set_chaddr(&mac);
    msg.opts_mut()
        .insert(DhcpOption::MessageType(MessageType::Release));
    msg
}

/// Like [`build_release`] but with `ciaddr` set. An RFC 2131 conformant client
/// fills `ciaddr` with its own leased address, but the field is decoded
/// straight from the packet body, so a forging attacker can set it to any
/// value with no spoofing. Used to prove the release authorization ignores it.
fn build_release_with_ciaddr(mac: [u8; 6], ciaddr: Ipv4Addr) -> Message {
    let mut msg = build_release(mac);
    msg.set_ciaddr(ciaddr);
    msg
}

/// A fake client address for incoming packets.
fn client_addr() -> SocketAddr {
    "192.168.1.50:68".parse().unwrap()
}

/// Encode a DHCP message to wire bytes.
fn encode_message(msg: &Message) -> Vec<u8> {
    let mut buf = Vec::with_capacity(512);
    let mut encoder = Encoder::new(&mut buf);
    msg.encode(&mut encoder).unwrap();
    buf
}

/// Poll the mock socket until at least `count` responses have been sent.
/// Returns as soon as the server loop has actually produced the output,
/// with a hard 2-second deadline instead of a fixed lower bound of sleep.
async fn wait_for_sent(socket: &MockDhcpSocket, count: usize) {
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        while socket.outgoing.lock().await.len() < count {
            tokio::time::sleep(std::time::Duration::from_millis(2)).await;
        }
    })
    .await
    .expect("timed out waiting for the DHCP server to send responses");
}

/// Run `server_loop` with the given socket and service, returning the socket
/// after the loop finishes (via cancellation token).
async fn run_server_loop_until_idle(
    socket: Arc<MockDhcpSocket>,
    service: Arc<dyn DhcpService>,
) -> Arc<MockDhcpSocket> {
    let running = Arc::new(AtomicBool::new(true));
    let cancel = tokio_util::sync::CancellationToken::new();

    let cancel_clone = cancel.clone();
    let socket_dyn: Arc<dyn DhcpSocket> = Arc::clone(&socket) as Arc<dyn DhcpSocket>;
    let running_clone = Arc::clone(&running);
    let own_macs = Arc::new(HashSet::new());

    let handle = tokio::spawn(async move {
        server::server_loop(
            socket_dyn,
            service,
            None,
            running_clone,
            cancel_clone,
            own_macs,
        )
        .await;
    });

    // Give the loop time to process all queued packets.
    // We yield multiple times to let the async tasks make progress.
    for _ in 0..20 {
        tokio::task::yield_now().await;
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }

    cancel.cancel();
    let _ = handle.await;
    socket
}

// ---------------------------------------------------------------------------
// Pure function tests
// ---------------------------------------------------------------------------

#[test]
fn format_mac_formats_correctly() {
    let mac = [0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff];
    assert_eq!(crate::dhcp::server::format_mac(&mac), "aa:bb:cc:dd:ee:ff");
}

#[test]
fn format_mac_handles_short_input() {
    let mac = [0x01, 0x02];
    assert_eq!(crate::dhcp::server::format_mac(&mac), "01:02");
}

#[test]
fn format_mac_handles_padded_chaddr() {
    // DHCP chaddr is 16 bytes, only first 6 are MAC.
    let mut chaddr = [0u8; 16];
    chaddr[..6].copy_from_slice(&[0xde, 0xad, 0xbe, 0xef, 0x00, 0x01]);
    assert_eq!(
        crate::dhcp::server::format_mac(&chaddr),
        "de:ad:be:ef:00:01"
    );
}

#[test]
fn format_mac_empty_slice() {
    assert_eq!(server::format_mac(&[]), "");
}

#[test]
fn extract_hostname_returns_hostname_option() {
    let msg = build_discover([0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff]);
    let hostname = crate::dhcp::server::extract_hostname(&msg);
    assert_eq!(hostname, Some("test-host".to_owned()));
}

#[test]
fn extract_hostname_returns_none_when_absent() {
    let msg = build_request([0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff]);
    let hostname = crate::dhcp::server::extract_hostname(&msg);
    assert_eq!(hostname, None);
}

#[test]
fn extract_param_list_preserves_client_ordering() {
    // Option 55's identifying property is the ORDER the client asks in, not the
    // set of codes — that ordering is what makes it a device-class fingerprint
    // (issue #1099). Sorting or de-duplicating here would destroy the signal.
    let mut msg = build_request([0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff]);
    msg.opts_mut().insert(DhcpOption::ParameterRequestList(vec![
        OptionCode::SubnetMask,
        OptionCode::Router,
        OptionCode::DomainNameServer,
    ]));

    assert_eq!(
        crate::dhcp::server::extract_param_list(&msg),
        Some("1,3,6".to_owned())
    );
}

#[test]
fn extract_param_list_returns_none_when_absent_or_empty() {
    let msg = build_request([0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff]);
    assert_eq!(crate::dhcp::server::extract_param_list(&msg), None);

    let mut empty = build_request([0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff]);
    empty
        .opts_mut()
        .insert(DhcpOption::ParameterRequestList(vec![]));
    assert_eq!(crate::dhcp::server::extract_param_list(&empty), None);
}

#[test]
fn extract_vendor_class_reads_option_60() {
    let mut msg = build_request([0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff]);
    msg.opts_mut()
        .insert(DhcpOption::ClassIdentifier(b"ubnt-unifi-ap".to_vec()));

    assert_eq!(
        crate::dhcp::server::extract_vendor_class(&msg),
        Some("ubnt-unifi-ap".to_owned())
    );
}

#[test]
fn extract_vendor_class_survives_non_utf8_bytes() {
    // Option 60 is a byte string with no encoding guarantee. A mangled
    // character still gives the vendor catalog a usable substring to match,
    // whereas dropping the option loses the only vendor tell some devices emit.
    let mut msg = build_request([0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff]);
    msg.opts_mut()
        .insert(DhcpOption::ClassIdentifier(vec![b'u', b'b', 0xff, b'n']));

    let class = crate::dhcp::server::extract_vendor_class(&msg);
    assert!(
        class.is_some_and(|c| c.starts_with("ub")),
        "non-UTF-8 vendor class should degrade, not vanish"
    );
}

#[test]
fn extract_vendor_class_returns_none_when_absent_or_blank() {
    let msg = build_request([0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff]);
    assert_eq!(crate::dhcp::server::extract_vendor_class(&msg), None);

    let mut blank = build_request([0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff]);
    blank
        .opts_mut()
        .insert(DhcpOption::ClassIdentifier(b"   ".to_vec()));
    assert_eq!(crate::dhcp::server::extract_vendor_class(&blank), None);
}

#[test]
fn build_response_creates_valid_offer() {
    let request = build_discover([0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff]);
    let lease = test_lease();
    let scope = test_scope();

    let response =
        crate::dhcp::server::build_response(&request, MessageType::Offer, &lease, &scope);

    assert_eq!(response.opcode(), Opcode::BootReply);
    assert_eq!(response.xid(), request.xid());
    assert_eq!(response.yiaddr(), lease.ip_address);
    assert_eq!(response.opts().msg_type(), Some(MessageType::Offer));

    // Verify the response can be encoded and decoded (round-trip).
    let mut buf = Vec::new();
    let mut encoder = Encoder::new(&mut buf);
    response.encode(&mut encoder).unwrap();
    assert!(!buf.is_empty());

    let decoded = Message::decode(&mut Decoder::new(&buf)).unwrap();
    assert_eq!(decoded.yiaddr(), lease.ip_address);
}

#[test]
fn build_response_creates_valid_ack() {
    let request = build_request([0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff]);
    let lease = test_lease();
    let scope = test_scope();

    let response = crate::dhcp::server::build_response(&request, MessageType::Ack, &lease, &scope);

    assert_eq!(response.opcode(), Opcode::BootReply);
    assert_eq!(response.opts().msg_type(), Some(MessageType::Ack));
}

#[test]
fn build_response_includes_router_and_dns() {
    let request = build_discover([0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff]);
    let lease = test_lease();
    let scope = test_scope();

    let response =
        crate::dhcp::server::build_response(&request, MessageType::Offer, &lease, &scope);

    // Encode and decode to check options survive the round-trip.
    let mut buf = Vec::new();
    let mut encoder = Encoder::new(&mut buf);
    response.encode(&mut encoder).unwrap();

    let decoded = Message::decode(&mut Decoder::new(&buf)).unwrap();

    // Check that subnet mask, router, and DNS options are present.
    let mut has_subnet = false;
    let mut has_router = false;
    let mut has_dns = false;
    let mut has_lease_time = false;
    let mut has_server_id = false;

    for (_code, opt) in decoded.opts().iter() {
        match opt {
            DhcpOption::SubnetMask(mask) => {
                assert_eq!(*mask, Ipv4Addr::new(255, 255, 255, 0));
                has_subnet = true;
            }
            DhcpOption::Router(routers) => {
                assert!(routers.contains(&Ipv4Addr::new(192, 168, 1, 1)));
                has_router = true;
            }
            DhcpOption::DomainNameServer(servers) => {
                assert!(!servers.is_empty());
                has_dns = true;
            }
            DhcpOption::AddressLeaseTime(t) => {
                assert_eq!(*t, 86400);
                has_lease_time = true;
            }
            DhcpOption::ServerIdentifier(ip) => {
                assert_eq!(*ip, Ipv4Addr::new(192, 168, 1, 1));
                has_server_id = true;
            }
            _ => {}
        }
    }

    assert!(has_subnet, "SubnetMask option missing");
    assert!(has_router, "Router option missing");
    assert!(has_dns, "DomainNameServer option missing");
    assert!(has_lease_time, "AddressLeaseTime option missing");
    assert!(has_server_id, "ServerIdentifier option missing");
}

/// Decode option 6 from an encoded response — helper for the wire-level
/// DNS-advertisement guards below.
fn decoded_dns_servers(response: &Message) -> Vec<Ipv4Addr> {
    let mut buf = Vec::new();
    let mut encoder = Encoder::new(&mut buf);
    response.encode(&mut encoder).unwrap();
    let decoded = Message::decode(&mut Decoder::new(&buf)).unwrap();
    for (_code, opt) in decoded.opts().iter() {
        if let DhcpOption::DomainNameServer(servers) = opt {
            return servers.clone();
        }
    }
    panic!("DomainNameServer option missing");
}

/// Wire-level guard for the scope→option-6 contract: whatever DNS list the
/// resolved scope carries is exactly what clients receive. The
/// advertise-wardnet-dns bug shipped because nothing asserted the rendered
/// option, only the intermediate scope value.
#[test]
fn option6_carries_exactly_the_scope_dns_list() {
    let request = build_discover([0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff]);
    let lease = test_lease();
    let mut scope = test_scope();
    scope.dns = vec![Ipv4Addr::new(192, 168, 1, 1)];

    let response = crate::dhcp::server::build_response(&request, MessageType::Ack, &lease, &scope);
    assert_eq!(
        decoded_dns_servers(&response),
        vec![Ipv4Addr::new(192, 168, 1, 1)],
        "clients must receive exactly the scope's DNS list in option 6"
    );
}

/// The empty-scope fallback advertises the Pi itself (the scope gateway) —
/// pinned at the wire level so a regression can't hide behind the
/// service-layer tests.
#[test]
fn option6_empty_scope_dns_falls_back_to_the_pi() {
    let request = build_discover([0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff]);
    let lease = test_lease();
    let mut scope = test_scope();
    scope.dns = vec![];

    let response = crate::dhcp::server::build_response(&request, MessageType::Ack, &lease, &scope);
    assert_eq!(
        decoded_dns_servers(&response),
        vec![scope.gateway_ip],
        "an empty scope DNS list must advertise the Pi, never nothing"
    );
}

#[test]
fn build_response_siaddr_is_wardnet_gateway_ip() {
    let request = build_discover([0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff]);
    let lease = test_lease();
    let mut scope = test_scope();
    scope.router_ip = None;

    let response =
        crate::dhcp::server::build_response(&request, MessageType::Offer, &lease, &scope);

    // siaddr is always wardnet's own IP for the scope (its gateway alias),
    // independent of the optional upstream router fallback.
    assert_eq!(response.siaddr(), scope.gateway_ip);
}

#[test]
fn build_response_copies_chaddr() {
    let mac_bytes = [0x11, 0x22, 0x33, 0x44, 0x55, 0x66];
    let request = build_discover(mac_bytes);
    let lease = test_lease();
    let scope = test_scope();

    let response = server::build_response(&request, MessageType::Offer, &lease, &scope);
    assert_eq!(&response.chaddr()[..6], &mac_bytes);
}

// ---------------------------------------------------------------------------
// handle_discover / handle_request tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn handle_discover_calls_assign_lease_and_returns_offer() {
    let lease = test_lease();
    let mock = Arc::new(MockDhcpService::new(lease.clone()));
    let service: Arc<dyn DhcpService> = Arc::clone(&mock) as Arc<dyn DhcpService>;
    let msg = build_discover([0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff]);

    let response = server::handle_discover(&service, &msg, "aa:bb:cc:dd:ee:ff")
        .await
        .unwrap();

    assert_eq!(response.opcode(), Opcode::BootReply);
    assert_eq!(response.yiaddr(), lease.ip_address);
    assert_eq!(response.opts().msg_type(), Some(MessageType::Offer));

    let calls = mock.recorded_calls().await;
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].0, "assign_lease");
}

#[tokio::test]
async fn handle_discover_extracts_hostname_from_message() {
    let lease = test_lease();
    let mock = Arc::new(MockDhcpService::new(lease));
    let service: Arc<dyn DhcpService> = Arc::clone(&mock) as Arc<dyn DhcpService>;
    let msg = build_discover([0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff]);

    let _response = server::handle_discover(&service, &msg, "aa:bb:cc:dd:ee:ff")
        .await
        .unwrap();

    let calls = mock.recorded_calls().await;
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].0, "assign_lease");
    assert_eq!(calls[0].1, "aa:bb:cc:dd:ee:ff");
}

#[tokio::test]
async fn handle_discover_preserves_xid() {
    let lease = test_lease();
    let service: Arc<dyn DhcpService> = Arc::new(MockDhcpService::new(lease));
    let mut msg = build_discover([0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff]);
    msg.set_xid(0x1234_5678);

    let response = server::handle_discover(&service, &msg, "aa:bb:cc:dd:ee:ff")
        .await
        .unwrap();

    assert_eq!(response.xid(), 0x1234_5678);
}

#[tokio::test]
async fn handle_request_calls_renew_lease_and_returns_ack() {
    let lease = test_lease();
    let mock = Arc::new(MockDhcpService::new(lease.clone()));
    let service: Arc<dyn DhcpService> = Arc::clone(&mock) as Arc<dyn DhcpService>;
    let msg = build_request([0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff]);

    let response = server::handle_request(&service, &msg, "aa:bb:cc:dd:ee:ff")
        .await
        .unwrap();

    assert_eq!(response.opcode(), Opcode::BootReply);
    assert_eq!(response.yiaddr(), lease.ip_address);
    assert_eq!(response.opts().msg_type(), Some(MessageType::Ack));

    let calls = mock.recorded_calls().await;
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].0, "renew_lease");
}

#[tokio::test]
async fn handle_request_naks_when_assigned_ip_differs_from_requested() {
    // Client renews from an out-of-range address (ciaddr) but the service
    // hands back a different, in-range lease — meaning the requested IP is no
    // longer valid. The server must NAK, not ACK a surprise IP (issue #227).
    let lease = test_lease(); // ip 192.168.1.100
    let mock = Arc::new(MockDhcpService::new(lease.clone()));
    let service: Arc<dyn DhcpService> = Arc::clone(&mock) as Arc<dyn DhcpService>;

    let mut msg = build_request([0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff]);
    msg.set_ciaddr(Ipv4Addr::new(192, 168, 1, 50)); // old, now out-of-range IP

    let response = server::handle_request(&service, &msg, "aa:bb:cc:dd:ee:ff")
        .await
        .unwrap();

    assert_eq!(response.opts().msg_type(), Some(MessageType::Nak));
    // A NAK carries no address assignment.
    assert_eq!(response.yiaddr(), Ipv4Addr::UNSPECIFIED);
}

#[tokio::test]
async fn handle_request_acks_when_requested_ip_matches_assigned() {
    // A normal renewal: the client asks to keep exactly the IP the service
    // returns, so the server ACKs it.
    let lease = test_lease(); // ip 192.168.1.100
    let mock = Arc::new(MockDhcpService::new(lease.clone()));
    let service: Arc<dyn DhcpService> = Arc::clone(&mock) as Arc<dyn DhcpService>;

    let mut msg = build_request([0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff]);
    msg.set_ciaddr(lease.ip_address);

    let response = server::handle_request(&service, &msg, "aa:bb:cc:dd:ee:ff")
        .await
        .unwrap();

    assert_eq!(response.opts().msg_type(), Some(MessageType::Ack));
    assert_eq!(response.yiaddr(), lease.ip_address);
}

#[tokio::test]
async fn handle_request_preserves_xid() {
    let lease = test_lease();
    let service: Arc<dyn DhcpService> = Arc::new(MockDhcpService::new(lease));
    let mut msg = build_request([0x11, 0x22, 0x33, 0x44, 0x55, 0x66]);
    msg.set_xid(0xdead_beef);

    let response = server::handle_request(&service, &msg, "11:22:33:44:55:66")
        .await
        .unwrap();

    assert_eq!(response.xid(), 0xdead_beef);
}

#[tokio::test]
async fn handle_discover_returns_error_when_service_fails() {
    /// Mock service that always returns an error on `assign_lease`.
    struct FailingService;

    #[async_trait]
    impl DhcpService for FailingService {
        async fn get_config(&self) -> Result<DhcpConfigResponse, AppError> {
            unimplemented!()
        }
        async fn update_config(
            &self,
            _r: UpdateDhcpConfigRequest,
        ) -> Result<DhcpConfigResponse, AppError> {
            unimplemented!()
        }
        async fn preview_config(
            &self,
            _req: PreviewDhcpConfigRequest,
        ) -> Result<PreviewDhcpConfigResponse, AppError> {
            Ok(PreviewDhcpConfigResponse {
                affected: Vec::new(),
            })
        }
        async fn toggle(&self, _r: ToggleDhcpRequest) -> Result<DhcpConfigResponse, AppError> {
            unimplemented!()
        }
        async fn list_leases(&self) -> Result<ListDhcpLeasesResponse, AppError> {
            unimplemented!()
        }
        async fn revoke_lease(&self, _id: Uuid) -> Result<RevokeDhcpLeaseResponse, AppError> {
            unimplemented!()
        }
        async fn list_reservations(&self) -> Result<ListDhcpReservationsResponse, AppError> {
            unimplemented!()
        }
        async fn create_reservation(
            &self,
            _r: CreateDhcpReservationRequest,
        ) -> Result<CreateDhcpReservationResponse, AppError> {
            unimplemented!()
        }
        async fn delete_reservation(
            &self,
            _id: Uuid,
        ) -> Result<DeleteDhcpReservationResponse, AppError> {
            unimplemented!()
        }
        async fn status(&self) -> Result<DhcpStatusResponse, AppError> {
            unimplemented!()
        }
        async fn assign_lease(
            &self,
            _mac: &str,
            _hostname: Option<&str>,
        ) -> Result<DhcpLease, AppError> {
            Err(AppError::Conflict("pool exhausted".to_owned()))
        }
        async fn renew_lease(
            &self,
            _mac: &str,
            _hostname: Option<&str>,
        ) -> Result<DhcpLease, AppError> {
            Err(AppError::NotFound("no active lease".to_owned()))
        }
        async fn release_lease(&self, _mac: &str) -> Result<(), AppError> {
            Ok(())
        }
        async fn cleanup_expired(&self) -> Result<u64, AppError> {
            Ok(0)
        }
        async fn get_dhcp_config(&self) -> Result<DhcpConfig, AppError> {
            Ok(test_config())
        }
        async fn scope_for_mac(&self, _mac: &str) -> Result<DhcpScope, AppError> {
            Ok(test_scope())
        }
    }

    let service: Arc<dyn DhcpService> = Arc::new(FailingService);
    let msg = build_discover([0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff]);

    let result = server::handle_discover(&service, &msg, "aa:bb:cc:dd:ee:ff").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn handle_request_returns_error_when_service_fails() {
    /// Mock service that always returns an error on `renew_lease`.
    struct FailingRenewService;

    #[async_trait]
    impl DhcpService for FailingRenewService {
        async fn get_config(&self) -> Result<DhcpConfigResponse, AppError> {
            unimplemented!()
        }
        async fn update_config(
            &self,
            _r: UpdateDhcpConfigRequest,
        ) -> Result<DhcpConfigResponse, AppError> {
            unimplemented!()
        }
        async fn preview_config(
            &self,
            _req: PreviewDhcpConfigRequest,
        ) -> Result<PreviewDhcpConfigResponse, AppError> {
            Ok(PreviewDhcpConfigResponse {
                affected: Vec::new(),
            })
        }
        async fn toggle(&self, _r: ToggleDhcpRequest) -> Result<DhcpConfigResponse, AppError> {
            unimplemented!()
        }
        async fn list_leases(&self) -> Result<ListDhcpLeasesResponse, AppError> {
            unimplemented!()
        }
        async fn revoke_lease(&self, _id: Uuid) -> Result<RevokeDhcpLeaseResponse, AppError> {
            unimplemented!()
        }
        async fn list_reservations(&self) -> Result<ListDhcpReservationsResponse, AppError> {
            unimplemented!()
        }
        async fn create_reservation(
            &self,
            _r: CreateDhcpReservationRequest,
        ) -> Result<CreateDhcpReservationResponse, AppError> {
            unimplemented!()
        }
        async fn delete_reservation(
            &self,
            _id: Uuid,
        ) -> Result<DeleteDhcpReservationResponse, AppError> {
            unimplemented!()
        }
        async fn status(&self) -> Result<DhcpStatusResponse, AppError> {
            unimplemented!()
        }
        async fn assign_lease(
            &self,
            _mac: &str,
            _hostname: Option<&str>,
        ) -> Result<DhcpLease, AppError> {
            unimplemented!()
        }
        async fn renew_lease(
            &self,
            _mac: &str,
            _hostname: Option<&str>,
        ) -> Result<DhcpLease, AppError> {
            Err(AppError::Internal(anyhow::anyhow!("database error")))
        }
        async fn release_lease(&self, _mac: &str) -> Result<(), AppError> {
            Ok(())
        }
        async fn cleanup_expired(&self) -> Result<u64, AppError> {
            Ok(0)
        }
        async fn get_dhcp_config(&self) -> Result<DhcpConfig, AppError> {
            Ok(test_config())
        }
        async fn scope_for_mac(&self, _mac: &str) -> Result<DhcpScope, AppError> {
            Ok(test_scope())
        }
    }

    let service: Arc<dyn DhcpService> = Arc::new(FailingRenewService);
    let msg = build_request([0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff]);

    let result = server::handle_request(&service, &msg, "aa:bb:cc:dd:ee:ff").await;
    assert!(result.is_err());
}

// ---------------------------------------------------------------------------
// send_response tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn send_response_encodes_and_sends() {
    let socket = MockDhcpSocket::new();
    let lease = test_lease();
    let scope = test_scope();
    let request = build_discover([0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff]);
    let response = server::build_response(&request, MessageType::Offer, &lease, &scope);
    let dest: SocketAddr = "192.168.1.50:68".parse().unwrap();

    server::send_response(&socket, &response, dest).await;

    let sent = socket.sent_packets().await;
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].1, dest);

    // Verify the sent bytes decode to a valid DHCP message.
    let decoded = Message::decode(&mut Decoder::new(&sent[0].0)).unwrap();
    assert_eq!(decoded.opts().msg_type(), Some(MessageType::Offer));
    assert_eq!(decoded.yiaddr(), lease.ip_address);
}

// ---------------------------------------------------------------------------
// server_loop integration tests (using MockDhcpSocket)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn server_loop_responds_to_discover_with_offer() {
    let lease = test_lease();
    let service: Arc<dyn DhcpService> = Arc::new(MockDhcpService::new(lease.clone()));
    let socket = Arc::new(MockDhcpSocket::new());

    let discover = build_discover([0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff]);
    socket.push_message(&discover, client_addr()).await;

    let socket = run_server_loop_until_idle(socket, service).await;

    let messages = socket.sent_messages().await;
    assert_eq!(messages.len(), 1, "expected exactly one response");
    assert_eq!(messages[0].0.opts().msg_type(), Some(MessageType::Offer));
    assert_eq!(messages[0].0.yiaddr(), lease.ip_address);
    // Per RFC 2131 §4.1, DHCPOFFER is broadcast to 255.255.255.255:68 when the
    // client's `ciaddr` is 0.0.0.0 (which it is in a fresh DHCPDISCOVER); the
    // client cannot accept unicast IP-layer traffic until it has bound its
    // offered address.
    assert_eq!(
        messages[0].1,
        "255.255.255.255:68".parse::<SocketAddr>().unwrap()
    );
}

#[tokio::test]
async fn server_loop_ignores_discover_from_own_mac() {
    let lease = test_lease();
    let service: Arc<dyn DhcpService> = Arc::new(MockDhcpService::new(lease));
    let socket = Arc::new(MockDhcpSocket::new());

    let own_chaddr = [0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff];
    let discover = build_discover(own_chaddr);
    socket.push_message(&discover, client_addr()).await;

    let running = Arc::new(AtomicBool::new(true));
    let cancel = tokio_util::sync::CancellationToken::new();
    let cancel_clone = cancel.clone();
    let socket_dyn: Arc<dyn DhcpSocket> = Arc::clone(&socket) as Arc<dyn DhcpSocket>;
    let running_clone = Arc::clone(&running);
    let own_macs = Arc::new(HashSet::from([server::format_mac(&own_chaddr)]));

    let handle = tokio::spawn(async move {
        server::server_loop(
            socket_dyn,
            service,
            None,
            running_clone,
            cancel_clone,
            own_macs,
        )
        .await;
    });

    for _ in 0..20 {
        tokio::task::yield_now().await;
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    cancel.cancel();
    let _ = handle.await;

    let messages = socket.sent_messages().await;
    assert!(
        messages.is_empty(),
        "expected no DHCPOFFER for a DISCOVER from the daemon's own interface, got {messages:?}"
    );
}

#[tokio::test]
async fn server_loop_responds_to_request_with_ack() {
    let lease = test_lease();
    let service: Arc<dyn DhcpService> = Arc::new(MockDhcpService::new(lease.clone()));
    let socket = Arc::new(MockDhcpSocket::new());

    let request = build_request([0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff]);
    socket.push_message(&request, client_addr()).await;

    let socket = run_server_loop_until_idle(socket, service).await;

    let messages = socket.sent_messages().await;
    assert_eq!(messages.len(), 1, "expected exactly one response");
    assert_eq!(messages[0].0.opts().msg_type(), Some(MessageType::Ack));
    assert_eq!(messages[0].0.yiaddr(), lease.ip_address);
}

#[tokio::test]
async fn server_loop_handles_release_without_response() {
    let lease = test_lease();
    // A legitimate release is unicast from the client's own leased address, so
    // the packet source must match the lease IP for the release to be honoured.
    let lease_src = SocketAddr::from((lease.ip_address, 68));
    let mock_service = Arc::new(MockDhcpService::new(lease));
    let service: Arc<dyn DhcpService> = Arc::clone(&mock_service) as Arc<dyn DhcpService>;
    let socket = Arc::new(MockDhcpSocket::new());

    let release = build_release([0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff]);
    socket.push_message(&release, lease_src).await;

    let socket = run_server_loop_until_idle(socket, service).await;

    // No response should be sent for a RELEASE.
    let messages = socket.sent_messages().await;
    assert!(messages.is_empty(), "RELEASE should not produce a response");

    // But the service should have been called.
    let calls = mock_service.recorded_calls().await;
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].0, "release_lease");
    assert_eq!(calls[0].1, "aa:bb:cc:dd:ee:ff");
}

#[tokio::test]
async fn server_loop_release_from_lease_ip_releases_lease() {
    // A DHCPRELEASE whose source is the lease's own recorded IP is a legitimate
    // client releasing its own lease and must free it (RFC 2131 unicast release).
    let lease = test_lease(); // ip 192.168.1.100, mac aa:bb:cc:dd:ee:ff
    let lease_src = SocketAddr::from((lease.ip_address, 68));
    let mock_service = Arc::new(MockDhcpService::new(lease));
    let service: Arc<dyn DhcpService> = Arc::clone(&mock_service) as Arc<dyn DhcpService>;
    let socket = Arc::new(MockDhcpSocket::new());

    let release = build_release([0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff]);
    socket.push_message(&release, lease_src).await;

    let socket = run_server_loop_until_idle(socket, service).await;

    // RELEASE never produces a response.
    assert!(socket.sent_messages().await.is_empty());

    // The lease was released because the source matched its recorded IP.
    let calls = mock_service.recorded_calls().await;
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].0, "release_lease");
    assert_eq!(calls[0].1, "aa:bb:cc:dd:ee:ff");
}

#[tokio::test]
async fn server_loop_release_from_spoofed_source_does_not_release_lease() {
    // Attacker forges the victim's MAC in a DHCPRELEASE but sends it from a
    // different source IP (and no ciaddr). The claimed source does not match the
    // victim's recorded lease IP, so the release MUST be dropped — the victim's
    // lease stays intact (CWE-639, finding F3).
    let lease = test_lease(); // victim: ip 192.168.1.100, mac aa:bb:cc:dd:ee:ff
    let mock_service = Arc::new(MockDhcpService::new(lease));
    let service: Arc<dyn DhcpService> = Arc::clone(&mock_service) as Arc<dyn DhcpService>;
    let socket = Arc::new(MockDhcpSocket::new());

    let forged = build_release([0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff]);
    let attacker_src: SocketAddr = "192.168.1.66:68".parse().unwrap();
    socket.push_message(&forged, attacker_src).await;

    let socket = run_server_loop_until_idle(socket, service).await;

    // RELEASE never produces a response.
    assert!(socket.sent_messages().await.is_empty());

    // Crucially, `release_lease` was never called — the victim keeps its lease.
    let calls = mock_service.recorded_calls().await;
    assert!(
        !calls.iter().any(|(method, _)| method == "release_lease"),
        "forged DHCPRELEASE from a spoofed source must not release the victim's lease, got: {calls:?}"
    );
}

#[tokio::test]
async fn server_loop_release_with_forged_ciaddr_does_not_release_lease() {
    // The `ciaddr` wire field is attacker-controlled packet payload. An attacker
    // forges the victim's MAC AND sets `ciaddr` to the victim's lease IP — both
    // discoverable via ARP — but sends from their own source address, with no
    // network-layer spoofing. Authorization is on the real UDP source, not
    // `ciaddr`, so the source (192.168.1.66) does not match the victim's lease
    // IP (192.168.1.100) and the release MUST be dropped (CWE-639, finding F3).
    let lease = test_lease(); // victim: ip 192.168.1.100, mac aa:bb:cc:dd:ee:ff
    let mock_service = Arc::new(MockDhcpService::new(lease));
    let service: Arc<dyn DhcpService> = Arc::clone(&mock_service) as Arc<dyn DhcpService>;
    let socket = Arc::new(MockDhcpSocket::new());

    let victim_ip: Ipv4Addr = "192.168.1.100".parse().unwrap();
    let forged = build_release_with_ciaddr([0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff], victim_ip);
    let attacker_src: SocketAddr = "192.168.1.66:68".parse().unwrap();
    socket.push_message(&forged, attacker_src).await;

    let socket = run_server_loop_until_idle(socket, service).await;

    // RELEASE never produces a response.
    assert!(socket.sent_messages().await.is_empty());

    // The victim keeps its lease: trusting `ciaddr` would have wrongly freed it.
    let calls = mock_service.recorded_calls().await;
    assert!(
        !calls.iter().any(|(method, _)| method == "release_lease"),
        "a DHCPRELEASE with a forged ciaddr from a mismatched source must not release the victim's lease, got: {calls:?}"
    );
}

#[tokio::test]
async fn server_loop_release_from_ipv6_source_does_not_release_lease() {
    // A DHCPv4 client is always reached over IPv4. A release arriving from an
    // IPv6 source cannot own an IPv4 lease, so its claimed IPv4 source collapses
    // to 0.0.0.0 and the packet is dropped as unauthenticated.
    let lease = test_lease();
    let mock_service = Arc::new(MockDhcpService::new(lease));
    let service: Arc<dyn DhcpService> = Arc::clone(&mock_service) as Arc<dyn DhcpService>;
    let socket = Arc::new(MockDhcpSocket::new());

    let release = build_release([0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff]);
    let ipv6_src: SocketAddr = "[fe80::1]:68".parse().unwrap();
    socket.push_message(&release, ipv6_src).await;

    let socket = run_server_loop_until_idle(socket, service).await;
    assert!(socket.sent_messages().await.is_empty());

    let calls = mock_service.recorded_calls().await;
    assert!(
        !calls.iter().any(|(method, _)| method == "release_lease"),
        "a DHCPRELEASE from an IPv6 source must not release an IPv4 lease, got: {calls:?}"
    );
}

#[tokio::test]
async fn server_loop_release_for_unknown_mac_does_not_release_lease() {
    // A release for a MAC with no recorded active lease is dropped: there is
    // nothing to authorise the release against.
    let lease = test_lease(); // recorded mac aa:bb:cc:dd:ee:ff
    let mock_service = Arc::new(MockDhcpService::new(lease));
    let service: Arc<dyn DhcpService> = Arc::clone(&mock_service) as Arc<dyn DhcpService>;
    let socket = Arc::new(MockDhcpSocket::new());

    let release = build_release([0x11, 0x22, 0x33, 0x44, 0x55, 0x66]);
    let src: SocketAddr = "192.168.1.77:68".parse().unwrap();
    socket.push_message(&release, src).await;

    let socket = run_server_loop_until_idle(socket, service).await;
    assert!(socket.sent_messages().await.is_empty());

    let calls = mock_service.recorded_calls().await;
    assert!(
        !calls.iter().any(|(method, _)| method == "release_lease"),
        "a DHCPRELEASE for a MAC with no active lease must not release anything, got: {calls:?}"
    );
}

#[tokio::test]
async fn server_loop_release_lookup_error_does_not_release_lease() {
    // If the active-lease lookup itself fails, the release is dropped
    // (fail-closed): we never release without a verified ownership match.
    let lease = test_lease();
    let lease_src = SocketAddr::from((lease.ip_address, 68));
    let mock_service = Arc::new(MockDhcpService::new(lease).failing_active_lease());
    let service: Arc<dyn DhcpService> = Arc::clone(&mock_service) as Arc<dyn DhcpService>;
    let socket = Arc::new(MockDhcpSocket::new());

    let release = build_release([0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff]);
    socket.push_message(&release, lease_src).await;

    let socket = run_server_loop_until_idle(socket, service).await;
    assert!(socket.sent_messages().await.is_empty());

    let calls = mock_service.recorded_calls().await;
    assert!(
        !calls.iter().any(|(method, _)| method == "release_lease"),
        "a failed active-lease lookup must not release the lease, got: {calls:?}"
    );
}

#[tokio::test]
async fn server_loop_release_service_error_is_logged_and_swallowed() {
    // An authorised release whose service call fails is logged and does not
    // crash the loop. The release was attempted (the call is recorded before
    // the service returns its error).
    let lease = test_lease();
    let lease_src = SocketAddr::from((lease.ip_address, 68));
    let mock_service = Arc::new(MockDhcpService::new(lease).failing_release());
    let service: Arc<dyn DhcpService> = Arc::clone(&mock_service) as Arc<dyn DhcpService>;
    let socket = Arc::new(MockDhcpSocket::new());

    let release = build_release([0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff]);
    socket.push_message(&release, lease_src).await;

    let socket = run_server_loop_until_idle(socket, service).await;
    assert!(socket.sent_messages().await.is_empty());

    // The release was authorised and attempted even though the service errored.
    let calls = mock_service.recorded_calls().await;
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].0, "release_lease");
    assert_eq!(calls[0].1, "aa:bb:cc:dd:ee:ff");
}

#[tokio::test]
async fn server_loop_ignores_garbage_packets() {
    let lease = test_lease();
    let service: Arc<dyn DhcpService> = Arc::new(MockDhcpService::new(lease.clone()));
    let socket = Arc::new(MockDhcpSocket::new());

    // Push garbage bytes first.
    socket
        .push_packet(vec![0xde, 0xad, 0xbe, 0xef], client_addr())
        .await;

    // Then push a valid DISCOVER.
    let discover = build_discover([0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff]);
    socket.push_message(&discover, client_addr()).await;

    let socket = run_server_loop_until_idle(socket, service).await;

    // The server should have recovered and responded to the DISCOVER.
    let messages = socket.sent_messages().await;
    assert_eq!(
        messages.len(),
        1,
        "server should still produce the OFFER after garbage"
    );
    assert_eq!(messages[0].0.opts().msg_type(), Some(MessageType::Offer));
}

#[tokio::test]
async fn server_loop_ignores_message_without_type() {
    let lease = test_lease();
    let mock_service = Arc::new(MockDhcpService::new(lease));
    let service: Arc<dyn DhcpService> = Arc::clone(&mock_service) as Arc<dyn DhcpService>;
    let socket = Arc::new(MockDhcpSocket::new());

    // Build a valid DHCP message but without a MessageType option.
    let mut msg = Message::default();
    msg.set_opcode(Opcode::BootRequest)
        .set_chaddr(&[0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff]);
    // No MessageType inserted.
    socket.push_message(&msg, client_addr()).await;

    let socket = run_server_loop_until_idle(socket, service).await;

    // No response and no service calls.
    let messages = socket.sent_messages().await;
    assert!(
        messages.is_empty(),
        "message without type should be ignored"
    );
    let calls = mock_service.recorded_calls().await;
    assert!(calls.is_empty());
}

#[tokio::test]
async fn server_loop_stops_on_cancellation() {
    let lease = test_lease();
    let service: Arc<dyn DhcpService> = Arc::new(MockDhcpService::new(lease));
    let socket = Arc::new(MockDhcpSocket::new());

    let running = Arc::new(AtomicBool::new(true));
    let cancel = tokio_util::sync::CancellationToken::new();

    let cancel_clone = cancel.clone();
    let socket_dyn: Arc<dyn DhcpSocket> = Arc::clone(&socket) as Arc<dyn DhcpSocket>;
    let running_clone = Arc::clone(&running);
    let own_macs = Arc::new(HashSet::new());

    let handle = tokio::spawn(async move {
        server::server_loop(
            socket_dyn,
            service,
            None,
            running_clone,
            cancel_clone,
            own_macs,
        )
        .await;
    });

    // Immediately cancel.
    cancel.cancel();

    // The task should complete promptly.
    let result = tokio::time::timeout(std::time::Duration::from_secs(2), handle).await;
    assert!(result.is_ok(), "server_loop should exit on cancellation");
    assert!(
        !running.load(Ordering::SeqCst),
        "running flag should be cleared"
    );
}

// ---------------------------------------------------------------------------
// UdpDhcpServer start/stop tests (using MockDhcpSocket)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn udp_server_start_sets_running_flag() {
    let lease = test_lease();
    let service: Arc<dyn DhcpService> = Arc::new(MockDhcpService::new(lease));
    let socket: Arc<dyn DhcpSocket> = Arc::new(MockDhcpSocket::new());

    let server = UdpDhcpServer::with_socket(service, socket);

    server.start().await.unwrap();
    assert!(server.is_running(), "server should be running after start");

    server.stop().await.unwrap();
}

#[tokio::test]
async fn udp_server_stop_clears_running_flag() {
    let lease = test_lease();
    let service: Arc<dyn DhcpService> = Arc::new(MockDhcpService::new(lease));
    let socket: Arc<dyn DhcpSocket> = Arc::new(MockDhcpSocket::new());

    let server = UdpDhcpServer::with_socket(service, socket);

    server.start().await.unwrap();
    assert!(server.is_running());

    server.stop().await.unwrap();

    // Give the spawned task a moment to complete.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    assert!(
        !server.is_running(),
        "server should not be running after stop"
    );
}

#[tokio::test]
async fn udp_server_start_when_already_running() {
    let lease = test_lease();
    let service: Arc<dyn DhcpService> = Arc::new(MockDhcpService::new(lease));
    let socket: Arc<dyn DhcpSocket> = Arc::new(MockDhcpSocket::new());

    let server = UdpDhcpServer::with_socket(service, socket);

    server.start().await.unwrap();
    // Second start should be a no-op (returns Ok).
    server.start().await.unwrap();
    assert!(server.is_running());

    server.stop().await.unwrap();
}

#[tokio::test]
async fn udp_server_stop_when_not_running() {
    let lease = test_lease();
    let service: Arc<dyn DhcpService> = Arc::new(MockDhcpService::new(lease));
    let socket: Arc<dyn DhcpSocket> = Arc::new(MockDhcpSocket::new());

    let server = UdpDhcpServer::with_socket(service, socket);

    // Stop without start should be a no-op.
    server.stop().await.unwrap();
    assert!(!server.is_running());
}

// ---------------------------------------------------------------------------
// server_loop processes multiple messages in sequence
// ---------------------------------------------------------------------------

#[tokio::test]
async fn server_loop_handles_discover_then_request_sequence() {
    let lease = test_lease();
    let mock_service = Arc::new(MockDhcpService::new(lease.clone()));
    let service: Arc<dyn DhcpService> = Arc::clone(&mock_service) as Arc<dyn DhcpService>;
    let socket = Arc::new(MockDhcpSocket::new());
    let mac = [0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff];

    // Push DISCOVER followed by REQUEST (normal DHCP flow).
    let discover = build_discover(mac);
    socket.push_message(&discover, client_addr()).await;

    let request = build_request(mac);
    socket.push_message(&request, client_addr()).await;

    let socket = run_server_loop_until_idle(socket, service).await;

    let messages = socket.sent_messages().await;
    assert_eq!(messages.len(), 2, "expected OFFER + ACK");
    assert_eq!(messages[0].0.opts().msg_type(), Some(MessageType::Offer));
    assert_eq!(messages[1].0.opts().msg_type(), Some(MessageType::Ack));

    let calls = mock_service.recorded_calls().await;
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].0, "assign_lease");
    assert_eq!(calls[1].0, "renew_lease");
}

// ---------------------------------------------------------------------------
// server_loop: attacker-controlled hlen (issue #829)
// ---------------------------------------------------------------------------

/// Encode `msg` and overwrite the BOOTP `hlen` field (byte offset 2) with an
/// attacker-controlled value. `dhcproto`'s `set_chaddr` clamps `hlen` to 16,
/// so an oversized value can only be produced by patching the raw bytes —
/// exactly what an attacker on the wire would send.
fn encode_with_hlen(msg: &Message, hlen: u8) -> Vec<u8> {
    let mut buf = encode_message(msg);
    buf[2] = hlen;
    buf
}

/// Encode `msg` and overwrite the BOOTP `htype` field (byte offset 1),
/// crafting a non-Ethernet hardware type without dhcproto's setters.
fn encode_with_htype(msg: &Message, htype: u8) -> Vec<u8> {
    let mut buf = encode_message(msg);
    buf[1] = htype;
    buf
}

#[tokio::test]
async fn server_loop_drops_oversized_hlen_and_keeps_serving() {
    let lease = test_lease();
    let mock_service = Arc::new(MockDhcpService::new(lease.clone()));
    let service: Arc<dyn DhcpService> = Arc::clone(&mock_service) as Arc<dyn DhcpService>;
    let socket = Arc::new(MockDhcpSocket::new());

    // Valid option-53 DISCOVERs whose wire hlen exceeds the 16-byte chaddr
    // array — both the minimal (17) and maximal (255) malicious values.
    let malicious = build_discover([0xde, 0xad, 0xbe, 0xef, 0x00, 0x01]);
    socket
        .push_packet(encode_with_hlen(&malicious, 17), client_addr())
        .await;
    socket
        .push_packet(encode_with_hlen(&malicious, 255), client_addr())
        .await;

    // A legitimate DISCOVER after the malicious packets: it only gets an
    // OFFER if the loop survived (did not panic on the oversized hlen).
    let valid = build_discover([0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff]);
    socket.push_message(&valid, client_addr()).await;

    let socket = run_server_loop_until_idle(socket, service).await;

    let messages = socket.sent_messages().await;
    assert_eq!(
        messages.len(),
        1,
        "malicious packets must be dropped and the valid DISCOVER must still get an OFFER"
    );
    assert_eq!(messages[0].0.opts().msg_type(), Some(MessageType::Offer));

    // The service must never have seen the malicious packets.
    let calls = mock_service.recorded_calls().await;
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].0, "assign_lease");
    assert_eq!(calls[0].1, "aa:bb:cc:dd:ee:ff");
}

#[tokio::test]
async fn udp_server_survives_oversized_hlen_packet() {
    let lease = test_lease();
    let service: Arc<dyn DhcpService> = Arc::new(MockDhcpService::new(lease.clone()));
    let socket = Arc::new(MockDhcpSocket::new());
    let socket_dyn: Arc<dyn DhcpSocket> = Arc::clone(&socket) as Arc<dyn DhcpSocket>;

    let server = UdpDhcpServer::with_socket(service, socket_dyn);
    server.start().await.unwrap();

    // One unauthenticated packet with hlen > 16 must not kill the task. The
    // valid DISCOVER queued behind it only gets an OFFER if the loop survived
    // (the FIFO socket guarantees the malicious packet was consumed first).
    let malicious = build_discover([0xde, 0xad, 0xbe, 0xef, 0x00, 0x01]);
    socket
        .push_packet(encode_with_hlen(&malicious, 255), client_addr())
        .await;
    let valid = build_discover([0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff]);
    socket.push_message(&valid, client_addr()).await;

    wait_for_sent(&socket, 1).await;

    // The OFFER above proves is_running() reflects a live loop rather than
    // a stale flag left behind by a panicked task.
    assert!(
        server.is_running(),
        "server must still be running after an oversized-hlen packet"
    );

    let messages = socket.sent_messages().await;
    assert_eq!(
        messages.len(),
        1,
        "a valid DISCOVER after the malicious packet must still be answered"
    );
    assert_eq!(messages[0].0.opts().msg_type(), Some(MessageType::Offer));

    server.stop().await.unwrap();
}

#[tokio::test]
async fn server_loop_drops_non_ethernet_hardware_addresses() {
    let lease = test_lease();
    let mock_service = Arc::new(MockDhcpService::new(lease.clone()));
    let service: Arc<dyn DhcpService> = Arc::clone(&mock_service) as Arc<dyn DhcpService>;
    let socket = Arc::new(MockDhcpSocket::new());

    // A legitimate DHCP client on this network is always Ethernet/Wi-Fi:
    // htype 1 with a 6-byte hardware address. hlen 0-5 would otherwise
    // become truncated or empty MAC strings used as lease-store identity
    // keys; hlen 7-16 and foreign htypes are not real clients here.
    let malicious = build_discover([0xde, 0xad, 0xbe, 0xef, 0x00, 0x02]);
    for packet in [
        encode_with_hlen(&malicious, 0),
        encode_with_hlen(&malicious, 5),
        encode_with_hlen(&malicious, 16),
        encode_with_htype(&malicious, 6), // htype 6 = IEEE 802, hlen still 6
    ] {
        socket.push_packet(packet, client_addr()).await;
    }

    let valid = build_discover([0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff]);
    socket.push_message(&valid, client_addr()).await;

    let socket = run_server_loop_until_idle(socket, service).await;

    let messages = socket.sent_messages().await;
    assert_eq!(
        messages.len(),
        1,
        "non-Ethernet hardware addresses must be dropped; only the valid DISCOVER gets an OFFER"
    );
    assert_eq!(messages[0].0.opts().msg_type(), Some(MessageType::Offer));

    // The lease service must never see the degenerate identities.
    let calls = mock_service.recorded_calls().await;
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].0, "assign_lease");
    assert_eq!(calls[0].1, "aa:bb:cc:dd:ee:ff");
}

/// The shared wire-decode bound used by every non-server decode site (the
/// pnet network probe): oversized `hlen` is rejected at decode time so no
/// later `chaddr()` call can panic, while any in-bounds `hlen` still decodes.
#[test]
fn decode_bounded_rejects_oversized_hlen() {
    let msg = build_discover([0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff]);

    assert!(server::decode_bounded(&encode_with_hlen(&msg, 17)).is_none());
    assert!(server::decode_bounded(&encode_with_hlen(&msg, 255)).is_none());

    // In-bounds hlen values decode, including the 16-byte boundary.
    assert!(server::decode_bounded(&encode_message(&msg)).is_some());
    assert!(server::decode_bounded(&encode_with_hlen(&msg, 16)).is_some());

    // Undecodable bytes are rejected, not panicked on.
    assert!(server::decode_bounded(&[0xde, 0xad]).is_none());
}

// ---------------------------------------------------------------------------
// server_loop: recv error recovery
// ---------------------------------------------------------------------------

/// Mock socket that returns an IO error on the first recv, then blocks forever.
struct RecvErrorSocket {
    error_returned: std::sync::atomic::AtomicBool,
    /// Packets sent via `send_to`.
    outgoing: Mutex<Vec<(Vec<u8>, SocketAddr)>>,
    notify: tokio::sync::Notify,
}

impl RecvErrorSocket {
    fn new() -> Self {
        Self {
            error_returned: std::sync::atomic::AtomicBool::new(false),
            outgoing: Mutex::new(Vec::new()),
            notify: tokio::sync::Notify::new(),
        }
    }
}

#[async_trait]
impl DhcpSocket for RecvErrorSocket {
    async fn recv_from(&self, _buf: &mut [u8]) -> std::io::Result<(usize, SocketAddr)> {
        if !self.error_returned.swap(true, Ordering::SeqCst) {
            return Err(std::io::Error::other("simulated recv error"));
        }
        // Block forever after the error so the test can cancel.
        self.notify.notified().await;
        unreachable!()
    }

    async fn send_to(&self, buf: &[u8], target: SocketAddr) -> std::io::Result<usize> {
        let data = buf.to_vec();
        let len = data.len();
        self.outgoing.lock().await.push((data, target));
        Ok(len)
    }
}

#[tokio::test]
async fn server_loop_continues_after_recv_error() {
    let lease = test_lease();
    let service: Arc<dyn DhcpService> = Arc::new(MockDhcpService::new(lease));
    let socket = Arc::new(RecvErrorSocket::new());

    let running = Arc::new(AtomicBool::new(true));
    let cancel = tokio_util::sync::CancellationToken::new();

    let cancel_clone = cancel.clone();
    let socket_dyn: Arc<dyn DhcpSocket> = Arc::clone(&socket) as Arc<dyn DhcpSocket>;
    let running_clone = Arc::clone(&running);
    let own_macs = Arc::new(HashSet::new());

    let handle = tokio::spawn(async move {
        server::server_loop(
            socket_dyn,
            service,
            None,
            running_clone,
            cancel_clone,
            own_macs,
        )
        .await;
    });

    // Give the loop time to hit the error and continue.
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // The server should still be running (error was recovered from).
    assert!(running.load(Ordering::SeqCst));

    cancel.cancel();
    let _ = handle.await;
}

// ---------------------------------------------------------------------------
// server_loop: ignores unsupported message type
// ---------------------------------------------------------------------------

#[tokio::test]
async fn server_loop_ignores_unsupported_message_type() {
    let lease = test_lease();
    let mock_service = Arc::new(MockDhcpService::new(lease));
    let service: Arc<dyn DhcpService> = Arc::clone(&mock_service) as Arc<dyn DhcpService>;
    let socket = Arc::new(MockDhcpSocket::new());

    // Build a DHCP INFORM message (unsupported type).
    let mut msg = Message::default();
    msg.set_opcode(Opcode::BootRequest)
        .set_chaddr(&[0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff]);
    msg.opts_mut()
        .insert(DhcpOption::MessageType(MessageType::Inform));
    socket.push_message(&msg, client_addr()).await;

    let socket = run_server_loop_until_idle(socket, service).await;

    // No response should be sent for an unsupported type.
    let messages = socket.sent_messages().await;
    assert!(
        messages.is_empty(),
        "unsupported message type should be ignored"
    );

    // Service should not have been called.
    let calls = mock_service.recorded_calls().await;
    assert!(calls.is_empty());
}

// ---------------------------------------------------------------------------
// send_response: send_to error
// ---------------------------------------------------------------------------

/// Mock socket that always fails on `send_to`.
struct SendErrorSocket;

#[async_trait]
impl DhcpSocket for SendErrorSocket {
    async fn recv_from(&self, _buf: &mut [u8]) -> std::io::Result<(usize, SocketAddr)> {
        unreachable!()
    }

    async fn send_to(&self, _buf: &[u8], _target: SocketAddr) -> std::io::Result<usize> {
        Err(std::io::Error::other("simulated send error"))
    }
}

#[tokio::test]
async fn send_response_handles_send_error_gracefully() {
    let socket = SendErrorSocket;
    let lease = test_lease();
    let scope = test_scope();
    let request = build_discover([0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff]);
    let response = server::build_response(&request, MessageType::Offer, &lease, &scope);
    let dest: SocketAddr = "192.168.1.50:68".parse().unwrap();

    // Should not panic -- just logs the error.
    server::send_response(&socket, &response, dest).await;
}

// ---------------------------------------------------------------------------
// build_response: router_ip == server_ip (no duplicate in routers list)
// ---------------------------------------------------------------------------

#[test]
fn build_response_router_ip_same_as_server_ip_no_duplicate() {
    let request = build_discover([0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff]);
    let lease = test_lease();
    let mut scope = test_scope();
    // Set router_ip == gateway_ip so the duplicate is elided from the list.
    scope.router_ip = Some(Ipv4Addr::new(192, 168, 1, 1));

    let response =
        crate::dhcp::server::build_response(&request, MessageType::Offer, &lease, &scope);

    // Encode/decode to inspect the Router option.
    let mut buf = Vec::new();
    let mut encoder = Encoder::new(&mut buf);
    response.encode(&mut encoder).unwrap();
    let decoded = Message::decode(&mut Decoder::new(&buf)).unwrap();

    // The Router option should contain exactly one entry (no duplicate).
    for (_code, opt) in decoded.opts().iter() {
        if let DhcpOption::Router(routers) = opt {
            assert_eq!(
                routers.len(),
                1,
                "router list should have 1 entry when router_ip == server_ip"
            );
            assert_eq!(routers[0], Ipv4Addr::new(192, 168, 1, 1));
        }
    }
}

// ---------------------------------------------------------------------------
// server_loop: discover/request error paths within the loop
// ---------------------------------------------------------------------------

#[tokio::test]
async fn server_loop_handles_discover_error_gracefully() {
    /// Mock service that fails on `assign_lease` but tracks calls.
    struct FailAssignService;

    #[async_trait]
    impl DhcpService for FailAssignService {
        async fn get_config(&self) -> Result<DhcpConfigResponse, AppError> {
            unimplemented!()
        }
        async fn update_config(
            &self,
            _r: UpdateDhcpConfigRequest,
        ) -> Result<DhcpConfigResponse, AppError> {
            unimplemented!()
        }
        async fn preview_config(
            &self,
            _req: PreviewDhcpConfigRequest,
        ) -> Result<PreviewDhcpConfigResponse, AppError> {
            Ok(PreviewDhcpConfigResponse {
                affected: Vec::new(),
            })
        }
        async fn toggle(&self, _r: ToggleDhcpRequest) -> Result<DhcpConfigResponse, AppError> {
            unimplemented!()
        }
        async fn list_leases(&self) -> Result<ListDhcpLeasesResponse, AppError> {
            unimplemented!()
        }
        async fn revoke_lease(&self, _id: Uuid) -> Result<RevokeDhcpLeaseResponse, AppError> {
            unimplemented!()
        }
        async fn list_reservations(&self) -> Result<ListDhcpReservationsResponse, AppError> {
            unimplemented!()
        }
        async fn create_reservation(
            &self,
            _r: CreateDhcpReservationRequest,
        ) -> Result<CreateDhcpReservationResponse, AppError> {
            unimplemented!()
        }
        async fn delete_reservation(
            &self,
            _id: Uuid,
        ) -> Result<DeleteDhcpReservationResponse, AppError> {
            unimplemented!()
        }
        async fn status(&self) -> Result<DhcpStatusResponse, AppError> {
            unimplemented!()
        }
        async fn assign_lease(
            &self,
            _mac: &str,
            _hostname: Option<&str>,
        ) -> Result<DhcpLease, AppError> {
            Err(AppError::Conflict("pool exhausted".to_owned()))
        }
        async fn renew_lease(
            &self,
            _mac: &str,
            _hostname: Option<&str>,
        ) -> Result<DhcpLease, AppError> {
            Err(AppError::NotFound("no lease".to_owned()))
        }
        async fn release_lease(&self, _mac: &str) -> Result<(), AppError> {
            Ok(())
        }
        async fn cleanup_expired(&self) -> Result<u64, AppError> {
            Ok(0)
        }
        async fn get_dhcp_config(&self) -> Result<DhcpConfig, AppError> {
            Ok(test_config())
        }
        async fn scope_for_mac(&self, _mac: &str) -> Result<DhcpScope, AppError> {
            Ok(test_scope())
        }
    }

    let service: Arc<dyn DhcpService> = Arc::new(FailAssignService);
    let socket = Arc::new(MockDhcpSocket::new());

    // Push a DISCOVER that will fail, followed by a RELEASE that will succeed.
    let discover = build_discover([0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff]);
    socket.push_message(&discover, client_addr()).await;

    let release = build_release([0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff]);
    socket.push_message(&release, client_addr()).await;

    let socket = run_server_loop_until_idle(socket, service).await;

    // No OFFER should be sent (discover failed), and no response for RELEASE.
    let messages = socket.sent_messages().await;
    assert!(messages.is_empty());
}

#[tokio::test]
async fn server_loop_handles_request_error_gracefully() {
    /// Mock service that fails on `renew_lease`.
    struct FailRenewService;

    #[async_trait]
    impl DhcpService for FailRenewService {
        async fn get_config(&self) -> Result<DhcpConfigResponse, AppError> {
            unimplemented!()
        }
        async fn update_config(
            &self,
            _r: UpdateDhcpConfigRequest,
        ) -> Result<DhcpConfigResponse, AppError> {
            unimplemented!()
        }
        async fn preview_config(
            &self,
            _req: PreviewDhcpConfigRequest,
        ) -> Result<PreviewDhcpConfigResponse, AppError> {
            Ok(PreviewDhcpConfigResponse {
                affected: Vec::new(),
            })
        }
        async fn toggle(&self, _r: ToggleDhcpRequest) -> Result<DhcpConfigResponse, AppError> {
            unimplemented!()
        }
        async fn list_leases(&self) -> Result<ListDhcpLeasesResponse, AppError> {
            unimplemented!()
        }
        async fn revoke_lease(&self, _id: Uuid) -> Result<RevokeDhcpLeaseResponse, AppError> {
            unimplemented!()
        }
        async fn list_reservations(&self) -> Result<ListDhcpReservationsResponse, AppError> {
            unimplemented!()
        }
        async fn create_reservation(
            &self,
            _r: CreateDhcpReservationRequest,
        ) -> Result<CreateDhcpReservationResponse, AppError> {
            unimplemented!()
        }
        async fn delete_reservation(
            &self,
            _id: Uuid,
        ) -> Result<DeleteDhcpReservationResponse, AppError> {
            unimplemented!()
        }
        async fn status(&self) -> Result<DhcpStatusResponse, AppError> {
            unimplemented!()
        }
        async fn assign_lease(
            &self,
            _mac: &str,
            _hostname: Option<&str>,
        ) -> Result<DhcpLease, AppError> {
            unimplemented!()
        }
        async fn renew_lease(
            &self,
            _mac: &str,
            _hostname: Option<&str>,
        ) -> Result<DhcpLease, AppError> {
            Err(AppError::Internal(anyhow::anyhow!("db error")))
        }
        async fn release_lease(&self, _mac: &str) -> Result<(), AppError> {
            Ok(())
        }
        async fn cleanup_expired(&self) -> Result<u64, AppError> {
            Ok(0)
        }
        async fn get_dhcp_config(&self) -> Result<DhcpConfig, AppError> {
            Ok(test_config())
        }
        async fn scope_for_mac(&self, _mac: &str) -> Result<DhcpScope, AppError> {
            Ok(test_scope())
        }
    }

    let service: Arc<dyn DhcpService> = Arc::new(FailRenewService);
    let socket = Arc::new(MockDhcpSocket::new());

    let request = build_request([0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff]);
    socket.push_message(&request, client_addr()).await;

    let socket = run_server_loop_until_idle(socket, service).await;

    // No ACK should be sent (renew failed).
    let messages = socket.sent_messages().await;
    assert!(messages.is_empty());
}

// ---------------------------------------------------------------------------
// UdpDhcpServer: restart cycle (stop then start again)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn udp_server_restart_cycle() {
    let lease = test_lease();
    let service: Arc<dyn DhcpService> = Arc::new(MockDhcpService::new(lease));
    let socket: Arc<dyn DhcpSocket> = Arc::new(MockDhcpSocket::new());

    let server = UdpDhcpServer::with_socket(service, socket);

    // First cycle.
    server.start().await.unwrap();
    assert!(server.is_running());
    server.stop().await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    assert!(!server.is_running());

    // Second cycle should work (fresh cancellation token).
    server.start().await.unwrap();
    assert!(server.is_running());
    server.stop().await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    assert!(!server.is_running());
}

/// Records every signal a DHCP message carries, so the spawned recorder can be
/// asserted on without racing it.
struct RecordingIdentification {
    seen: Arc<std::sync::Mutex<Vec<(String, DeviceSignalKind, String)>>>,
    /// When true, every call fails — the recorder must swallow it.
    fail: bool,
}

#[async_trait]
impl wardnetd_services::device::DeviceIdentificationService for RecordingIdentification {
    async fn record_signal(
        &self,
        _device_id: &str,
        _kind: DeviceSignalKind,
        _value: &str,
    ) -> Result<(), AppError> {
        unimplemented!("the DHCP path records by MAC")
    }

    async fn record_signal_for_mac(
        &self,
        mac: &str,
        kind: DeviceSignalKind,
        value: &str,
    ) -> Result<(), AppError> {
        if self.fail {
            return Err(AppError::Internal(anyhow::anyhow!("boom")));
        }
        self.seen
            .lock()
            .unwrap()
            .push((mac.to_owned(), kind, value.to_owned()));
        Ok(())
    }

    async fn record_signal_for_ip(
        &self,
        _ip: std::net::IpAddr,
        _kind: DeviceSignalKind,
        _value: &str,
    ) -> Result<(), AppError> {
        unimplemented!("the DHCP path records by MAC")
    }

    async fn signals_for(
        &self,
        _device_id: &str,
    ) -> Result<Vec<wardnet_common::device::DeviceSignal>, AppError> {
        unimplemented!("not exercised by the DHCP path")
    }

    async fn reconcile_from_catalog(&self) -> Result<usize, AppError> {
        unimplemented!("not exercised by the DHCP path")
    }

    async fn probe_device(
        &self,
        _device_id: &str,
    ) -> Result<wardnetd_services::device::ProbeOutcome, AppError> {
        // ADR 0025 §5: probing is an explicit admin action only. A DHCP packet
        // reaching here would be exactly the background scan the invariant
        // forbids, so the panic is the assertion.
        unimplemented!("the DHCP path must never trigger a probe")
    }
}

/// Build a DISCOVER carrying all three identification-bearing options.
fn build_discover_with_signals(mac: [u8; 6]) -> Message {
    let mut msg = build_discover(mac);
    msg.opts_mut().insert(DhcpOption::ParameterRequestList(vec![
        OptionCode::SubnetMask,
        OptionCode::Router,
    ]));
    msg.opts_mut()
        .insert(DhcpOption::ClassIdentifier(b"ubnt-unifi-ap".to_vec()));
    msg
}

#[tokio::test]
async fn record_dhcp_signals_captures_hostname_param_list_and_vendor_class() {
    let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
    let svc: Arc<dyn wardnetd_services::device::DeviceIdentificationService> =
        Arc::new(RecordingIdentification {
            seen: Arc::clone(&seen),
            fail: false,
        });
    let msg = build_discover_with_signals([0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff]);

    let handle = server::record_dhcp_signals(Some(&svc), &msg, "aa:bb:cc:dd:ee:ff")
        .expect("signals present, so a task should be spawned");
    handle.await.unwrap();

    let seen = seen.lock().unwrap().clone();
    assert_eq!(seen.len(), 3, "hostname + option 55 + option 60");
    assert!(seen.iter().all(|(m, _, _)| m == "aa:bb:cc:dd:ee:ff"));
    assert!(
        seen.iter()
            .any(|(_, k, v)| *k == DeviceSignalKind::DhcpParamList && v == "1,3"),
        "option 55 must keep the client's ordering: {seen:?}"
    );
    assert!(
        seen.iter()
            .any(|(_, k, v)| *k == DeviceSignalKind::DhcpVendorClass && v == "ubnt-unifi-ap"),
    );
}

#[tokio::test]
async fn record_dhcp_signals_is_a_no_op_without_an_identification_service() {
    let msg = build_discover_with_signals([0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff]);
    assert!(server::record_dhcp_signals(None, &msg, "aa:bb:cc:dd:ee:ff").is_none());
}

#[tokio::test]
async fn record_dhcp_signals_spawns_nothing_when_the_message_carries_none() {
    // A bare REQUEST has no hostname, no option 55 and no option 60.
    let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
    let svc: Arc<dyn wardnetd_services::device::DeviceIdentificationService> =
        Arc::new(RecordingIdentification { seen, fail: false });
    let msg = build_request([0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff]);

    assert!(server::record_dhcp_signals(Some(&svc), &msg, "aa:bb:cc:dd:ee:ff").is_none());
}

#[tokio::test]
async fn a_failing_signal_write_is_swallowed() {
    // Recording is best-effort: the lease must not depend on it.
    let svc: Arc<dyn wardnetd_services::device::DeviceIdentificationService> =
        Arc::new(RecordingIdentification {
            seen: Arc::new(std::sync::Mutex::new(Vec::new())),
            fail: true,
        });
    let msg = build_discover_with_signals([0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff]);

    let handle = server::record_dhcp_signals(Some(&svc), &msg, "aa:bb:cc:dd:ee:ff").unwrap();
    handle.await.expect("the recorder must not panic on error");
}
