//! Passkey tests. No network and no real authenticator: `webauthn-rs` ships a
//! software authenticator (`WebauthnAuthenticator` + `SoftPasskey`) that does
//! genuine COSE key generation and signing, so these exercise the real
//! verification path rather than a stub of it.

use std::sync::Arc;

use uuid::Uuid;
use wardnet_common::auth::UserRole;
use wardnet_test_support::principal;
use wardnetd_data::repository::session::SessionRepository;
use wardnetd_data::repository::user::UserRepository;
use wardnetd_data::repository::user_credential::{CredentialKind, UserCredentialRepository};
use wardnetd_data::repository::user_enrolment::UserEnrolmentRepository;
use webauthn_authenticator_rs::WebauthnAuthenticator;
use webauthn_authenticator_rs::softpasskey::SoftPasskey;

use crate::auth_context;
use crate::ddns::{KEY_PROVIDER, KEY_SUBDOMAIN, PROVIDER_WARDNET};
use crate::error::AppError;
use crate::tests::repo_mocks::{
    MockCredentialRepo, MockEnrolmentRepo, MockSecretStore, MockSessionRepo, MockSystemConfigRepo,
    MockUserRepo, user_row,
};
use crate::user::oauth::{OauthConfig, ReqwestOauthClient};
use crate::user::passkey::KEY_PASSKEY_RP_ID;
use crate::user::{UserService, UserServiceImpl};

const ADMIN_ID: &str = "00000000-0000-0000-0000-000000000001";
const MEMBER_ID: &str = "00000000-0000-0000-0000-000000000002";
const FQDN: &str = "happy-einstein.wardnet.app";

fn admin() -> Uuid {
    Uuid::parse_str(ADMIN_ID).unwrap()
}
fn member() -> Uuid {
    Uuid::parse_str(MEMBER_ID).unwrap()
}

struct Fixture {
    svc: UserServiceImpl,
    credentials: Arc<MockCredentialRepo>,
    config: Arc<MockSystemConfigRepo>,
}

/// A household with a canonical hostname, so passkeys are possible.
fn fixture(with_fqdn: bool) -> Fixture {
    let users = Arc::new(MockUserRepo::with_rows(vec![
        user_row(ADMIN_ID, "admin", UserRole::Admin, true),
        user_row(MEMBER_ID, "kid", UserRole::Member, true),
    ]));
    let credentials = Arc::new(MockCredentialRepo::joined_to(Arc::clone(&users)));
    let config = Arc::new(MockSystemConfigRepo::empty());

    if with_fqdn {
        let mut values = config.values.lock().unwrap();
        values.insert(KEY_PROVIDER.to_owned(), PROVIDER_WARDNET.to_owned());
        values.insert(KEY_SUBDOMAIN.to_owned(), FQDN.to_owned());
    }

    let svc = UserServiceImpl::new(
        Arc::clone(&users) as Arc<dyn UserRepository>,
        Arc::clone(&credentials) as Arc<dyn UserCredentialRepository>,
        Arc::new(MockEnrolmentRepo::empty()) as Arc<dyn UserEnrolmentRepository>,
        Arc::new(MockSessionRepo::joined_to(Arc::clone(&users))) as Arc<dyn SessionRepository>,
        OauthConfig {
            system_config: Arc::clone(&config)
                as Arc<dyn wardnetd_data::repository::SystemConfigRepository>,
            secrets: Arc::new(MockSecretStore::empty())
                as Arc<dyn wardnetd_data::secret_store::SecretStore>,
        },
        Arc::new(ReqwestOauthClient::new().unwrap()),
    );

    Fixture {
        svc,
        credentials,
        config,
    }
}

/// Register a passkey for `who` through the software authenticator.
///
/// Returns the authenticator, so a later sign-in can use the same credential —
/// which is the whole point: a passkey is only useful if the same authenticator
/// can assert it afterwards.
async fn register(
    f: &Fixture,
    who: Uuid,
    label: &str,
) -> Result<WebauthnAuthenticator<SoftPasskey>, AppError> {
    let ctx = principal::admin_context(who);
    let challenge =
        auth_context::with_context(ctx.clone(), f.svc.start_passkey_registration(FQDN)).await?;

    let challenge_id = challenge
        .get("challengeId")
        .and_then(|v| v.as_str())
        .expect("the daemon must hand back an opaque challenge id")
        .to_owned();

    let options: webauthn_rs::prelude::CreationChallengeResponse =
        serde_json::from_value(challenge).unwrap();

    let mut authenticator = WebauthnAuthenticator::new(SoftPasskey::new(true));
    let credential = authenticator
        .do_registration(
            url::Url::parse(&format!("https://{FQDN}")).unwrap(),
            options,
        )
        .expect("the soft authenticator should complete registration");

    let mut value = serde_json::to_value(&credential).unwrap();
    value
        .as_object_mut()
        .unwrap()
        .insert("challengeId".to_owned(), challenge_id.into());

    auth_context::with_context(
        ctx,
        f.svc.finish_passkey_registration(FQDN, Some(label), value),
    )
    .await?;

    Ok(authenticator)
}

/// Drive a sign-in through the soft authenticator.
///
/// The daemon's request is **discoverable** — no allow-list — which is what makes
/// production sign-in a single button with no username. `SoftPasskey` cannot
/// honour that: it picks which credential to sign with from
/// `allow_credentials`, so with an empty list it has nothing to select and
/// fails. So the test tells the authenticator which credential to use by
/// injecting the allow-list into the options *it* receives.
///
/// This does not weaken what is under test. The daemon verifies the challenge,
/// the origin, the signature and the counter against its own stored state; it
/// never sees or trusts the allow-list the browser was handed. The only thing
/// stubbed out is the authenticator's credential *selection*, which is the
/// platform's job in production.
async fn sign_in(
    f: &Fixture,
    authenticator: &mut WebauthnAuthenticator<SoftPasskey>,
    credential_id: &str,
    host: &str,
) -> Result<(crate::user::UserProfile, UserRole), AppError> {
    let challenge = f.svc.start_passkey_authentication(host).await?;
    let challenge_id = challenge
        .get("challengeId")
        .and_then(|v| v.as_str())
        .expect("the daemon must hand back an opaque challenge id")
        .to_owned();

    let mut options = challenge;
    options.as_object_mut().unwrap().remove("challengeId");
    // Point the soft authenticator at the credential it registered.
    if let Some(public_key) = options.get_mut("publicKey").and_then(|k| k.as_object_mut()) {
        public_key.insert(
            "allowCredentials".to_owned(),
            serde_json::json!([{ "type": "public-key", "id": credential_id }]),
        );
    }

    let options: webauthn_rs::prelude::RequestChallengeResponse =
        serde_json::from_value(options).unwrap();
    let assertion = authenticator
        .do_authentication(
            url::Url::parse(&format!("https://{host}")).unwrap(),
            options,
        )
        .expect("the soft authenticator should assert the credential it registered");

    let mut value = serde_json::to_value(&assertion).unwrap();
    value
        .as_object_mut()
        .unwrap()
        .insert("challengeId".to_owned(), challenge_id.into());

    f.svc.finish_passkey_authentication(host, value).await
}

/// The stored base64url credential id of the household's only passkey.
fn only_credential_subject(f: &Fixture) -> String {
    let rows = f.credentials.rows.lock().unwrap();
    let passkey = rows
        .iter()
        .find(|r| r.kind == CredentialKind::Passkey)
        .expect("a passkey must be registered");
    passkey.subject.clone()
}

// -- preconditions --------------------------------------------------------

#[tokio::test]
async fn passkeys_are_unavailable_without_a_canonical_hostname() {
    // WebAuthn needs a real domain. Registering one here would produce a
    // credential nobody could ever use, so this is a 412 rather than a partial
    // success — and the message tells the operator to use a password meanwhile.
    let f = fixture(false);
    let ctx = principal::admin_context(admin());

    let result =
        auth_context::with_context(ctx, f.svc.start_passkey_registration("192.168.1.2:7411")).await;

    assert!(
        matches!(result, Err(AppError::PreconditionFailed(_))),
        "expected 412, got {result:?}"
    );
}

#[tokio::test]
async fn a_request_to_the_plain_http_surface_is_refused_once_an_rp_id_is_pinned() {
    // The honest consequence of RP-ID pinning: `:7411` and bare LAN IPs cannot
    // do passkeys, which is precisely why the local password is never removable.
    let f = fixture(true);
    let ctx = principal::admin_context(admin());
    register(&f, admin(), "laptop").await.unwrap();

    let result =
        auth_context::with_context(ctx, f.svc.start_passkey_registration("192.168.1.2:7411")).await;

    match result {
        Err(AppError::PreconditionFailed(message)) => {
            assert!(
                message.contains(FQDN),
                "the refusal should name the hostname to use: {message}"
            );
        }
        other => panic!("expected 412, got {other:?}"),
    }
}

#[tokio::test]
async fn the_rp_id_is_pinned_at_first_registration() {
    let f = fixture(true);
    assert!(f.config.read(KEY_PASSKEY_RP_ID).is_none());

    register(&f, admin(), "laptop").await.unwrap();

    assert_eq!(
        f.config.read(KEY_PASSKEY_RP_ID).as_deref(),
        Some(FQDN),
        "the first registration pins the relying-party id"
    );
}

#[tokio::test]
async fn a_pinned_rp_id_is_not_silently_replaced_when_the_hostname_changes() {
    // Re-pinning would invalidate every existing passkey with no explanation.
    // The box must fail loudly and wait for an explicit admin reset.
    let f = fixture(true);
    register(&f, admin(), "laptop").await.unwrap();

    // The household moves to a new hostname.
    f.config
        .values
        .lock()
        .unwrap()
        .insert(KEY_SUBDOMAIN.to_owned(), "moved.example.com".to_owned());

    let ctx = principal::admin_context(admin());
    let result =
        auth_context::with_context(ctx, f.svc.start_passkey_registration("moved.example.com"))
            .await;

    assert!(matches!(result, Err(AppError::PreconditionFailed(_))));
    assert_eq!(
        f.config.read(KEY_PASSKEY_RP_ID).as_deref(),
        Some(FQDN),
        "the pin must not move on its own"
    );
}

#[tokio::test]
async fn a_subdomain_of_the_pinned_rp_id_is_accepted() {
    // `allow_subdomains(true)` is what lets one passkey cover the published-app
    // subdomains #1149 adds, instead of one registration per app.
    let f = fixture(true);
    register(&f, admin(), "laptop").await.unwrap();
    let ctx = principal::admin_context(admin());

    let result = auth_context::with_context(
        ctx,
        f.svc.start_passkey_registration(&format!("app.{FQDN}")),
    )
    .await;

    assert!(result.is_ok(), "a subdomain should be allowed: {result:?}");
}

// -- registration ---------------------------------------------------------

#[tokio::test]
async fn registering_a_passkey_stores_the_credential_without_a_secret() {
    let f = fixture(true);
    register(&f, member(), "Pixel 8").await.unwrap();

    let rows = f.credentials.rows.lock().unwrap().clone();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].kind, CredentialKind::Passkey);
    assert_eq!(rows[0].label.as_deref(), Some("Pixel 8"));
    assert!(
        rows[0].secret.is_none(),
        "a passkey's public key belongs in metadata, not in the secret column"
    );
    assert!(
        !rows[0].subject.is_empty(),
        "the credential id is the login key for a discoverable sign-in"
    );
}

#[tokio::test]
async fn registration_requires_authentication() {
    let f = fixture(true);
    let result = f.svc.start_passkey_registration(FQDN).await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}

#[tokio::test]
async fn a_registration_challenge_is_single_use() {
    let f = fixture(true);
    let ctx = principal::admin_context(admin());
    let challenge = auth_context::with_context(ctx.clone(), f.svc.start_passkey_registration(FQDN))
        .await
        .unwrap();
    let challenge_id = challenge
        .get("challengeId")
        .and_then(|v| v.as_str())
        .unwrap()
        .to_owned();
    let options: webauthn_rs::prelude::CreationChallengeResponse =
        serde_json::from_value(challenge).unwrap();

    let mut authenticator = WebauthnAuthenticator::new(SoftPasskey::new(true));
    let credential = authenticator
        .do_registration(
            url::Url::parse(&format!("https://{FQDN}")).unwrap(),
            options,
        )
        .unwrap();
    let mut value = serde_json::to_value(&credential).unwrap();
    value
        .as_object_mut()
        .unwrap()
        .insert("challengeId".to_owned(), challenge_id.clone().into());

    auth_context::with_context(
        ctx.clone(),
        f.svc.finish_passkey_registration(FQDN, None, value.clone()),
    )
    .await
    .unwrap();

    let replay =
        auth_context::with_context(ctx, f.svc.finish_passkey_registration(FQDN, None, value)).await;
    assert!(
        matches!(replay, Err(AppError::Unauthorized(_))),
        "a replayed challenge must be refused"
    );
}

#[tokio::test]
async fn a_registration_cannot_be_completed_by_a_different_user() {
    // Otherwise a caller could finish somebody else's ceremony and attach the
    // credential to their own account.
    let f = fixture(true);
    let starter = principal::admin_context(admin());
    let challenge = auth_context::with_context(starter, f.svc.start_passkey_registration(FQDN))
        .await
        .unwrap();
    let challenge_id = challenge
        .get("challengeId")
        .and_then(|v| v.as_str())
        .unwrap()
        .to_owned();
    let options: webauthn_rs::prelude::CreationChallengeResponse =
        serde_json::from_value(challenge).unwrap();

    let mut authenticator = WebauthnAuthenticator::new(SoftPasskey::new(true));
    let credential = authenticator
        .do_registration(
            url::Url::parse(&format!("https://{FQDN}")).unwrap(),
            options,
        )
        .unwrap();
    let mut value = serde_json::to_value(&credential).unwrap();
    value
        .as_object_mut()
        .unwrap()
        .insert("challengeId".to_owned(), challenge_id.into());

    // A different user tries to finish it.
    let thief = principal::member_context(member());
    let result =
        auth_context::with_context(thief, f.svc.finish_passkey_registration(FQDN, None, value))
            .await;

    assert!(matches!(result, Err(AppError::Forbidden(_))));
    assert!(f.credentials.rows.lock().unwrap().is_empty());
}

// -- authentication -------------------------------------------------------

#[tokio::test]
async fn a_registered_passkey_signs_in() {
    let f = fixture(true);
    let mut authenticator = register(&f, member(), "Pixel 8").await.unwrap();
    let credential_id = only_credential_subject(&f);

    let (profile, role) = sign_in(&f, &mut authenticator, &credential_id, FQDN)
        .await
        .expect("a registered passkey must sign in");

    assert_eq!(profile.id, member());
    assert_eq!(role, UserRole::Member, "the role comes from the users row");
}

#[tokio::test]
async fn sign_in_starts_without_any_username() {
    // Discoverable credentials: the request carries no allow-list, so it also
    // discloses nothing about which accounts exist.
    let f = fixture(true);
    register(&f, member(), "Pixel 8").await.unwrap();

    let challenge = f.svc.start_passkey_authentication(FQDN).await.unwrap();
    let allow = challenge
        .get("publicKey")
        .and_then(|k| k.get("allowCredentials"));

    assert!(
        allow.is_none_or(|a| a.as_array().is_some_and(Vec::is_empty)),
        "a discoverable sign-in must not enumerate credentials: {challenge}"
    );
}

#[tokio::test]
async fn an_unregistered_passkey_is_refused() {
    let f = fixture(true);
    // Register for one household, then present a credential from a *different*
    // authenticator that this box has never seen.
    register(&f, member(), "Pixel 8").await.unwrap();

    let challenge = f.svc.start_passkey_authentication(FQDN).await.unwrap();
    let challenge_id = challenge
        .get("challengeId")
        .and_then(|v| v.as_str())
        .unwrap()
        .to_owned();
    let options: webauthn_rs::prelude::RequestChallengeResponse =
        serde_json::from_value(challenge).unwrap();

    // A brand-new authenticator with no registration here.
    let mut stranger = WebauthnAuthenticator::new(SoftPasskey::new(true));
    let attempt = stranger.do_authentication(
        url::Url::parse(&format!("https://{FQDN}")).unwrap(),
        options,
    );

    // The soft authenticator may refuse outright (no matching credential); if it
    // produces something, the daemon must refuse it.
    if let Ok(assertion) = attempt {
        let mut value = serde_json::to_value(&assertion).unwrap();
        value
            .as_object_mut()
            .unwrap()
            .insert("challengeId".to_owned(), challenge_id.into());
        let result = f.svc.finish_passkey_authentication(FQDN, value).await;
        assert!(
            matches!(result, Err(AppError::Unauthorized(_))),
            "an unknown credential must not authenticate"
        );
    }
}

#[tokio::test]
async fn an_authentication_challenge_is_single_use() {
    // The ceremony entry is taken, not read, so a captured assertion cannot be
    // replayed even though the first use succeeded.
    let f = fixture(true);
    let mut authenticator = register(&f, member(), "Pixel 8").await.unwrap();
    let credential_id = only_credential_subject(&f);

    let challenge = f.svc.start_passkey_authentication(FQDN).await.unwrap();
    let challenge_id = challenge
        .get("challengeId")
        .and_then(|v| v.as_str())
        .unwrap()
        .to_owned();

    let mut options = challenge;
    options.as_object_mut().unwrap().remove("challengeId");
    if let Some(public_key) = options.get_mut("publicKey").and_then(|k| k.as_object_mut()) {
        public_key.insert(
            "allowCredentials".to_owned(),
            serde_json::json!([{ "type": "public-key", "id": credential_id }]),
        );
    }
    let options: webauthn_rs::prelude::RequestChallengeResponse =
        serde_json::from_value(options).unwrap();
    let assertion = authenticator
        .do_authentication(
            url::Url::parse(&format!("https://{FQDN}")).unwrap(),
            options,
        )
        .unwrap();

    let mut value = serde_json::to_value(&assertion).unwrap();
    value
        .as_object_mut()
        .unwrap()
        .insert("challengeId".to_owned(), challenge_id.into());

    f.svc
        .finish_passkey_authentication(FQDN, value.clone())
        .await
        .unwrap();

    let replay = f.svc.finish_passkey_authentication(FQDN, value).await;
    assert!(
        matches!(replay, Err(AppError::Unauthorized(_))),
        "a replayed assertion must be refused"
    );
}

#[tokio::test]
async fn a_missing_challenge_id_is_a_bad_request() {
    // The ceremony handle is server-minted; a response without it is malformed
    // rather than unauthorized.
    let f = fixture(true);
    let result = f
        .svc
        .finish_passkey_authentication(FQDN, serde_json::json!({}))
        .await;
    assert!(matches!(result, Err(AppError::BadRequest(_))));
}

#[tokio::test]
async fn signing_in_records_the_credential_as_used() {
    let f = fixture(true);
    let mut authenticator = register(&f, member(), "Pixel 8").await.unwrap();
    let credential_row_id = f.credentials.rows.lock().unwrap()[0].id.clone();
    let credential_id = only_credential_subject(&f);
    assert!(f.credentials.last_used(&credential_row_id).is_none());

    sign_in(&f, &mut authenticator, &credential_id, FQDN)
        .await
        .unwrap();

    assert!(
        f.credentials.last_used(&credential_row_id).is_some(),
        "a successful passkey sign-in stamps the credential"
    );
}

// -- reset ----------------------------------------------------------------

#[tokio::test]
async fn resetting_passkeys_removes_them_all_and_unpins_the_rp_id() {
    // The explicit recovery for an RP-ID divergence. It has to clear both: the
    // credentials alone would leave a stale pin, and the pin alone would leave
    // passkeys nobody can use.
    let f = fixture(true);
    register(&f, admin(), "laptop").await.unwrap();
    register(&f, member(), "Pixel 8").await.unwrap();
    assert_eq!(f.credentials.rows.lock().unwrap().len(), 2);

    let ctx = principal::admin_context(admin());
    let removed = auth_context::with_context(ctx, f.svc.reset_passkeys())
        .await
        .unwrap();

    assert_eq!(
        removed, 2,
        "every household passkey must go, not just the caller's"
    );
    assert!(f.credentials.rows.lock().unwrap().is_empty());
    assert!(
        f.config.read(KEY_PASSKEY_RP_ID).is_none(),
        "the pin must be cleared so the next registration can re-pin"
    );
}

#[tokio::test]
async fn resetting_passkeys_leaves_passwords_alone() {
    // This is what makes the reset recoverable: the household can still sign in
    // with passwords afterwards.
    let f = fixture(true);
    register(&f, member(), "Pixel 8").await.unwrap();
    f.credentials
        .set_password(
            "cred-pw",
            MEMBER_ID,
            "kid@example.com",
            "$argon2-hash",
            "now",
        )
        .await
        .unwrap();

    let ctx = principal::admin_context(admin());
    auth_context::with_context(ctx, f.svc.reset_passkeys())
        .await
        .unwrap();

    assert!(
        f.credentials
            .find_password(MEMBER_ID)
            .await
            .unwrap()
            .is_some(),
        "the local password is the floor and must survive a passkey reset"
    );
}

#[tokio::test]
async fn resetting_passkeys_requires_admin() {
    let f = fixture(true);
    let ctx = principal::member_context(member());

    let result = auth_context::with_context(ctx, f.svc.reset_passkeys()).await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}

#[tokio::test]
async fn a_passkey_can_be_used_again_after_a_re_pin() {
    // After a reset the household re-registers, and the new pin takes the
    // current hostname. Proves the recovery path actually recovers.
    let f = fixture(true);
    register(&f, admin(), "laptop").await.unwrap();
    let ctx = principal::admin_context(admin());
    auth_context::with_context(ctx, f.svc.reset_passkeys())
        .await
        .unwrap();

    f.config
        .values
        .lock()
        .unwrap()
        .insert(KEY_SUBDOMAIN.to_owned(), "moved.example.com".to_owned());

    let ctx = principal::admin_context(admin());
    let challenge =
        auth_context::with_context(ctx, f.svc.start_passkey_registration("moved.example.com"))
            .await;

    assert!(
        challenge.is_ok(),
        "after a reset, registration at the new hostname must work: {challenge:?}"
    );
    assert_eq!(
        f.config.read(KEY_PASSKEY_RP_ID).as_deref(),
        Some("moved.example.com")
    );
}
