use std::net::Ipv4Addr;

use crate::dhcp::{
    DhcpConfig, DhcpLease, DhcpLeaseEventType, DhcpLeaseLog, DhcpLeaseStatus, DhcpReservation,
};
use uuid::Uuid;

#[test]
fn lease_status_round_trip() {
    for status in [
        DhcpLeaseStatus::Active,
        DhcpLeaseStatus::Expired,
        DhcpLeaseStatus::Released,
    ] {
        let json = serde_json::to_string(&status).unwrap();
        let back: DhcpLeaseStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(status, back);
    }
}

#[test]
fn lease_status_snake_case_rename() {
    assert_eq!(
        serde_json::to_string(&DhcpLeaseStatus::Active).unwrap(),
        "\"active\""
    );
    assert_eq!(
        serde_json::to_string(&DhcpLeaseStatus::Expired).unwrap(),
        "\"expired\""
    );
    assert_eq!(
        serde_json::to_string(&DhcpLeaseStatus::Released).unwrap(),
        "\"released\""
    );
}

#[test]
fn lease_event_type_round_trip() {
    for event_type in [
        DhcpLeaseEventType::Assigned,
        DhcpLeaseEventType::Renewed,
        DhcpLeaseEventType::Released,
        DhcpLeaseEventType::Expired,
        DhcpLeaseEventType::Conflict,
    ] {
        let json = serde_json::to_string(&event_type).unwrap();
        let back: DhcpLeaseEventType = serde_json::from_str(&json).unwrap();
        assert_eq!(event_type, back);
    }
}

#[test]
fn lease_event_type_snake_case_rename() {
    assert_eq!(
        serde_json::to_string(&DhcpLeaseEventType::Assigned).unwrap(),
        "\"assigned\""
    );
    assert_eq!(
        serde_json::to_string(&DhcpLeaseEventType::Conflict).unwrap(),
        "\"conflict\""
    );
}

#[test]
fn dhcp_lease_round_trip() {
    let lease = DhcpLease {
        id: Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap(),
        mac_address: "AA:BB:CC:DD:EE:01".to_owned(),
        ip_address: Ipv4Addr::new(192, 168, 1, 100),
        hostname: Some("myphone".to_owned()),
        lease_start: "2026-03-07T00:00:00Z".parse().unwrap(),
        lease_end: "2026-03-08T00:00:00Z".parse().unwrap(),
        status: DhcpLeaseStatus::Active,
        device_id: None,
        created_at: "2026-03-07T00:00:00Z".parse().unwrap(),
        updated_at: "2026-03-07T00:00:00Z".parse().unwrap(),
    };
    let json = serde_json::to_string(&lease).unwrap();
    let back: DhcpLease = serde_json::from_str(&json).unwrap();
    assert_eq!(lease.id, back.id);
    assert_eq!(lease.mac_address, back.mac_address);
    assert_eq!(lease.ip_address, back.ip_address);
    assert_eq!(lease.hostname, back.hostname);
    assert_eq!(lease.status, back.status);
    assert_eq!(lease.lease_start, back.lease_start);
    assert_eq!(lease.lease_end, back.lease_end);
}

#[test]
fn dhcp_reservation_round_trip() {
    let reservation = DhcpReservation {
        id: Uuid::parse_str("00000000-0000-0000-0000-000000000002").unwrap(),
        mac_address: "AA:BB:CC:DD:EE:02".to_owned(),
        ip_address: Ipv4Addr::new(192, 168, 1, 50),
        hostname: Some("printer".to_owned()),
        description: Some("Office printer".to_owned()),
        created_at: "2026-03-07T00:00:00Z".parse().unwrap(),
        updated_at: "2026-03-07T00:00:00Z".parse().unwrap(),
    };
    let json = serde_json::to_string(&reservation).unwrap();
    let back: DhcpReservation = serde_json::from_str(&json).unwrap();
    assert_eq!(reservation.id, back.id);
    assert_eq!(reservation.mac_address, back.mac_address);
    assert_eq!(reservation.ip_address, back.ip_address);
    assert_eq!(reservation.hostname, back.hostname);
    assert_eq!(reservation.description, back.description);
}

#[test]
fn dhcp_config_round_trip() {
    let config = DhcpConfig {
        enabled: true,
        pool_start: Ipv4Addr::new(192, 168, 1, 100),
        pool_end: Ipv4Addr::new(192, 168, 1, 200),
        subnet_mask: Ipv4Addr::new(255, 255, 255, 0),
        upstream_dns: vec![Ipv4Addr::new(1, 1, 1, 1), Ipv4Addr::new(8, 8, 8, 8)],
        lease_duration_secs: 86400,
        router_ip: Some(Ipv4Addr::new(192, 168, 1, 1)),
    };
    let json = serde_json::to_string(&config).unwrap();
    let back: DhcpConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(config.enabled, back.enabled);
    assert_eq!(config.pool_start, back.pool_start);
    assert_eq!(config.pool_end, back.pool_end);
    assert_eq!(config.subnet_mask, back.subnet_mask);
    assert_eq!(config.upstream_dns, back.upstream_dns);
    assert_eq!(config.lease_duration_secs, back.lease_duration_secs);
    assert_eq!(config.router_ip, back.router_ip);
}

#[test]
fn dhcp_lease_log_round_trip() {
    let log = DhcpLeaseLog {
        id: 1,
        lease_id: Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap(),
        event_type: DhcpLeaseEventType::Assigned,
        details: Some("Initial assignment".to_owned()),
        created_at: "2026-03-07T00:00:00Z".parse().unwrap(),
    };
    let json = serde_json::to_string(&log).unwrap();
    let back: DhcpLeaseLog = serde_json::from_str(&json).unwrap();
    assert_eq!(log.id, back.id);
    assert_eq!(log.lease_id, back.lease_id);
    assert_eq!(log.event_type, back.event_type);
}
