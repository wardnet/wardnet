use crate::auth::*;
use uuid::Uuid;

#[test]
fn role_round_trip() {
    for role in [Role::Admin, Role::Public] {
        let json = serde_json::to_string(&role).unwrap();
        let back: Role = serde_json::from_str(&json).unwrap();
        assert_eq!(role, back);
    }
}

#[test]
fn session_round_trip() {
    let session = Session {
        id: Uuid::nil(),
        admin_id: Uuid::nil(),
        token_hash: "abc123".to_owned(),
        created_at: "2026-03-07T00:00:00Z".parse().unwrap(),
        expires_at: "2026-03-08T00:00:00Z".parse().unwrap(),
    };
    let json = serde_json::to_string(&session).unwrap();
    let back: Session = serde_json::from_str(&json).unwrap();
    assert_eq!(session.id, back.id);
    assert_eq!(session.token_hash, back.token_hash);
}

#[test]
fn auth_context_is_admin() {
    let admin = AuthContext::Admin {
        admin_id: Uuid::nil(),
    };
    assert!(admin.is_admin());

    let device = AuthContext::Device {
        mac: "AA:BB:CC:DD:EE:01".to_owned(),
    };
    assert!(!device.is_admin());

    assert!(!AuthContext::Anonymous.is_admin());
}

#[test]
fn auth_context_device_mac() {
    let admin = AuthContext::Admin {
        admin_id: Uuid::nil(),
    };
    assert!(admin.device_mac().is_none());

    let device = AuthContext::Device {
        mac: "AA:BB:CC:DD:EE:01".to_owned(),
    };
    assert_eq!(device.device_mac(), Some("AA:BB:CC:DD:EE:01"));

    assert!(AuthContext::Anonymous.device_mac().is_none());
}

#[test]
fn api_key_record_round_trip() {
    let record = ApiKeyRecord {
        id: Uuid::nil(),
        label: "CI key".to_owned(),
        key_hash: "hash123".to_owned(),
        created_at: "2026-03-07T00:00:00Z".parse().unwrap(),
        last_used_at: None,
    };
    let json = serde_json::to_string(&record).unwrap();
    let back: ApiKeyRecord = serde_json::from_str(&json).unwrap();
    assert_eq!(record.id, back.id);
    assert_eq!(record.label, back.label);
    assert!(back.last_used_at.is_none());
}
