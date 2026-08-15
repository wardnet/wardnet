//! Credential endpoints for household identity (ADR-0031): what a sign-in
//! surface may render, the federated OAuth ceremony, enrolment redemption, and
//! admin provider configuration.
//!
//! Everything here except the two provider-configuration routes carries
//! `security(())`. These paths *establish* identity and therefore cannot
//! require it — `.agents/auth.md` category (b). The document-level default in
//! [`crate::openapi`] marks routes authenticated, so a missing `security(())`
//! would document a lie rather than change behaviour, which is worse.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::header::{LOCATION, SET_COOKIE};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::{Extension, http::HeaderValue};
use serde::Deserialize;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use wardnet_common::api::{
    ApiError, AuthMethodsResponse, AuthProviderStatusResponse, ConfigureOauthProviderRequest,
    ListOauthProvidersResponse, OauthProviderConfigResponse, OauthProviderDto, OauthStartResponse,
    RedeemEnrolmentRequest, UserResponse,
};

use crate::api::auth::session_cookie;
use crate::api::middleware::{SecureTransport, SessionAuth};
use crate::api::responses::{AuthErrors, BadRequest};
use crate::api::users::user_response;
use crate::state::AppState;
use wardnetd_services::error::AppError;
use wardnetd_services::user::{OauthOutcome, OauthProvider, ProviderStatus, ReturnTo};

const TAG: &str = "auth";

/// Register the credential routes onto the given [`OpenApiRouter`].
pub fn register(router: OpenApiRouter<AppState>) -> OpenApiRouter<AppState> {
    router
        .routes(routes!(available_methods))
        .routes(routes!(start_oauth))
        .routes(routes!(complete_oauth_callback))
        .routes(routes!(redeem_enrolment))
        .routes(routes!(list_oauth_providers))
        .routes(routes!(configure_oauth_provider, clear_oauth_provider))
}

/// Parse a `{provider}` path segment, rejecting anything unrecognised.
///
/// A 400 rather than a fallback to a configured provider: guessing here would
/// mean a typo'd URL silently ran a ceremony against the wrong identity
/// provider.
fn parse_provider(raw: &str) -> Result<OauthProvider, AppError> {
    OauthProvider::parse(raw)
        .ok_or_else(|| AppError::BadRequest(format!("unknown identity provider: {raw}")))
}

/// Map the service's provider enum onto its wire mirror.
///
/// An explicit match rather than a string round-trip, so adding a provider to
/// either enum breaks here instead of silently rendering as another one.
fn oauth_provider_dto(provider: OauthProvider) -> OauthProviderDto {
    match provider {
        OauthProvider::Google => OauthProviderDto::Google,
        OauthProvider::Github => OauthProviderDto::Github,
    }
}

/// The **public** projection, for `GET /api/auth/methods`.
///
/// Deliberately drops **both** `client_id` and `redirect_uri`: that endpoint
/// is unauthenticated, and between them they name the household's own
/// registered OAuth application and its canonical hostname. A sign-in surface
/// needs neither — it renders a button from `enabled` alone. Admins read them
/// from [`list_oauth_providers`] instead.
/// By reference: with `client_id` and `redirect_uri` gone, every remaining
/// field is `Copy`, so there is nothing left to consume.
fn provider_status_response(status: &ProviderStatus) -> AuthProviderStatusResponse {
    AuthProviderStatusResponse {
        provider: oauth_provider_dto(status.provider),
        enabled: status.enabled,
        configured: status.configured,
    }
}

/// The **admin** projection, carrying the client id and the *stored* flag.
fn provider_config_response(status: ProviderStatus) -> OauthProviderConfigResponse {
    OauthProviderConfigResponse {
        provider: oauth_provider_dto(status.provider),
        client_id: status.client_id,
        // The stored flag, not the effective one — a settings form round-trips
        // this, and writing the computed value back would switch a provider
        // off whenever its secret or hostname was momentarily missing.
        enabled: status.enabled_stored,
        configured: status.configured,
        redirect_uri: status.redirect_uri,
    }
}

#[utoipa::path(
    get,
    path = "/api/auth/methods",
    tag = TAG,
    description = "Which sign-in methods this box can actually offer right \
                   now. A sign-in surface reads this to decide which buttons \
                   to render, rather than showing a federated button that \
                   fails at the provider. Reports availability only — never a \
                   credential, a client secret, or whether any particular \
                   account exists, so it discloses nothing an unauthenticated \
                   caller should not see. `password` is always true: the local \
                   password is the floor and no path removes it.",
    responses(
        (status = 200, description = "Available sign-in methods", body = AuthMethodsResponse),
        (status = 500, description = "Internal server error", body = ApiError),
    ),
    security(()),
)]
pub async fn available_methods(
    State(state): State<AppState>,
) -> Result<Json<AuthMethodsResponse>, AppError> {
    let methods = state.user_service().available_methods().await?;
    Ok(Json(AuthMethodsResponse {
        password: methods.password,
        providers: methods
            .providers
            .iter()
            .map(provider_status_response)
            .collect(),
    }))
}

/// Query parameters for the OAuth start endpoint.
///
/// Both carry client intent that **cannot survive the provider redirect on its
/// own**, because the callback is a bare redirect with no body. They are
/// parked on the ceremony here, which is what OAuth's `state` is for beyond
/// CSRF (ADR-0031 §11).
#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct StartOauthQuery {
    /// Which admin surface the ceremony began on: `admin` or `admin_app`.
    /// Defaults to `admin`. An unrecognised value is **rejected**, not
    /// sanitised into something close enough.
    pub return_to: Option<String>,
    /// Whether to issue a long-lived, refreshable session. Defaults to false.
    ///
    /// It must be decided here and nowhere else: it gates `refresh_session`,
    /// so an endpoint that raised it after the fact would be an endpoint that
    /// upgrades any short session into a 90-day one.
    pub remember_me: Option<bool>,
}

#[utoipa::path(
    get,
    path = "/api/auth/oauth/{provider}/start",
    tag = TAG,
    description = "Begin an OAuth ceremony and return the provider URL to \
                   send the browser to. Returns JSON rather than a redirect so \
                   the caller controls the navigation — a fetch that followed a \
                   302 to an external origin would be opaque to the client. \
                   A ceremony started by a signed-in caller is a **link** \
                   (attaching a provider account to their own user); one \
                   started anonymously is a **sign-in**. Which of the two it \
                   is, is recorded on the ceremony here and never re-guessed \
                   at the callback.",
    params(
        ("provider" = String, Path, description = "`google` or `github`"),
        StartOauthQuery,
    ),
    responses(
        (status = 200, description = "Provider authorize URL", body = OauthStartResponse),
        BadRequest,
        (status = 500, description = "Internal server error", body = ApiError),
    ),
    security(()),
)]
pub async fn start_oauth(
    State(state): State<AppState>,
    Path(provider): Path<String>,
    Query(query): Query<StartOauthQuery>,
) -> Result<Json<OauthStartResponse>, AppError> {
    let provider = parse_provider(&provider)?;

    let return_to = match query.return_to.as_deref() {
        None => ReturnTo::default(),
        Some(raw) => ReturnTo::parse(raw)
            .ok_or_else(|| AppError::BadRequest(format!("unknown return_to: {raw}")))?,
    };

    let redirect = state
        .user_service()
        .start_oauth(provider, return_to, query.remember_me.unwrap_or(false))
        .await?;

    Ok(Json(OauthStartResponse { url: redirect.url }))
}

/// Query parameters the provider sends back to the one registered redirect URI.
#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct OauthCallbackQuery {
    /// The single-use ceremony nonce issued at `/start`.
    pub state: Option<String>,
    /// The authorization code to exchange.
    pub code: Option<String>,
    /// Set instead of `code` when the person declined consent, or the provider
    /// refused. Its value is a provider-defined code (`access_denied`, …).
    pub error: Option<String>,
}

/// Stable, non-reflective error codes for the callback redirect.
///
/// The callback cannot render an error — it hands the browser back to a SPA —
/// so the failure has to ride in a query parameter. That parameter is a
/// **closed set of constants**, never [`AppError`]'s text: the message may name
/// a provider, a user, or an internal failure, and reflecting it into a URL
/// would both leak it and hand an attacker a text-injection sink on a page the
/// admin trusts.
const OAUTH_ERROR_ACCESS_DENIED: &str = "access_denied";
const OAUTH_ERROR_INVALID_REQUEST: &str = "invalid_request";
const OAUTH_ERROR_PROVIDER_UNAVAILABLE: &str = "provider_unavailable";
const OAUTH_ERROR_SERVER_ERROR: &str = "server_error";

fn oauth_error_code(err: &AppError) -> &'static str {
    match err {
        // The credential was read and refused: an unknown subject, a disabled
        // user, or a link attempted by someone other than the ceremony's
        // starter. **Also a spent, expired or replayed ceremony** —
        // `resolve_callback` reports a missed `take` as `Unauthorized`, so it
        // lands here rather than on the arm below. All indistinguishable to
        // the caller on purpose.
        AppError::Unauthorized(_) | AppError::Forbidden(_) | AppError::NotFound(_) => {
            OAUTH_ERROR_ACCESS_DENIED
        }
        // A malformed request — a `return_to` or provider the service refused.
        AppError::BadRequest(_) | AppError::Conflict(_) => OAUTH_ERROR_INVALID_REQUEST,
        AppError::UpstreamUnavailable(_) => OAUTH_ERROR_PROVIDER_UNAVAILABLE,
        AppError::TooManyRequests { .. } | AppError::Internal(_) | AppError::Database(_) => {
            OAUTH_ERROR_SERVER_ERROR
        }
    }
}

/// Build the 303 that hands the browser back to the admin surface.
///
/// `Location` is assembled from [`ReturnTo::path`] — one of two compile-time
/// constants — plus a constant error code. No caller-supplied text reaches the
/// header, so the classic open redirect cannot be written here (ADR-0031 §11).
fn oauth_redirect(return_to: ReturnTo, error: Option<&'static str>) -> String {
    match error {
        None => return_to.path().to_owned(),
        Some(code) => format!("{}?oauth_error={code}", return_to.path()),
    }
}

#[utoipa::path(
    get,
    path = "/api/auth/oauth/{provider}/callback",
    tag = TAG,
    description = "The provider's redirect target, and **the only OAuth \
                   callback entry point**. This exact URL is registered by \
                   hand with Google or GitHub by every household, so its shape \
                   is fixed. One URL serves both ceremonies the model allows, \
                   and the request arriving here carries nothing that says \
                   which — the stored ceremony does, and the service dispatches \
                   on it. Always answers 303 back to the admin surface the \
                   ceremony began on: on success with a session cookie set, on \
                   failure with a stable `oauth_error` code and no cookie. \
                   Never renders, and never reflects an error message.",
    params(
        ("provider" = String, Path, description = "`google` or `github`"),
        OauthCallbackQuery,
    ),
    responses(
        (status = 303, description = "Redirect to the admin surface, with a \
                                      session cookie on success or an \
                                      `oauth_error` query parameter on failure"),
        BadRequest,
    ),
    security(()),
)]
pub async fn complete_oauth_callback(
    State(state): State<AppState>,
    secure_transport: Option<Extension<SecureTransport>>,
    Path(provider): Path<String>,
    Query(query): Query<OauthCallbackQuery>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, AppError> {
    // An unparseable provider is the one failure answered with a status code
    // rather than a redirect: nothing has been consumed, and a URL this wrong
    // was never a real ceremony to return anybody from.
    parse_provider(&provider)?;

    let secure = secure_transport.is_some();
    let user_agent = headers
        .get(axum::http::header::USER_AGENT)
        .and_then(|v| v.to_str().ok());

    // The provider refused before we ever saw a code. Nothing to resolve — the
    // ceremony simply expires unused.
    if query.error.is_some() {
        tracing::info!(
            provider = %provider,
            "oauth callback: provider returned an error"
        );
        return Ok(oauth_failure(
            ReturnTo::default(),
            OAUTH_ERROR_ACCESS_DENIED,
        ));
    }

    let (Some(ceremony_state), Some(code)) = (query.state.as_deref(), query.code.as_deref()) else {
        return Ok(oauth_failure(
            ReturnTo::default(),
            OAUTH_ERROR_INVALID_REQUEST,
        ));
    };

    let outcome = match state
        .user_service()
        .complete_oauth_callback(ceremony_state, code)
        .await
    {
        Ok(outcome) => outcome,
        Err(err) => {
            // The full error is logged, never redirected. The person gets a
            // code; the admin reading logs gets the reason.
            tracing::warn!(provider = %provider, error = %err, "oauth callback failed");
            // `ReturnTo::default()`, because the ceremony that knew the real
            // one either did not resolve or no longer exists. Landing a failed
            // PWA sign-in on the desktop site is a cosmetic wrong; carrying a
            // caller-supplied path here to avoid it would not be.
            return Ok(oauth_failure(ReturnTo::default(), oauth_error_code(&err)));
        }
    };

    match outcome {
        OauthOutcome::SignedIn {
            profile,
            role: _,
            provider: signed_in_provider,
            return_to,
            remember_me,
        } => {
            // The credential was verified above: this is the sanctioned caller
            // of `issue_verified_session`, and the reason
            // `build-support/check-auth-constructors.sh` allow-lists this file.
            // Not `?`. The ceremony is already consumed by this point, and
            // this handler's contract is that it *always* answers 303 — a
            // JSON error body rendered into the browser's address bar is not
            // a place a person can act from. A failure here (a locked or full
            // database) is ours, so it maps to `server_error` like any other.
            let session = match state
                .auth_service()
                .issue_verified_session(profile.id, remember_me, user_agent)
                .await
            {
                Ok(session) => session,
                Err(err) => {
                    tracing::warn!(error = %err, "federated sign-in: session mint failed");
                    return Ok(oauth_failure(return_to, oauth_error_code(&err)));
                }
            };

            tracing::info!(
                // The provider the *ceremony* used, not the one in the URL:
                // the two need not agree, and the ceremony is the authority.
                provider = %signed_in_provider.as_str(),
                user_id = %profile.id,
                "federated sign-in succeeded"
            );

            let Some(cookie) = cookie_value(&session_cookie(
                &session.token,
                session.max_age_seconds,
                secure,
            )) else {
                tracing::error!("federated sign-in: session cookie failed to encode");
                return Ok(oauth_failure(return_to, OAUTH_ERROR_SERVER_ERROR));
            };

            Ok((
                StatusCode::SEE_OTHER,
                [
                    (LOCATION, location_value(&oauth_redirect(return_to, None))),
                    (SET_COOKIE, cookie),
                ],
            )
                .into_response())
        }
        OauthOutcome::Linked {
            user_id,
            provider: linked,
            return_to,
        } => {
            tracing::info!(
                provider = %linked.as_str(),
                user_id = %user_id,
                "provider account linked"
            );
            // No cookie: the caller already had a session, and re-minting one
            // on a link would silently re-negotiate its lifetime.
            Ok((
                StatusCode::SEE_OTHER,
                [(LOCATION, location_value(&oauth_redirect(return_to, None)))],
            )
                .into_response())
        }
    }
}

/// The failure arm of the callback: 303 with a code, never a cookie.
fn oauth_failure(return_to: ReturnTo, code: &'static str) -> axum::response::Response {
    (
        StatusCode::SEE_OTHER,
        [(
            LOCATION,
            location_value(&oauth_redirect(return_to, Some(code))),
        )],
    )
        .into_response()
}

/// Build a `Location` header from a path this module composed.
///
/// Every caller passes one of [`ReturnTo`]'s two constants, optionally plus a
/// constant error code, so the fallback is unreachable by construction — but
/// it is a fallback rather than an `unwrap`, because a panic here would turn a
/// formatting slip into a remotely-reachable crash on an unauthenticated path.
fn location_value(raw: &str) -> HeaderValue {
    HeaderValue::from_str(raw).unwrap_or_else(|_| HeaderValue::from_static(ReturnTo::Admin.path()))
}

/// Build a `Set-Cookie` header from a session cookie.
///
/// Unlike [`location_value`] this input is **not** constant — it embeds a
/// freshly minted session token — so there is no sensible substitute value. A
/// cookie that failed to encode must not silently become something else (the
/// old shared fallback would have emitted `/admin/` as a cookie); the caller
/// turns `None` into the ordinary `server_error` redirect instead.
fn cookie_value(raw: &str) -> Option<HeaderValue> {
    HeaderValue::from_str(raw).ok()
}

#[utoipa::path(
    post,
    path = "/api/auth/enrolments/redeem",
    tag = TAG,
    description = "Redeem a one-time enrolment invitation, setting the \
                   member's first password. Unauthenticated by necessity: the \
                   person redeeming has no credential yet and therefore cannot \
                   have a session. The token *is* the authorization, which is \
                   why it is single-use, expiring, and checked in SQL. \
                   Deliberately **not** rate-limited — see the note on the \
                   handler.",
    request_body = RedeemEnrolmentRequest,
    responses(
        (status = 200, description = "Enrolment redeemed; the user may now sign in", body = UserResponse),
        // One status and one message for unknown, expired and already-redeemed
        // alike. Distinguishing them would tell an unauthenticated caller which
        // guesses were once real tokens.
        (status = 401, description = "The invitation is not valid", body = ApiError),
        (status = 400, description = "The password does not meet the policy", body = ApiError),
        (status = 500, description = "Internal server error", body = ApiError),
    ),
    security(()),
)]
// No rate limiter, unlike `POST /api/auth/login`, and that asymmetry is
// deliberate. A login guesses a human-chosen password, so throttling is what
// makes it survivable; an enrolment token is 32 bytes of CSPRNG output, which
// is not guessable at any request rate a limiter would change. The cost side
// is also covered: Argon2id only runs after `find_redeemable` has matched, so
// an attacker cannot spend the box's CPU with junk tokens. Meanwhile a lockout
// here would DoS the *only* onboarding path a household has — and it is
// triggerable by anyone, since the endpoint is unauthenticated. Unknown,
// expired and spent tokens all return the same message, so the failure
// discloses nothing either.
pub async fn redeem_enrolment(
    State(state): State<AppState>,
    Json(body): Json<RedeemEnrolmentRequest>,
) -> Result<Json<UserResponse>, AppError> {
    let profile = state
        .user_service()
        .redeem_enrolment(&body.token, &body.password)
        .await?;
    Ok(Json(user_response(profile)))
}

#[utoipa::path(
    get,
    path = "/api/auth/providers",
    tag = TAG,
    description = "Every federated provider's full configuration, for the \
                   admin sign-in-methods screen: the client id, whether a \
                   secret is stored, the redirect URI to register, and the \
                   admin's **stored** on/off flag. Distinct from \
                   `GET /api/auth/methods`, which is unauthenticated and \
                   therefore reports availability only — the client id names \
                   the household's own OAuth application and is admin-only. \
                   The `enabled` here is the stored flag, so a settings form \
                   can round-trip it; `/api/auth/methods` reports the effective \
                   availability instead. Admin only.",
    responses(
        (status = 200, description = "Provider configuration", body = ListOauthProvidersResponse),
        AuthErrors,
    ),
)]
pub async fn list_oauth_providers(
    State(state): State<AppState>,
    _auth: SessionAuth,
) -> Result<Json<ListOauthProvidersResponse>, AppError> {
    let providers = state.user_service().list_oauth_providers().await?;
    Ok(Json(ListOauthProvidersResponse {
        providers: providers
            .into_iter()
            .map(provider_config_response)
            .collect(),
    }))
}

#[utoipa::path(
    put,
    path = "/api/auth/providers/{provider}",
    tag = TAG,
    description = "Configure a federated provider's client id and secret, and \
                   whether it is offered. Each household registers its own \
                   OAuth app. The secret goes to the `SecretStore` and is never \
                   readable back through any endpoint; reads report \
                   `configured: true|false`. Omitting `client_secret` leaves an \
                   existing one in place, so an admin can toggle `enabled` or \
                   fix a typo'd client id without re-pasting it. Admin only.",
    params(("provider" = String, Path, description = "`google` or `github`")),
    request_body = ConfigureOauthProviderRequest,
    responses(
        (status = 200, description = "Updated provider configuration", body = OauthProviderConfigResponse),
        AuthErrors,
        BadRequest,
    ),
)]
pub async fn configure_oauth_provider(
    State(state): State<AppState>,
    _auth: SessionAuth,
    Path(provider): Path<String>,
    Json(body): Json<ConfigureOauthProviderRequest>,
) -> Result<Json<OauthProviderConfigResponse>, AppError> {
    let provider = parse_provider(&provider)?;
    let status = state
        .user_service()
        .configure_oauth_provider(
            provider,
            &body.client_id,
            body.client_secret.as_deref(),
            body.enabled,
        )
        .await?;
    Ok(Json(provider_config_response(status)))
}

#[utoipa::path(
    delete,
    path = "/api/auth/providers/{provider}",
    tag = TAG,
    description = "Forget a provider's configuration entirely, including its \
                   client secret. Existing links to that provider are left \
                   alone — this removes the box's ability to run the ceremony, \
                   not the record of who linked what. Admin only.",
    params(("provider" = String, Path, description = "`google` or `github`")),
    responses(
        (status = 204, description = "Provider configuration cleared"),
        AuthErrors,
        BadRequest,
    ),
)]
pub async fn clear_oauth_provider(
    State(state): State<AppState>,
    _auth: SessionAuth,
    Path(provider): Path<String>,
) -> Result<StatusCode, AppError> {
    let provider = parse_provider(&provider)?;
    state.user_service().clear_oauth_provider(provider).await?;
    Ok(StatusCode::NO_CONTENT)
}
