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
        user_id: Uuid::nil(),
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
fn user_role_round_trip() {
    for role in [UserRole::Admin, UserRole::Member] {
        let json = serde_json::to_string(&role).unwrap();
        let back: UserRole = serde_json::from_str(&json).unwrap();
        assert_eq!(role, back);
        assert_eq!(UserRole::parse(role.as_str()), Some(role));
    }
    // Never a lenient default: an unrecognised role resolving to `Admin` would
    // be a privilege escalation.
    assert_eq!(UserRole::parse("root"), None);
    assert_eq!(UserRole::parse("Admin"), None);
}

/// The system actor is the nil UUID with `role = Admin`. The household-identity
/// migration refuses to create a `users` row bearing the nil UUID precisely so
/// this stays distinguishable from a real person in audit logs.
#[test]
fn system_context_is_the_nil_uuid_admin() {
    let system = AuthContext::system();
    assert_eq!(system.user_id(), Some(Uuid::nil()));
    assert_eq!(system.role(), Some(UserRole::Admin));
}

/// A `member` is authenticated but is *not* an admin. This is the distinction
/// the whole epic turns on: several guards used to conflate "signed in" with
/// "may administer the box".
#[test]
fn a_member_is_authenticated_but_not_an_admin() {
    let member = AuthContext::user(AuthenticatedUser::from_validated_session(
        Uuid::new_v4(),
        UserRole::Member,
    ));
    assert!(member.is_authenticated());
    assert!(!member.is_admin());
}

#[test]
fn a_device_and_anonymous_are_never_admins() {
    let device = AuthContext::Device {
        mac: "AA:BB:CC:DD:EE:01".to_owned(),
    };
    assert!(device.is_authenticated());
    assert!(!device.is_admin());

    assert!(!AuthContext::Anonymous.is_authenticated());
    assert!(!AuthContext::Anonymous.is_admin());
}

/// Device affinity is attribution, never a credential: a `Device` context
/// exposes no `user_id`, so nothing downstream can promote it to its owner.
#[test]
fn a_device_context_exposes_no_user_id() {
    let device = AuthContext::Device {
        mac: "AA:BB:CC:DD:EE:01".to_owned(),
    };
    assert_eq!(device.user_id(), None);
    assert_eq!(device.role(), None);
}

#[test]
fn auth_context_is_admin() {
    let admin = AuthContext::system();
    assert!(admin.is_admin());

    let device = AuthContext::Device {
        mac: "AA:BB:CC:DD:EE:01".to_owned(),
    };
    assert!(!device.is_admin());

    assert!(!AuthContext::Anonymous.is_admin());
}

#[test]
fn auth_context_device_mac() {
    let admin = AuthContext::system();
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
