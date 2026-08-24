//! Value-type tests for the access-request vocabulary (issue #919).
//!
//! These types are the wire contract between the daemon, both SDKs and three
//! frontends, and the database stores their `as_str` form — so a rename or a
//! reordered match arm is a migration-shaped bug, not a refactor.

use crate::access_request::{
    AccessRequestKind, AccessRequestStatus, ApprovalParams, CreateAccessRequestRequest,
    DecideAccessRequestRequest,
};

const KINDS: [AccessRequestKind; 3] = [
    AccessRequestKind::Block,
    AccessRequestKind::Allow,
    AccessRequestKind::PrivateDns,
];

const STATUSES: [AccessRequestStatus; 3] = [
    AccessRequestStatus::Pending,
    AccessRequestStatus::Approved,
    AccessRequestStatus::Rejected,
];

#[test]
fn kind_strings_are_the_stored_values() {
    // Pinned literally: these are what sits in `device_access_requests.kind`
    // on every deployed box, and the migration copied them across verbatim.
    assert_eq!(AccessRequestKind::Block.as_str(), "block");
    assert_eq!(AccessRequestKind::Allow.as_str(), "allow");
    assert_eq!(AccessRequestKind::PrivateDns.as_str(), "private_dns");
}

#[test]
fn kind_round_trips_through_its_stored_form() {
    for kind in KINDS {
        assert_eq!(AccessRequestKind::parse(kind.as_str()), kind);
    }
}

#[test]
fn an_unknown_kind_falls_back_to_block() {
    // Defensive rather than meaningful: the database `CHECK` constrains the
    // column, so this only fires on a row written by a future version.
    assert_eq!(
        AccessRequestKind::parse("teleport"),
        AccessRequestKind::Block
    );
    assert_eq!(AccessRequestKind::parse(""), AccessRequestKind::Block);
}

#[test]
fn only_domain_naming_kinds_require_a_domain() {
    // The service and the database `CHECK` both read this pairing; if they
    // ever disagree, an insert passes validation and then violates the row
    // constraint.
    assert!(AccessRequestKind::Block.requires_domain());
    assert!(AccessRequestKind::Allow.requires_domain());
    assert!(!AccessRequestKind::PrivateDns.requires_domain());
}

#[test]
fn status_strings_are_the_stored_values() {
    assert_eq!(AccessRequestStatus::Pending.as_str(), "pending");
    assert_eq!(AccessRequestStatus::Approved.as_str(), "approved");
    assert_eq!(AccessRequestStatus::Rejected.as_str(), "rejected");
}

#[test]
fn status_round_trips_through_its_stored_form() {
    for status in STATUSES {
        assert_eq!(AccessRequestStatus::parse(status.as_str()), status);
    }
}

#[test]
fn an_unknown_status_falls_back_to_pending() {
    // Pending is the safe fallback: it leaves the request in the inbox rather
    // than silently reporting a decision nobody made.
    assert_eq!(
        AccessRequestStatus::parse("escalated"),
        AccessRequestStatus::Pending
    );
}

#[test]
fn only_terminal_statuses_are_decisions() {
    assert!(!AccessRequestStatus::Pending.is_decision());
    assert!(AccessRequestStatus::Approved.is_decision());
    assert!(AccessRequestStatus::Rejected.is_decision());
}

#[test]
fn kind_serializes_snake_case_for_the_wire() {
    // The SDKs and PWAs match on these strings.
    assert_eq!(
        serde_json::to_string(&AccessRequestKind::PrivateDns).unwrap(),
        "\"private_dns\""
    );
    assert_eq!(
        serde_json::from_str::<AccessRequestKind>("\"allow\"").unwrap(),
        AccessRequestKind::Allow
    );
}

#[test]
fn a_create_request_may_omit_the_domain() {
    // A `private_dns` request names none, so the field has to be optional on
    // the wire rather than an empty string.
    let body: CreateAccessRequestRequest =
        serde_json::from_str(r#"{"kind":"private_dns"}"#).unwrap();
    assert_eq!(body.kind, AccessRequestKind::PrivateDns);
    assert!(body.domain.is_none());
    assert!(body.reason.is_none());
}

#[test]
fn a_decision_may_omit_approval_params() {
    // Rejecting takes none, and a kind needing no admin input sends none.
    let body: DecideAccessRequestRequest =
        serde_json::from_str(r#"{"status":"rejected"}"#).unwrap();
    assert_eq!(body.status, AccessRequestStatus::Rejected);
    assert!(body.approval.is_none());
}

#[test]
fn approval_params_are_tagged_by_kind() {
    // Tagged so a second kind's params can be added without the shape being
    // ambiguous — "absent means Private DNS" would not survive that.
    let body: DecideAccessRequestRequest =
        serde_json::from_str(r#"{"status":"approved","approval":{"kind":"private_dns"}}"#).unwrap();
    assert!(matches!(body.approval, Some(ApprovalParams::PrivateDns)));
    assert_eq!(
        serde_json::to_string(&ApprovalParams::PrivateDns).unwrap(),
        r#"{"kind":"private_dns"}"#
    );
}
