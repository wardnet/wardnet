//! The household user directory (ADR-0031).
//!
//! Every route here is admin-gated except `GET /api/users/me` and
//! `POST /api/users/me/password`, which are a user acting on themselves. The
//! gate itself lives in `UserService`, not in these handlers: `get_user` and
//! `update_profile` allow "admin, **or** that user about themselves", and that
//! ownership check belongs next to the truth table in
//! `wardnetd-services/src/tests/auth_context.rs` where a reviewer will see it
//! (`.agents/auth.md`).

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use wardnet_common::api::{
    ApiError, ChangePasswordRequest, CreateUserRequest, CredentialKindDto, EnrolmentInviteResponse,
    EnrolmentResponse, ListEnrolmentsResponse, ListUserCredentialsResponse, ListUsersResponse,
    MeResponse, SetUserEnabledRequest, SetUserRoleRequest, UpdateUserProfileRequest,
    UserCredentialResponse, UserResponse,
};

use crate::api::middleware::SessionAuth;
use crate::api::responses::{AuthErrors, BadRequest, Conflict, NotFound};
use crate::state::AppState;
use wardnetd_data::repository::user_credential::{CredentialKind, CredentialSummary};
use wardnetd_services::error::AppError;
use wardnetd_services::user::{EnrolmentSummary, NewUser, OauthProvider, UserProfile};

const TAG: &str = "users";

/// Register user-identity routes onto the given [`OpenApiRouter`].
pub fn register(router: OpenApiRouter<AppState>) -> OpenApiRouter<AppState> {
    router
        .routes(routes!(me))
        .routes(routes!(change_own_password))
        .routes(routes!(list_users, create_user))
        .routes(routes!(get_user, update_profile, delete_user))
        .routes(routes!(set_enabled))
        .routes(routes!(set_role))
        .routes(routes!(list_credentials))
        .routes(routes!(unlink_oauth))
        .routes(routes!(issue_enrolment, list_enrolments))
        .routes(routes!(revoke_enrolment))
}

/// Convert a service-layer profile into its wire form.
///
/// `pub(crate)` because enrolment redemption in [`crate::api::user_auth`]
/// returns the freshly enrolled user through the same shape.
pub(crate) fn user_response(profile: UserProfile) -> UserResponse {
    UserResponse {
        id: profile.id,
        display_name: profile.display_name,
        email: profile.email,
        role: profile.role,
        enabled: profile.enabled,
        created_at: profile.created_at,
        updated_at: profile.updated_at,
    }
}

/// Map the data layer's credential kind onto its wire mirror.
///
/// An explicit match, not a string round-trip: adding a variant to either
/// enum breaks this function, which is the point. A `_ =>` arm here would let
/// a new credential kind quietly render as an existing one.
fn credential_kind_dto(kind: CredentialKind) -> CredentialKindDto {
    match kind {
        CredentialKind::Password => CredentialKindDto::Password,
        CredentialKind::Google => CredentialKindDto::Google,
        CredentialKind::Github => CredentialKindDto::Github,
        CredentialKind::Passkey => CredentialKindDto::Passkey,
    }
}

/// Convert a credential summary into its wire form.
///
/// Drops `user_id` (already in the path) and `metadata` (kind-specific JSON
/// with no consumer yet). Neither carries a secret; the omission is about
/// keeping the published surface to what a client actually reads.
fn credential_response(summary: CredentialSummary) -> UserCredentialResponse {
    UserCredentialResponse {
        id: summary.id,
        kind: credential_kind_dto(summary.kind),
        subject: summary.subject,
        label: summary.label,
        created_at: summary.created_at,
        last_used_at: summary.last_used_at,
    }
}

fn enrolment_response(summary: EnrolmentSummary) -> EnrolmentResponse {
    EnrolmentResponse {
        id: summary.id,
        user_id: summary.user_id,
        created_at: summary.created_at,
        expires_at: summary.expires_at,
        used_at: summary.used_at,
    }
}

/// Parse a `{id}`-style path segment into a UUID.
fn parse_id(raw: &str, what: &str) -> Result<Uuid, AppError> {
    raw.parse()
        .map_err(|_| AppError::BadRequest(format!("invalid {what} ID")))
}

#[utoipa::path(
    get,
    path = "/api/users/me",
    tag = TAG,
    description = "Return the authenticated household user's identity. Used by \
                   the web UI (e.g. the setup wizard's review step) to display \
                   the account name without a separate credential store, and to \
                   decide which admin-only surfaces to render. Available to any \
                   authenticated user, including members reading their own \
                   profile.",
    responses(
        (status = 200, description = "Authenticated user identity", body = MeResponse),
        AuthErrors,
        (status = 500, description = "Internal server error", body = ApiError),
    ),
)]
pub async fn me(
    State(state): State<AppState>,
    _auth: SessionAuth,
) -> Result<Json<MeResponse>, AppError> {
    let user = state.auth_service().current_user().await?;
    Ok(Json(MeResponse {
        // `username` and `display_name` intentionally carry the same value: the
        // former is the pre-ADR-0031 field name that existing clients read, kept
        // so this stays an additive change.
        username: user.display_name.clone(),
        id: user.user_id.to_string(),
        display_name: user.display_name,
        email: user.email,
        role: user.role,
    }))
}

#[utoipa::path(
    post,
    path = "/api/users/me/password",
    tag = TAG,
    description = "Set your own password, proving the current one first. \
                   **Every** live session for the account is invalidated, \
                   including the one making this call — a password change is \
                   usually a response to 'somebody may know my password', and \
                   leaving that person's session alive would make the change \
                   cosmetic. The caller must sign in again with the new \
                   password. Not an admin route in either direction: an admin \
                   cannot set someone else's password (they would then know \
                   it), and a member changing their own needs no admin.",
    request_body = ChangePasswordRequest,
    responses(
        (status = 204, description = "Password changed; all sessions invalidated, including this one"),
        AuthErrors,
        BadRequest,
    ),
)]
pub async fn change_own_password(
    State(state): State<AppState>,
    _auth: SessionAuth,
    Json(body): Json<ChangePasswordRequest>,
) -> Result<StatusCode, AppError> {
    state
        .user_service()
        .change_own_password(&body.current_password, &body.new_password)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    get,
    path = "/api/users",
    tag = TAG,
    description = "Every household user, for the admin directory. Carries no \
                   credential material of any kind. Admin only — the list is \
                   the household's roster, and a member has no business \
                   enumerating it.",
    responses(
        (status = 200, description = "The household directory", body = ListUsersResponse),
        AuthErrors,
    ),
)]
pub async fn list_users(
    State(state): State<AppState>,
    _auth: SessionAuth,
) -> Result<Json<ListUsersResponse>, AppError> {
    let users = state.user_service().list_users().await?;
    Ok(Json(ListUsersResponse {
        users: users.into_iter().map(user_response).collect(),
    }))
}

#[utoipa::path(
    post,
    path = "/api/users",
    tag = TAG,
    description = "Create a household user with **no credential**. The account \
                   cannot be used until it is enrolled: the admin never learns \
                   a member's password, so account creation and credential \
                   creation are deliberately separate steps. Issue an \
                   invitation with `POST /api/users/{id}/enrolments` next. \
                   Admin only.",
    request_body = CreateUserRequest,
    responses(
        (status = 200, description = "The created user", body = UserResponse),
        AuthErrors,
        BadRequest,
        Conflict,
    ),
)]
pub async fn create_user(
    State(state): State<AppState>,
    _auth: SessionAuth,
    Json(body): Json<CreateUserRequest>,
) -> Result<Json<UserResponse>, AppError> {
    let profile = state
        .user_service()
        .create_user(NewUser {
            display_name: body.display_name,
            email: body.email,
            role: body.role,
        })
        .await?;
    Ok(Json(user_response(profile)))
}

#[utoipa::path(
    get,
    path = "/api/users/{id}",
    tag = TAG,
    description = "One household user. Readable by an admin, or by that user \
                   about themselves — a member must not be able to enumerate \
                   the household by walking ids.",
    params(("id" = Uuid, Path, description = "User ID")),
    responses(
        (status = 200, description = "The user", body = UserResponse),
        AuthErrors,
        NotFound,
        BadRequest,
    ),
)]
pub async fn get_user(
    State(state): State<AppState>,
    _auth: SessionAuth,
    Path(id): Path<String>,
) -> Result<Json<UserResponse>, AppError> {
    let profile = state
        .user_service()
        .get_user(parse_id(&id, "user")?)
        .await?;
    Ok(Json(user_response(profile)))
}

#[utoipa::path(
    patch,
    path = "/api/users/{id}",
    tag = TAG,
    description = "Update a user's display name and email. A user may edit \
                   their own profile; changing anybody else's is an admin \
                   action. Both fields are replacements — omitting `email` \
                   clears it.",
    params(("id" = Uuid, Path, description = "User ID")),
    request_body = UpdateUserProfileRequest,
    responses(
        (status = 200, description = "The updated user", body = UserResponse),
        AuthErrors,
        NotFound,
        BadRequest,
        Conflict,
    ),
)]
pub async fn update_profile(
    State(state): State<AppState>,
    _auth: SessionAuth,
    Path(id): Path<String>,
    Json(body): Json<UpdateUserProfileRequest>,
) -> Result<Json<UserResponse>, AppError> {
    let profile = state
        .user_service()
        .update_profile(
            parse_id(&id, "user")?,
            &body.display_name,
            body.email.as_deref(),
        )
        .await?;
    Ok(Json(user_response(profile)))
}

#[utoipa::path(
    delete,
    path = "/api/users/{id}",
    tag = TAG,
    description = "Delete a household user. Credentials, enrolment tokens and \
                   sessions cascade; `devices.owner_user_id` is set to NULL, \
                   because deleting a person must not delete the household's \
                   hardware. Refuses to delete the last enabled admin — a box \
                   with no admin is a box nobody can administer, and there is \
                   no recovery path from outside. Admin only.",
    params(("id" = Uuid, Path, description = "User ID")),
    responses(
        (status = 204, description = "User deleted"),
        AuthErrors,
        NotFound,
        BadRequest,
        Conflict,
    ),
)]
pub async fn delete_user(
    State(state): State<AppState>,
    _auth: SessionAuth,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    state
        .user_service()
        .delete_user(parse_id(&id, "user")?)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    // Explicit: the handler name `set_enabled` collides with Private DNS's
    // `PUT /api/private-dns`, and a duplicate `operationId` makes the
    // generated Go client fail to compile (two methods of the same name on one
    // interface). Operation ids are a flat global namespace across the whole
    // spec, so a handler name only unique within its module is not enough.
    operation_id = "set_user_enabled",
    put,
    path = "/api/users/{id}/enabled",
    tag = TAG,
    description = "Enable or disable a household user. Disabling revokes \
                   immediately: every live session is deleted, and the login \
                   join filters the account out from the next request onward. \
                   Refuses to disable the last enabled admin. Admin only.",
    params(("id" = Uuid, Path, description = "User ID")),
    request_body = SetUserEnabledRequest,
    responses(
        (status = 200, description = "The updated user", body = UserResponse),
        AuthErrors,
        NotFound,
        BadRequest,
        Conflict,
    ),
)]
pub async fn set_enabled(
    State(state): State<AppState>,
    _auth: SessionAuth,
    Path(id): Path<String>,
    Json(body): Json<SetUserEnabledRequest>,
) -> Result<Json<UserResponse>, AppError> {
    let profile = state
        .user_service()
        .set_enabled(parse_id(&id, "user")?, body.enabled)
        .await?;
    Ok(Json(user_response(profile)))
}

#[utoipa::path(
    put,
    path = "/api/users/{id}/role",
    tag = TAG,
    description = "Change a user's role between `admin` and `member`. An \
                   `admin` household user is exactly equal to the legacy local \
                   admin — there is no second tier. Refuses to demote the last \
                   enabled admin. Admin only.",
    params(("id" = Uuid, Path, description = "User ID")),
    request_body = SetUserRoleRequest,
    responses(
        (status = 200, description = "The updated user", body = UserResponse),
        AuthErrors,
        NotFound,
        BadRequest,
        Conflict,
    ),
)]
pub async fn set_role(
    State(state): State<AppState>,
    _auth: SessionAuth,
    Path(id): Path<String>,
    Json(body): Json<SetUserRoleRequest>,
) -> Result<Json<UserResponse>, AppError> {
    let profile = state
        .user_service()
        .set_role(parse_id(&id, "user")?, body.role)
        .await?;
    Ok(Json(user_response(profile)))
}

#[utoipa::path(
    get,
    path = "/api/users/{id}/credentials",
    tag = TAG,
    description = "List a user's credentials, without secrets. The response \
                   type structurally has no secret field, so this cannot leak \
                   one by a later widening. Admin only.",
    params(("id" = Uuid, Path, description = "User ID")),
    responses(
        (status = 200, description = "The user's credentials", body = ListUserCredentialsResponse),
        AuthErrors,
        NotFound,
        BadRequest,
    ),
)]
pub async fn list_credentials(
    State(state): State<AppState>,
    _auth: SessionAuth,
    Path(id): Path<String>,
) -> Result<Json<ListUserCredentialsResponse>, AppError> {
    let credentials = state
        .user_service()
        .list_credentials(parse_id(&id, "user")?)
        .await?;
    Ok(Json(ListUserCredentialsResponse {
        credentials: credentials.into_iter().map(credential_response).collect(),
    }))
}

#[utoipa::path(
    delete,
    path = "/api/users/{id}/credentials/{provider}",
    tag = TAG,
    description = "Unlink every credential of one federated provider from a \
                   user. Never touches the local password: that is the floor, \
                   and an account whose only credential depended on a reachable \
                   provider would be unreachable during a WAN outage. \
                   Idempotent — unlinking a provider that was never linked \
                   succeeds. Admin only.",
    params(
        ("id" = Uuid, Path, description = "User ID"),
        ("provider" = String, Path, description = "`google` or `github`"),
    ),
    responses(
        (status = 204, description = "Provider credentials unlinked"),
        AuthErrors,
        NotFound,
        BadRequest,
    ),
)]
pub async fn unlink_oauth(
    State(state): State<AppState>,
    _auth: SessionAuth,
    Path((id, provider)): Path<(String, String)>,
) -> Result<StatusCode, AppError> {
    let provider = OauthProvider::parse(&provider)
        .ok_or_else(|| AppError::BadRequest(format!("unknown identity provider: {provider}")))?;
    state
        .user_service()
        .unlink_oauth(parse_id(&id, "user")?, provider)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/api/users/{id}/enrolments",
    tag = TAG,
    description = "Issue a one-time enrolment invitation for a user who has no \
                   password yet. The `token` in the response is shown **exactly \
                   once** — only its hash is stored, so an admin who loses it \
                   must issue another. Hand it over out of band; the member \
                   redeems it at `POST /api/auth/enrolments/redeem` and chooses \
                   their own password, which the admin never learns. Admin only.",
    params(("id" = Uuid, Path, description = "User ID")),
    responses(
        (status = 200, description = "The invitation, with its one-time token", body = EnrolmentInviteResponse),
        AuthErrors,
        NotFound,
        BadRequest,
        Conflict,
    ),
)]
pub async fn issue_enrolment(
    State(state): State<AppState>,
    _auth: SessionAuth,
    Path(id): Path<String>,
) -> Result<Json<EnrolmentInviteResponse>, AppError> {
    let invite = state
        .user_service()
        .issue_enrolment(parse_id(&id, "user")?)
        .await?;
    Ok(Json(EnrolmentInviteResponse {
        token: invite.token,
        expires_at: invite.expires_at,
        user_id: invite.user_id,
    }))
}

#[utoipa::path(
    get,
    path = "/api/users/{id}/enrolments",
    tag = TAG,
    description = "Outstanding and spent invitations for a user, so an admin \
                   can see whether one is still open before issuing another. \
                   Tokens are never included — only their metadata. Admin only.",
    params(("id" = Uuid, Path, description = "User ID")),
    responses(
        (status = 200, description = "The user's invitations", body = ListEnrolmentsResponse),
        AuthErrors,
        NotFound,
        BadRequest,
    ),
)]
pub async fn list_enrolments(
    State(state): State<AppState>,
    _auth: SessionAuth,
    Path(id): Path<String>,
) -> Result<Json<ListEnrolmentsResponse>, AppError> {
    let enrolments = state
        .user_service()
        .list_enrolments(parse_id(&id, "user")?)
        .await?;
    Ok(Json(ListEnrolmentsResponse {
        enrolments: enrolments.into_iter().map(enrolment_response).collect(),
    }))
}

#[utoipa::path(
    delete,
    path = "/api/users/{id}/enrolments/{enrolment_id}",
    tag = TAG,
    description = "Revoke an unredeemed invitation, for an admin who issued one \
                   by mistake or to the wrong person. Admin only.",
    params(
        ("id" = Uuid, Path, description = "User ID"),
        ("enrolment_id" = Uuid, Path, description = "Enrolment ID"),
    ),
    responses(
        (status = 204, description = "Invitation revoked"),
        AuthErrors,
        NotFound,
        BadRequest,
    ),
)]
pub async fn revoke_enrolment(
    State(state): State<AppState>,
    _auth: SessionAuth,
    Path((id, enrolment_id)): Path<(String, String)>,
) -> Result<StatusCode, AppError> {
    state
        .user_service()
        .revoke_enrolment(
            parse_id(&id, "user")?,
            parse_id(&enrolment_id, "enrolment")?,
        )
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
