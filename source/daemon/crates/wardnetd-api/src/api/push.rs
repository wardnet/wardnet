use axum::Json;
use axum::extract::{Query, State};
use serde::Deserialize;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use wardnet_common::api::{
    ApiError, PushSubscriptionResponse, VapidPublicKeyResponse, WebPushSubscription,
};

use crate::state::AppState;
use wardnetd_services::error::AppError;

const TAG: &str = "push";
const PATH_VAPID: &str = "/api/push/vapid-public-key";
const PATH_SUBSCRIPTIONS: &str = "/api/push/subscriptions";

/// Register push-notification routes onto the given [`OpenApiRouter`].
pub fn register(router: OpenApiRouter<AppState>) -> OpenApiRouter<AppState> {
    router
        .routes(routes!(get_vapid_public_key))
        .routes(routes!(subscribe, unsubscribe))
}

#[utoipa::path(
    get,
    path = PATH_VAPID,
    tag = TAG,
    description = "The VAPID application server public key (base64url). \
                   Unauthenticated — the browser needs it to build a push \
                   subscription before any identity exists.",
    responses(
        (status = 200, description = "The VAPID public key", body = VapidPublicKeyResponse),
        (status = 500, description = "Internal server error", body = ApiError),
    ),
    security(()),
)]
pub async fn get_vapid_public_key(
    State(state): State<AppState>,
) -> Result<Json<VapidPublicKeyResponse>, AppError> {
    let key = state.push_service().vapid_public_key().await?;
    Ok(Json(VapidPublicKeyResponse { key }))
}

#[utoipa::path(
    post,
    path = PATH_SUBSCRIPTIONS,
    tag = TAG,
    description = "Register a Web Push subscription for the calling context. \
                   Admin callers (session cookie present) are keyed to their \
                   account; LAN devices are keyed to their MAC. Accessible to \
                   both — the caller must be a known device or an admin.",
    request_body = WebPushSubscription,
    responses(
        (status = 200, description = "Subscription stored", body = PushSubscriptionResponse),
        (status = 400, description = "Malformed subscription", body = ApiError),
        (status = 403, description = "Caller could not be identified", body = ApiError),
        (status = 500, description = "Internal server error", body = ApiError),
    ),
    security(()),
)]
pub async fn subscribe(
    State(state): State<AppState>,
    Json(body): Json<WebPushSubscription>,
) -> Result<Json<PushSubscriptionResponse>, AppError> {
    state.push_service().subscribe(body).await?;
    Ok(Json(PushSubscriptionResponse {
        message: "subscribed".to_owned(),
    }))
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct UnsubscribeQuery {
    /// When set, only the subscription with this endpoint is removed; otherwise
    /// every subscription owned by the caller is removed.
    endpoint: Option<String>,
}

#[utoipa::path(
    delete,
    path = PATH_SUBSCRIPTIONS,
    tag = TAG,
    params(UnsubscribeQuery),
    description = "Remove the caller's push subscription(s). Used when the user \
                   revokes notification permission in the app.",
    responses(
        (status = 200, description = "Subscription(s) removed", body = PushSubscriptionResponse),
        (status = 403, description = "Caller could not be identified", body = ApiError),
        (status = 500, description = "Internal server error", body = ApiError),
    ),
    security(()),
)]
pub async fn unsubscribe(
    State(state): State<AppState>,
    Query(query): Query<UnsubscribeQuery>,
) -> Result<Json<PushSubscriptionResponse>, AppError> {
    state.push_service().unsubscribe(query.endpoint).await?;
    Ok(Json(PushSubscriptionResponse {
        message: "unsubscribed".to_owned(),
    }))
}
