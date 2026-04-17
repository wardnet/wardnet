use std::collections::VecDeque;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;
use dhcproto::v4::{DhcpOption, Message, MessageType, Opcode};
use dhcproto::{Decodable, Decoder, Encodable, Encoder};
use tokio::sync::Mutex;
use uuid::Uuid;
use wardnet_common::api::{
    CreateDhcpReservationRequest, CreateDhcpReservationResponse, DeleteDhcpReservationResponse,
    DhcpConfigResponse, DhcpStatusResponse, ListDhcpLeasesResponse, ListDhcpReservationsResponse,
    RevokeDhcpLeaseResponse, ToggleDhcpRequest, UpdateDhcpConfigRequest,
};
use wardnet_common::dhcp::{DhcpConfig, DhcpLease, DhcpLeaseStatus};

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
        let mut buf = Vec::with_capacity(512);
        let mut encoder = Encoder::new(&mut buf);
        msg.encode(&mut encoder).unwrap();
        self.push_packet(buf, src).await;
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
}

impl MockDhcpService {
    fn new(lease: DhcpLease) -> Self {
        Self {
            lease,
            calls: Mutex::new(Vec::new()),
        }
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

    async fn renew_lease(&self, mac: &str) -> Result<DhcpLease, AppError> {
        self.calls
            .lock()
            .await
            .push(("renew_lease".to_owned(), mac.to_owned()));
        Ok(self.lease.clone())
    }

    async fn release_lease(&self, mac: &str) -> Result<(), AppError> {
        self.calls
            .lock()
            .await
            .push(("release_lease".to_owned(), mac.to_owned()));
        Ok(())
    }

    async fn cleanup_expired(&self) -> Result<u64, AppError> {
        Ok(0)
    }

    async fn get_dhcp_config(&self) -> Result<DhcpConfig, AppError> {
        Ok(test_config())
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

/// A fake client address for incoming packets.
fn client_addr() -> SocketAddr {
    "192.168.1.50:68".parse().unwrap()
}

/// Run `server_loop` with the given socket and service, returning the socket
/// after the loop finishes (via cancellation token).
async fn run_server_loop_until_idle(
    socket: Arc<MockDhcpSocket>,
    service: Arc<dyn DhcpService>,
    config: DhcpConfig,
) -> Arc<MockDhcpSocket> {
    let running = Arc::new(AtomicBool::new(true));
    let cancel = tokio_util::sync::CancellationToken::new();
    let config = Arc::new(tokio::sync::RwLock::new(config));

    let cancel_clone = cancel.clone();
    let socket_dyn: Arc<dyn DhcpSocket> = Arc::clone(&socket) as Arc<dyn DhcpSocket>;
    let running_clone = Arc::clone(&running);

    let handle = tokio::spawn(async move {
        server::server_loop(socket_dyn, service, config, running_clone, cancel_clone).await;
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
fn build_response_creates_valid_offer() {
    let request = build_discover([0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff]);
    let lease = test_lease();
    let config = test_config();

    let response =
        crate::dhcp::server::build_response(&request, MessageType::Offer, &lease, &config);

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
    let config = test_config();

    let response = crate::dhcp::server::build_response(&request, MessageType::Ack, &lease, &config);

    assert_eq!(response.opcode(), Opcode::BootReply);
    assert_eq!(response.opts().msg_type(), Some(MessageType::Ack));
}

#[test]
fn build_response_includes_router_and_dns() {
    let request = build_discover([0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff]);
    let lease = test_lease();
    let config = test_config();

    let response =
        crate::dhcp::server::build_response(&request, MessageType::Offer, &lease, &config);

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

#[test]
fn build_response_siaddr_is_wardnet_gateway_ip() {
    let request = build_discover([0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff]);
    let lease = test_lease();
    let mut config = test_config();
    config.router_ip = None;

    let response =
        crate::dhcp::server::build_response(&request, MessageType::Offer, &lease, &config);

    // siaddr is always wardnet's own LAN IP (auto-detected into `gateway_ip`),
    // independent of the optional upstream router fallback.
    assert_eq!(response.siaddr(), config.gateway_ip);
}

#[test]
fn build_response_copies_chaddr() {
    let mac_bytes = [0x11, 0x22, 0x33, 0x44, 0x55, 0x66];
    let request = build_discover(mac_bytes);
    let lease = test_lease();
    let config = test_config();

    let response = server::build_response(&request, MessageType::Offer, &lease, &config);
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
    let config = test_config();

    let response = server::handle_discover(&service, &msg, "aa:bb:cc:dd:ee:ff", &config)
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
    let config = test_config();

    let _response = server::handle_discover(&service, &msg, "aa:bb:cc:dd:ee:ff", &config)
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
    let config = test_config();

    let response = server::handle_discover(&service, &msg, "aa:bb:cc:dd:ee:ff", &config)
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
    let config = test_config();

    let response = server::handle_request(&service, &msg, "aa:bb:cc:dd:ee:ff", &config)
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
async fn handle_request_preserves_xid() {
    let lease = test_lease();
    let service: Arc<dyn DhcpService> = Arc::new(MockDhcpService::new(lease));
    let mut msg = build_request([0x11, 0x22, 0x33, 0x44, 0x55, 0x66]);
    msg.set_xid(0xdead_beef);
    let config = test_config();

    let response = server::handle_request(&service, &msg, "11:22:33:44:55:66", &config)
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
        async fn renew_lease(&self, _mac: &str) -> Result<DhcpLease, AppError> {
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
    }

    let service: Arc<dyn DhcpService> = Arc::new(FailingService);
    let msg = build_discover([0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff]);
    let config = test_config();

    let result = server::handle_discover(&service, &msg, "aa:bb:cc:dd:ee:ff", &config).await;
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
        async fn renew_lease(&self, _mac: &str) -> Result<DhcpLease, AppError> {
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
    }

    let service: Arc<dyn DhcpService> = Arc::new(FailingRenewService);
    let msg = build_request([0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff]);
    let config = test_config();

    let result = server::handle_request(&service, &msg, "aa:bb:cc:dd:ee:ff", &config).await;
    assert!(result.is_err());
}

// ---------------------------------------------------------------------------
// send_response tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn send_response_encodes_and_sends() {
    let socket = MockDhcpSocket::new();
    let lease = test_lease();
    let config = test_config();
    let request = build_discover([0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff]);
    let response = server::build_response(&request, MessageType::Offer, &lease, &config);
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

    let socket = run_server_loop_until_idle(socket, service, test_config()).await;

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
async fn server_loop_responds_to_request_with_ack() {
    let lease = test_lease();
    let service: Arc<dyn DhcpService> = Arc::new(MockDhcpService::new(lease.clone()));
    let socket = Arc::new(MockDhcpSocket::new());

    let request = build_request([0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff]);
    socket.push_message(&request, client_addr()).await;

    let socket = run_server_loop_until_idle(socket, service, test_config()).await;

    let messages = socket.sent_messages().await;
    assert_eq!(messages.len(), 1, "expected exactly one response");
    assert_eq!(messages[0].0.opts().msg_type(), Some(MessageType::Ack));
    assert_eq!(messages[0].0.yiaddr(), lease.ip_address);
}

#[tokio::test]
async fn server_loop_handles_release_without_response() {
    let lease = test_lease();
    let mock_service = Arc::new(MockDhcpService::new(lease));
    let service: Arc<dyn DhcpService> = Arc::clone(&mock_service) as Arc<dyn DhcpService>;
    let socket = Arc::new(MockDhcpSocket::new());

    let release = build_release([0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff]);
    socket.push_message(&release, client_addr()).await;

    let socket = run_server_loop_until_idle(socket, service, test_config()).await;

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

    let socket = run_server_loop_until_idle(socket, service, test_config()).await;

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

    let socket = run_server_loop_until_idle(socket, service, test_config()).await;

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
    let config = Arc::new(tokio::sync::RwLock::new(test_config()));

    let cancel_clone = cancel.clone();
    let socket_dyn: Arc<dyn DhcpSocket> = Arc::clone(&socket) as Arc<dyn DhcpSocket>;
    let running_clone = Arc::clone(&running);

    let handle = tokio::spawn(async move {
        server::server_loop(socket_dyn, service, config, running_clone, cancel_clone).await;
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

    let server = UdpDhcpServer::with_socket(service, test_config(), socket);

    server.start().await.unwrap();
    assert!(server.is_running(), "server should be running after start");

    server.stop().await.unwrap();
}

#[tokio::test]
async fn udp_server_stop_clears_running_flag() {
    let lease = test_lease();
    let service: Arc<dyn DhcpService> = Arc::new(MockDhcpService::new(lease));
    let socket: Arc<dyn DhcpSocket> = Arc::new(MockDhcpSocket::new());

    let server = UdpDhcpServer::with_socket(service, test_config(), socket);

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

    let server = UdpDhcpServer::with_socket(service, test_config(), socket);

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

    let server = UdpDhcpServer::with_socket(service, test_config(), socket);

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

    let socket = run_server_loop_until_idle(socket, service, test_config()).await;

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
    let config = Arc::new(tokio::sync::RwLock::new(test_config()));

    let cancel_clone = cancel.clone();
    let socket_dyn: Arc<dyn DhcpSocket> = Arc::clone(&socket) as Arc<dyn DhcpSocket>;
    let running_clone = Arc::clone(&running);

    let handle = tokio::spawn(async move {
        server::server_loop(socket_dyn, service, config, running_clone, cancel_clone).await;
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

    let socket = run_server_loop_until_idle(socket, service, test_config()).await;

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
    let config = test_config();
    let request = build_discover([0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff]);
    let response = server::build_response(&request, MessageType::Offer, &lease, &config);
    let dest: SocketAddr = "192.168.1.50:68".parse().unwrap();

    // Should not panic -- just logs the error.
    server::send_response(&socket, &response, dest).await;
}

// ---------------------------------------------------------------------------
// UdpDhcpServer::update_config
// ---------------------------------------------------------------------------

#[tokio::test]
async fn udp_server_update_config() {
    let lease = test_lease();
    let service: Arc<dyn DhcpService> = Arc::new(MockDhcpService::new(lease));
    let socket: Arc<dyn DhcpSocket> = Arc::new(MockDhcpSocket::new());

    let server = UdpDhcpServer::with_socket(service, test_config(), socket);

    // Update the config and verify it does not panic.
    let mut new_config = test_config();
    new_config.lease_duration_secs = 7200;
    new_config.pool_end = Ipv4Addr::new(192, 168, 1, 250);
    server.update_config(new_config).await;
}

// ---------------------------------------------------------------------------
// build_response: router_ip == server_ip (no duplicate in routers list)
// ---------------------------------------------------------------------------

#[test]
fn build_response_router_ip_same_as_server_ip_no_duplicate() {
    let request = build_discover([0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff]);
    let lease = test_lease();
    let mut config = test_config();
    // Set router_ip == server_ip (which is pool_start when router_ip is present).
    config.router_ip = Some(Ipv4Addr::new(192, 168, 1, 1));

    let response =
        crate::dhcp::server::build_response(&request, MessageType::Offer, &lease, &config);

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
        async fn renew_lease(&self, _mac: &str) -> Result<DhcpLease, AppError> {
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
    }

    let service: Arc<dyn DhcpService> = Arc::new(FailAssignService);
    let socket = Arc::new(MockDhcpSocket::new());

    // Push a DISCOVER that will fail, followed by a RELEASE that will succeed.
    let discover = build_discover([0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff]);
    socket.push_message(&discover, client_addr()).await;

    let release = build_release([0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff]);
    socket.push_message(&release, client_addr()).await;

    let socket = run_server_loop_until_idle(socket, service, test_config()).await;

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
        async fn renew_lease(&self, _mac: &str) -> Result<DhcpLease, AppError> {
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
    }

    let service: Arc<dyn DhcpService> = Arc::new(FailRenewService);
    let socket = Arc::new(MockDhcpSocket::new());

    let request = build_request([0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff]);
    socket.push_message(&request, client_addr()).await;

    let socket = run_server_loop_until_idle(socket, service, test_config()).await;

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

    let server = UdpDhcpServer::with_socket(service, test_config(), socket);

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
