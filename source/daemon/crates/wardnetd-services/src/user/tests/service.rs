//! Tests for [`UserServiceImpl`] — the household directory, the last-admin
//! rule, self-vs-other authorization, and enrolment.

use std::sync::Arc;

use uuid::Uuid;
use wardnet_common::auth::UserRole;
use wardnet_test_support::principal;
use wardnetd_data::repository::session::SessionRepository;
use wardnetd_data::repository::user::UserRepository;
use wardnetd_data::repository::user_credential::UserCredentialRepository;
use wardnetd_data::repository::user_enrolment::{EnrolmentTokenRow, UserEnrolmentRepository};

use crate::auth::password::hash_token;
use crate::auth_context;
use crate::error::AppError;
use crate::tests::repo_mocks::{
    MockCredentialRepo, MockEnrolmentRepo, MockSessionRepo, MockUserRepo, StoredSession, user_row,
};
use crate::user::{NewUser, UserService, UserServiceImpl};

const ADMIN_ID: &str = "00000000-0000-0000-0000-000000000001";
const MEMBER_ID: &str = "00000000-0000-0000-0000-000000000002";
const OTHER_ADMIN_ID: &str = "00000000-0000-0000-0000-000000000003";

fn admin() -> Uuid {
    Uuid::parse_str(ADMIN_ID).unwrap()
}
fn member() -> Uuid {
    Uuid::parse_str(MEMBER_ID).unwrap()
}

struct Fixture {
    svc: UserServiceImpl,
    users: Arc<MockUserRepo>,
    credentials: Arc<MockCredentialRepo>,
    enrolments: Arc<MockEnrolmentRepo>,
    sessions: Arc<MockSessionRepo>,
}

/// A household with one enabled admin and one enabled member.
fn household() -> Fixture {
    build(MockUserRepo::with_rows(vec![
        user_row(ADMIN_ID, "admin", UserRole::Admin, true),
        user_row(MEMBER_ID, "kid", UserRole::Member, true),
    ]))
}

fn build(users: MockUserRepo) -> Fixture {
    let users = Arc::new(users);
    let credentials = Arc::new(MockCredentialRepo::joined_to(Arc::clone(&users)));
    let enrolments = Arc::new(MockEnrolmentRepo::empty());
    let sessions = Arc::new(MockSessionRepo::joined_to(Arc::clone(&users)));
    let svc = UserServiceImpl::new(
        Arc::clone(&users) as Arc<dyn UserRepository>,
        Arc::clone(&credentials) as Arc<dyn UserCredentialRepository>,
        Arc::clone(&enrolments) as Arc<dyn UserEnrolmentRepository>,
        Arc::clone(&sessions) as Arc<dyn SessionRepository>,
        // No provider is configured in these tests: the directory and enrolment
        // paths must not depend on federated sign-in being available.
        crate::user::OauthConfig {
            system_config: Arc::new(crate::tests::repo_mocks::MockSystemConfigRepo::empty()),
            secrets: Arc::new(crate::tests::repo_mocks::MockSecretStore::empty()),
        },
        Arc::new(crate::user::ReqwestOauthClient::new().unwrap()),
    );
    Fixture {
        svc,
        users,
        credentials,
        enrolments,
        sessions,
    }
}

/// Argon2id-hash a password with a fixed salt.
fn argon2_hash(password: &str) -> String {
    use argon2::PasswordHasher;
    let salt = argon2::password_hash::SaltString::from_b64("dGVzdHNhbHR2YWx1ZTEyMw").unwrap();
    argon2::Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .unwrap()
        .to_string()
}

fn live_session(user_id: &str, token: &str) -> StoredSession {
    let now = chrono::Utc::now();
    StoredSession {
        id: Uuid::new_v4().to_string(),
        user_id: user_id.to_owned(),
        token_hash: hash_token(token),
        created_at: now.to_rfc3339(),
        expires_at: (now + chrono::Duration::days(1)).to_rfc3339(),
        remember_me: true,
        device_id: None,
        user_agent: None,
        absolute_expires_at: (now + chrono::Duration::days(90)).to_rfc3339(),
    }
}

// -- guards ---------------------------------------------------------------

#[tokio::test]
async fn every_admin_only_method_refuses_a_member() {
    // The directory is an admin surface. A member must not be able to read who
    // else is in the household, create accounts, or change anybody's role.
    let f = household();
    let ctx = principal::member_context(member());

    macro_rules! refused {
        ($call:expr, $what:literal) => {{
            let result = auth_context::with_context(ctx.clone(), $call).await;
            assert!(
                matches!(result, Err(AppError::Forbidden(_))),
                concat!($what, " must refuse a member")
            );
        }};
    }

    refused!(f.svc.list_users(), "list_users");
    refused!(
        f.svc.create_user(NewUser {
            display_name: "new".to_owned(),
            email: None,
            role: UserRole::Member,
        }),
        "create_user"
    );
    refused!(f.svc.set_enabled(admin(), false), "set_enabled");
    refused!(f.svc.set_role(admin(), UserRole::Member), "set_role");
    refused!(f.svc.delete_user(admin()), "delete_user");
    refused!(f.svc.issue_enrolment(member()), "issue_enrolment");
    refused!(f.svc.list_enrolments(member()), "list_enrolments");
    refused!(f.svc.revoke_enrolment(Uuid::new_v4()), "revoke_enrolment");
    refused!(
        f.svc.cleanup_expired_enrolments(),
        "cleanup_expired_enrolments"
    );
}

#[tokio::test]
async fn every_method_refuses_an_anonymous_caller() {
    let f = household();
    assert!(matches!(
        f.svc.list_users().await,
        Err(AppError::Forbidden(_))
    ));
    assert!(matches!(
        f.svc.get_user(admin()).await,
        Err(AppError::Forbidden(_))
    ));
    assert!(matches!(
        f.svc.change_own_password("a", "bbbbbbbb").await,
        Err(AppError::Forbidden(_))
    ));
}

#[tokio::test]
async fn a_device_caller_is_refused_everywhere() {
    // A device holds no credential and owns no profile. `owner_user_id` is
    // attribution, never authority (ADR-0031 §4).
    let f = household();
    let ctx = principal::device_context("AA:BB:CC:DD:EE:01");

    let result = auth_context::with_context(ctx.clone(), f.svc.get_user(admin())).await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));

    let result = auth_context::with_context(ctx, f.svc.list_users()).await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}

// -- self vs other --------------------------------------------------------

#[tokio::test]
async fn a_member_can_read_their_own_profile_but_not_anothers() {
    let f = household();
    let ctx = principal::member_context(member());

    let own = auth_context::with_context(ctx.clone(), f.svc.get_user(member()))
        .await
        .expect("a member may read themselves");
    assert_eq!(own.display_name, "kid");

    let other = auth_context::with_context(ctx, f.svc.get_user(admin())).await;
    assert!(
        matches!(other, Err(AppError::Forbidden(_))),
        "a member must not read another household member's profile"
    );
}

#[tokio::test]
async fn a_member_can_edit_their_own_profile() {
    let f = household();
    let ctx = principal::member_context(member());

    let updated = auth_context::with_context(
        ctx,
        f.svc
            .update_profile(member(), "Kid Renamed", Some("kid@example.com")),
    )
    .await
    .unwrap();

    assert_eq!(updated.display_name, "Kid Renamed");
    assert_eq!(updated.email.as_deref(), Some("kid@example.com"));
}

#[tokio::test]
async fn a_member_cannot_edit_another_profile() {
    let f = household();
    let ctx = principal::member_context(member());

    let result =
        auth_context::with_context(ctx, f.svc.update_profile(admin(), "hijacked", None)).await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}

#[tokio::test]
async fn an_admin_can_edit_anybodys_profile() {
    let f = household();
    let ctx = principal::admin_context(admin());

    let updated = auth_context::with_context(
        ctx,
        f.svc.update_profile(member(), "Renamed By Admin", None),
    )
    .await
    .unwrap();
    assert_eq!(updated.display_name, "Renamed By Admin");
}

// -- create ---------------------------------------------------------------

#[tokio::test]
async fn create_user_makes_an_account_with_no_credential() {
    // The admin never learns a member's password (ADR-0031 §3), so creation and
    // credential-setting are separate steps by design.
    let f = household();
    let ctx = principal::admin_context(admin());

    let created = auth_context::with_context(
        ctx.clone(),
        f.svc.create_user(NewUser {
            display_name: "Teenager".to_owned(),
            email: Some("Teen@Example.COM".to_owned()),
            role: UserRole::Member,
        }),
    )
    .await
    .unwrap();

    assert_eq!(created.role, UserRole::Member);
    assert!(created.enabled);
    // Email is folded to lower case, matching the case-insensitive index.
    assert_eq!(created.email.as_deref(), Some("teen@example.com"));

    let credentials = auth_context::with_context(ctx, f.svc.list_credentials(created.id))
        .await
        .unwrap();
    assert!(
        credentials.is_empty(),
        "a newly created user must hold no credential until enrolled"
    );
}

#[tokio::test]
async fn create_user_validates_its_input() {
    let f = household();
    let ctx = principal::admin_context(admin());

    for (name, email) in [
        ("", None),
        ("   ", None),
        ("ok", Some("not-an-email")),
        ("ok", Some("missing@domain")),
    ] {
        let result = auth_context::with_context(
            ctx.clone(),
            f.svc.create_user(NewUser {
                display_name: name.to_owned(),
                email: email.map(str::to_owned),
                role: UserRole::Member,
            }),
        )
        .await;
        assert!(
            matches!(result, Err(AppError::BadRequest(_))),
            "expected {name:?}/{email:?} to be rejected"
        );
    }
}

#[tokio::test]
async fn an_empty_email_is_stored_as_none_not_as_an_empty_string() {
    // An empty string would occupy the unique email index with a value that
    // means "no email", so the second such user would collide.
    let f = household();
    let ctx = principal::admin_context(admin());

    let created = auth_context::with_context(
        ctx,
        f.svc.create_user(NewUser {
            display_name: "No Email".to_owned(),
            email: Some("   ".to_owned()),
            role: UserRole::Member,
        }),
    )
    .await
    .unwrap();

    assert_eq!(created.email, None);
}

// -- the last-admin rule --------------------------------------------------

#[tokio::test]
async fn the_last_enabled_admin_cannot_be_demoted_disabled_or_deleted() {
    // A box with no enabled admin cannot be administered at all, and the local
    // password is the break-glass path — there is no recovery short of editing
    // the database by hand.
    let f = build(MockUserRepo::with_rows(vec![
        user_row(ADMIN_ID, "admin", UserRole::Admin, true),
        user_row(MEMBER_ID, "kid", UserRole::Member, true),
    ]));
    let ctx = principal::admin_context(admin());

    assert!(matches!(
        auth_context::with_context(ctx.clone(), f.svc.set_role(admin(), UserRole::Member)).await,
        Err(AppError::Conflict(_))
    ));
    assert!(matches!(
        auth_context::with_context(ctx.clone(), f.svc.set_enabled(admin(), false)).await,
        Err(AppError::Conflict(_))
    ));
    assert!(matches!(
        auth_context::with_context(ctx, f.svc.delete_user(admin())).await,
        Err(AppError::Conflict(_))
    ));

    // And the admin is still there, still an enabled admin.
    let row = f.users.rows.lock().unwrap()[0].clone();
    assert_eq!(row.role, UserRole::Admin);
    assert!(row.enabled);
}

#[tokio::test]
async fn a_second_admin_makes_the_first_removable() {
    let f = build(MockUserRepo::with_rows(vec![
        user_row(ADMIN_ID, "admin", UserRole::Admin, true),
        user_row(OTHER_ADMIN_ID, "spouse", UserRole::Admin, true),
    ]));
    let ctx = principal::admin_context(admin());

    let demoted = auth_context::with_context(ctx, f.svc.set_role(admin(), UserRole::Member))
        .await
        .unwrap();
    assert_eq!(demoted.role, UserRole::Member);
}

#[tokio::test]
async fn a_disabled_second_admin_does_not_count_towards_the_last_admin_rule() {
    // A household whose only other admin is disabled is locked out just as
    // thoroughly as one with no second admin at all.
    let f = build(MockUserRepo::with_rows(vec![
        user_row(ADMIN_ID, "admin", UserRole::Admin, true),
        user_row(OTHER_ADMIN_ID, "spouse", UserRole::Admin, false),
    ]));
    let ctx = principal::admin_context(admin());

    let result = auth_context::with_context(ctx, f.svc.delete_user(admin())).await;
    assert!(matches!(result, Err(AppError::Conflict(_))));
}

#[tokio::test]
async fn a_member_can_always_be_removed() {
    let f = household();
    let ctx = principal::admin_context(admin());

    auth_context::with_context(ctx, f.svc.delete_user(member()))
        .await
        .unwrap();
    assert_eq!(f.users.count(), 1);
}

// -- disabling revokes ----------------------------------------------------

#[tokio::test]
async fn disabling_a_user_deletes_their_sessions() {
    // Disabling is the revocation primitive; leaving live sessions standing
    // would make it a revocation that does not revoke.
    let f = household();
    f.sessions
        .rows
        .lock()
        .unwrap()
        .push(live_session(MEMBER_ID, "kid-token"));
    f.sessions
        .rows
        .lock()
        .unwrap()
        .push(live_session(ADMIN_ID, "admin-token"));
    let ctx = principal::admin_context(admin());

    auth_context::with_context(ctx, f.svc.set_enabled(member(), false))
        .await
        .unwrap();

    let remaining: Vec<String> = f
        .sessions
        .rows
        .lock()
        .unwrap()
        .iter()
        .map(|r| r.user_id.clone())
        .collect();
    assert_eq!(
        remaining,
        vec![ADMIN_ID.to_owned()],
        "only the disabled user's sessions should be gone"
    );
}

// -- password change ------------------------------------------------------

#[tokio::test]
async fn change_own_password_verifies_the_current_one() {
    let f = household();
    f.credentials.rows.lock().unwrap().push(
        wardnetd_data::repository::user_credential::CredentialRow {
            id: "cred-1".to_owned(),
            user_id: MEMBER_ID.to_owned(),
            kind: wardnetd_data::repository::user_credential::CredentialKind::Password,
            subject: "kid@example.com".to_owned(),
            secret: Some(argon2_hash("old-password")),
            label: None,
            metadata: "{}".to_owned(),
            created_at: "2026-01-01T00:00:00+00:00".to_owned(),
            last_used_at: None,
        },
    );
    let ctx = principal::member_context(member());

    let wrong = auth_context::with_context(
        ctx.clone(),
        f.svc
            .change_own_password("not-the-password", "new-password"),
    )
    .await;
    assert!(matches!(wrong, Err(AppError::Unauthorized(_))));

    auth_context::with_context(
        ctx,
        f.svc.change_own_password("old-password", "new-password"),
    )
    .await
    .expect("the correct current password must be accepted");
}

#[tokio::test]
async fn change_own_password_revokes_every_session() {
    // A password change is usually a response to "somebody may know my
    // password". Leaving the attacker's session alive would make it cosmetic.
    let f = household();
    f.credentials.rows.lock().unwrap().push(
        wardnetd_data::repository::user_credential::CredentialRow {
            id: "cred-1".to_owned(),
            user_id: MEMBER_ID.to_owned(),
            kind: wardnetd_data::repository::user_credential::CredentialKind::Password,
            subject: "kid@example.com".to_owned(),
            secret: Some(argon2_hash("old-password")),
            label: None,
            metadata: "{}".to_owned(),
            created_at: "2026-01-01T00:00:00+00:00".to_owned(),
            last_used_at: None,
        },
    );
    f.sessions
        .rows
        .lock()
        .unwrap()
        .push(live_session(MEMBER_ID, "kid-token"));
    let ctx = principal::member_context(member());

    auth_context::with_context(
        ctx,
        f.svc.change_own_password("old-password", "new-password"),
    )
    .await
    .unwrap();

    assert_eq!(f.sessions.count(), 0);
}

#[tokio::test]
async fn change_own_password_enforces_the_length_policy() {
    let f = household();
    let ctx = principal::member_context(member());

    let result = auth_context::with_context(ctx, f.svc.change_own_password("old", "short")).await;
    assert!(matches!(result, Err(AppError::BadRequest(_))));
}

#[tokio::test]
async fn change_own_password_reports_a_conflict_when_there_is_no_password_yet() {
    // An unenrolled account has nothing to change; the fix is to redeem an
    // invitation, and the error says so.
    let f = household();
    let ctx = principal::member_context(member());

    let result = auth_context::with_context(
        ctx,
        f.svc.change_own_password("anything", "a-good-password"),
    )
    .await;
    assert!(matches!(result, Err(AppError::Conflict(_))));
}

// -- enrolment ------------------------------------------------------------

#[tokio::test]
async fn issue_enrolment_returns_a_token_and_stores_only_its_hash() {
    // A readable token in a database backup would be a standing invitation to
    // join the household, and backups leave the box.
    let f = household();
    let ctx = principal::admin_context(admin());

    let invite = auth_context::with_context(ctx, f.svc.issue_enrolment(member()))
        .await
        .unwrap();

    assert!(!invite.token.is_empty());
    assert_eq!(invite.user_id, member());

    let stored = f.enrolments.rows.lock().unwrap()[0].clone();
    assert_eq!(stored.token_hash, hash_token(&invite.token));
    assert_ne!(
        stored.token_hash, invite.token,
        "the raw token must never be persisted"
    );
    assert!(stored.used_at.is_none());
}

#[tokio::test]
async fn issue_enrolment_refuses_a_disabled_user() {
    let f = build(MockUserRepo::with_rows(vec![
        user_row(ADMIN_ID, "admin", UserRole::Admin, true),
        user_row(MEMBER_ID, "kid", UserRole::Member, false),
    ]));
    let ctx = principal::admin_context(admin());

    let result = auth_context::with_context(ctx, f.svc.issue_enrolment(member())).await;
    assert!(matches!(result, Err(AppError::Conflict(_))));
}

#[tokio::test]
async fn redeem_enrolment_sets_the_first_password_and_spends_the_token() {
    let f = household();
    // Give the member an email so the credential gets a typeable subject.
    let admin_ctx = principal::admin_context(admin());
    auth_context::with_context(
        admin_ctx.clone(),
        f.svc
            .update_profile(member(), "kid", Some("kid@example.com")),
    )
    .await
    .unwrap();
    let invite = auth_context::with_context(admin_ctx, f.svc.issue_enrolment(member()))
        .await
        .unwrap();

    // Redeemed with no auth context at all — the token is the authorization.
    let profile = f
        .svc
        .redeem_enrolment(&invite.token, "a-good-password")
        .await
        .expect("a valid invitation must be redeemable without a session");
    assert_eq!(profile.id, member());

    let credential = f
        .credentials
        .find_password(MEMBER_ID)
        .await
        .unwrap()
        .expect("redemption must create a password credential");
    assert_eq!(credential.subject, "kid@example.com");
    assert!(
        credential
            .secret
            .as_deref()
            .is_some_and(|s| s.starts_with("$argon2")),
        "the password must be stored as an Argon2 hash"
    );

    let stored_id = f.enrolments.rows.lock().unwrap()[0].id.clone();
    assert!(
        f.enrolments.used_at(&stored_id).is_some(),
        "the token must be marked spent"
    );
}

#[tokio::test]
async fn an_enrolment_token_is_single_use() {
    let f = household();
    let ctx = principal::admin_context(admin());
    let invite = auth_context::with_context(ctx, f.svc.issue_enrolment(member()))
        .await
        .unwrap();

    f.svc
        .redeem_enrolment(&invite.token, "a-good-password")
        .await
        .unwrap();

    let replay = f
        .svc
        .redeem_enrolment(&invite.token, "another-password")
        .await;
    assert!(
        matches!(replay, Err(AppError::Unauthorized(_))),
        "a spent token must not be redeemable again"
    );
}

#[tokio::test]
async fn an_expired_enrolment_token_is_refused() {
    let f = household();
    let token = "expired-token";
    f.enrolments.push(EnrolmentTokenRow {
        id: Uuid::new_v4().to_string(),
        user_id: MEMBER_ID.to_owned(),
        token_hash: hash_token(token),
        created_at: "2026-01-01T00:00:00+00:00".to_owned(),
        expires_at: "2026-01-02T00:00:00+00:00".to_owned(),
        used_at: None,
    });

    let result = f.svc.redeem_enrolment(token, "a-good-password").await;
    assert!(matches!(result, Err(AppError::Unauthorized(_))));
}

#[tokio::test]
async fn an_unknown_enrolment_token_is_refused_indistinguishably() {
    // Unknown, expired and spent all report the same thing: telling a caller
    // which it was turns this into an oracle for guessing tokens.
    let f = household();
    let result = f
        .svc
        .redeem_enrolment("never-issued", "a-good-password")
        .await;
    match result {
        Err(AppError::Unauthorized(message)) => {
            assert_eq!(message, "that invitation is not valid");
        }
        other => panic!("expected Unauthorized, got {other:?}"),
    }
}

#[tokio::test]
async fn redeem_enrolment_enforces_the_password_policy_before_spending_the_token() {
    // A rejected password must leave the invitation usable — otherwise a typo
    // burns it and the admin has to issue another.
    let f = household();
    let ctx = principal::admin_context(admin());
    let invite = auth_context::with_context(ctx, f.svc.issue_enrolment(member()))
        .await
        .unwrap();

    let result = f.svc.redeem_enrolment(&invite.token, "short").await;
    assert!(matches!(result, Err(AppError::BadRequest(_))));

    let stored_id = f.enrolments.rows.lock().unwrap()[0].id.clone();
    assert!(
        f.enrolments.used_at(&stored_id).is_none(),
        "a rejected password must not spend the invitation"
    );
}

#[tokio::test]
async fn redeem_enrolment_refuses_a_disabled_user() {
    let f = household();
    let ctx = principal::admin_context(admin());
    let invite = auth_context::with_context(ctx.clone(), f.svc.issue_enrolment(member()))
        .await
        .unwrap();

    // The admin disables the account after issuing the invitation.
    auth_context::with_context(ctx, f.svc.set_enabled(member(), false))
        .await
        .unwrap();

    let result = f
        .svc
        .redeem_enrolment(&invite.token, "a-good-password")
        .await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}

#[tokio::test]
async fn revoke_enrolment_removes_an_outstanding_invitation() {
    let f = household();
    let ctx = principal::admin_context(admin());
    let invite = auth_context::with_context(ctx.clone(), f.svc.issue_enrolment(member()))
        .await
        .unwrap();
    let stored_id = Uuid::parse_str(&f.enrolments.rows.lock().unwrap()[0].id).unwrap();

    auth_context::with_context(ctx, f.svc.revoke_enrolment(stored_id))
        .await
        .unwrap();

    assert_eq!(f.enrolments.count(), 0);
    let result = f
        .svc
        .redeem_enrolment(&invite.token, "a-good-password")
        .await;
    assert!(matches!(result, Err(AppError::Unauthorized(_))));
}

#[tokio::test]
async fn revoke_enrolment_reports_not_found_for_an_unknown_id() {
    let f = household();
    let ctx = principal::admin_context(admin());

    let result = auth_context::with_context(ctx, f.svc.revoke_enrolment(Uuid::new_v4())).await;
    assert!(matches!(result, Err(AppError::NotFound(_))));
}

#[tokio::test]
async fn cleanup_expired_enrolments_removes_only_expired_rows() {
    let f = household();
    let now = chrono::Utc::now();
    f.enrolments.push(EnrolmentTokenRow {
        id: "expired".to_owned(),
        user_id: MEMBER_ID.to_owned(),
        token_hash: hash_token("a"),
        created_at: "2026-01-01T00:00:00+00:00".to_owned(),
        expires_at: (now - chrono::Duration::hours(1)).to_rfc3339(),
        used_at: None,
    });
    f.enrolments.push(EnrolmentTokenRow {
        id: "live".to_owned(),
        user_id: MEMBER_ID.to_owned(),
        token_hash: hash_token("b"),
        created_at: "2026-01-01T00:00:00+00:00".to_owned(),
        expires_at: (now + chrono::Duration::hours(1)).to_rfc3339(),
        used_at: None,
    });
    let ctx = principal::admin_context(admin());

    let removed = auth_context::with_context(ctx, f.svc.cleanup_expired_enrolments())
        .await
        .unwrap();

    assert_eq!(removed, 1);
    assert_eq!(f.enrolments.count(), 1);
    assert_eq!(f.enrolments.rows.lock().unwrap()[0].id, "live");
}

// -- reads ----------------------------------------------------------------

#[tokio::test]
async fn list_users_returns_the_whole_household_to_an_admin() {
    let f = household();
    let ctx = principal::admin_context(admin());

    let users = auth_context::with_context(ctx, f.svc.list_users())
        .await
        .unwrap();
    assert_eq!(users.len(), 2);
}

#[tokio::test]
async fn get_user_reports_not_found_for_an_unknown_id() {
    let f = household();
    let ctx = principal::admin_context(admin());

    let result = auth_context::with_context(ctx, f.svc.get_user(Uuid::new_v4())).await;
    assert!(matches!(result, Err(AppError::NotFound(_))));
}

// -- regressions from the high-effort code review --------------------------

#[tokio::test]
async fn an_enrolment_token_cannot_overwrite_an_existing_password() {
    // The takeover path: an admin issues a second invitation against an account
    // that already has a password, redeems it with a password of their choosing,
    // and can then sign in as that person. That is exactly what ADR-0031 §3
    // rules out, and why `change_own_password` has no admin override.
    let f = household();
    let ctx = principal::admin_context(admin());

    // Ann already has a password of her own.
    f.credentials
        .set_password(
            "cred-pw",
            MEMBER_ID,
            "kid@example.com",
            "$argon2-ann",
            "now",
        )
        .await
        .unwrap();

    // Issuing is refused up front, so no unusable token is even minted.
    let issued = auth_context::with_context(ctx.clone(), f.svc.issue_enrolment(member())).await;
    assert!(
        matches!(issued, Err(AppError::Conflict(_))),
        "issuing against an account with a password must be refused: {issued:?}"
    );
    assert_eq!(f.enrolments.count(), 0);

    // And redemption refuses even for a token minted before the password existed.
    f.enrolments.push(EnrolmentTokenRow {
        id: "pre-existing".to_owned(),
        user_id: MEMBER_ID.to_owned(),
        token_hash: hash_token("smuggled"),
        created_at: "2026-01-01T00:00:00+00:00".to_owned(),
        expires_at: (chrono::Utc::now() + chrono::Duration::hours(1)).to_rfc3339(),
        used_at: None,
    });
    let redeemed = f.svc.redeem_enrolment("smuggled", "attacker-chosen").await;
    assert!(
        matches!(redeemed, Err(AppError::Conflict(_))),
        "redeeming against an account with a password must be refused: {redeemed:?}"
    );

    // Ann's hash is untouched, so her password still works and the admin's does
    // not become hers.
    let credential = f
        .credentials
        .find_password(MEMBER_ID)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(credential.secret.as_deref(), Some("$argon2-ann"));
}

#[tokio::test]
async fn a_failed_redemption_does_not_burn_the_invitation() {
    // The token is claimed only after every check passes, so a refusal leaves it
    // redeemable. Otherwise a typo'd password or a mid-flight disable costs the
    // household a fresh invitation.
    let f = household();
    let ctx = principal::admin_context(admin());
    let invite = auth_context::with_context(ctx.clone(), f.svc.issue_enrolment(member()))
        .await
        .unwrap();
    let stored_id = f.enrolments.rows.lock().unwrap()[0].id.clone();

    // A rejected password.
    assert!(matches!(
        f.svc.redeem_enrolment(&invite.token, "short").await,
        Err(AppError::BadRequest(_))
    ));
    assert!(f.enrolments.used_at(&stored_id).is_none());

    // A disabled account.
    auth_context::with_context(ctx, f.svc.set_enabled(member(), false))
        .await
        .unwrap();
    assert!(matches!(
        f.svc
            .redeem_enrolment(&invite.token, "a-good-password")
            .await,
        Err(AppError::Forbidden(_))
    ));
    assert!(
        f.enrolments.used_at(&stored_id).is_none(),
        "a refused redemption must leave the invitation spendable"
    );
}

#[tokio::test]
async fn changing_the_email_moves_the_password_login_subject() {
    // The email *is* the password login identifier. If the two diverge, the
    // profile shows an address that cannot sign in, and the freed address
    // becomes a landmine for the next user given it.
    let f = household();
    f.credentials
        .set_password(
            "cred-pw",
            MEMBER_ID,
            "kid@example.com",
            "$argon2-ann",
            "now",
        )
        .await
        .unwrap();
    let ctx = principal::admin_context(admin());

    auth_context::with_context(
        ctx,
        f.svc
            .update_profile(member(), "Kid", Some("A.Jones@Example.com")),
    )
    .await
    .unwrap();

    let credential = f
        .credentials
        .find_password(MEMBER_ID)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        credential.subject, "a.jones@example.com",
        "the login subject must follow the email, lowercased"
    );
    assert_eq!(
        credential.secret.as_deref(),
        Some("$argon2-ann"),
        "the password itself must not change because the address did"
    );
}

#[tokio::test]
async fn a_profile_update_that_does_not_touch_the_email_leaves_the_subject_alone() {
    let f = household();
    f.credentials
        .set_password(
            "cred-pw",
            MEMBER_ID,
            "kid@example.com",
            "$argon2-ann",
            "now",
        )
        .await
        .unwrap();
    let ctx = principal::admin_context(admin());

    auth_context::with_context(
        ctx,
        f.svc
            .update_profile(member(), "Renamed Only", Some("kid@example.com")),
    )
    .await
    .unwrap();

    let credential = f
        .credentials
        .find_password(MEMBER_ID)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(credential.subject, "kid@example.com");
    assert_eq!(credential.id, "cred-pw", "no needless credential rewrite");
}
