use axum::http::StatusCode;
use axum::response::IntoResponse;

use crate::error::AppError;
use crate::request_context;

/// Extract status code and JSON body from an `AppError` response.
async fn error_response(err: AppError) -> (StatusCode, serde_json::Value) {
    let response = err.into_response();
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), 4096)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    (status, json)
}

#[tokio::test]
async fn not_found_returns_404_with_detail() {
    let (status, json) = error_response(AppError::NotFound("thing missing".to_owned())).await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(json["error"], "not found");
    assert_eq!(json["detail"], "thing missing");
}

#[tokio::test]
async fn unauthorized_returns_401_with_detail() {
    let (status, json) = error_response(AppError::Unauthorized("bad credentials".to_owned())).await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(json["error"], "unauthorized");
    assert_eq!(json["detail"], "bad credentials");
}

#[tokio::test]
async fn forbidden_returns_403_with_detail() {
    let (status, json) = error_response(AppError::Forbidden("no access".to_owned())).await;

    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(json["error"], "forbidden");
    assert_eq!(json["detail"], "no access");
}

#[tokio::test]
async fn bad_request_returns_400_with_detail() {
    let (status, json) = error_response(AppError::BadRequest("invalid input".to_owned())).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["error"], "bad request");
    assert_eq!(json["detail"], "invalid input");
}

#[tokio::test]
async fn conflict_returns_409_with_detail() {
    let (status, json) = error_response(AppError::Conflict("already exists".to_owned())).await;

    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(json["error"], "conflict");
    assert_eq!(json["detail"], "already exists");
}

#[tokio::test]
async fn too_many_requests_returns_429_with_detail() {
    let (status, json) = error_response(AppError::TooManyRequests {
        message: "try again shortly".to_owned(),
        retry_after_seconds: 12,
    })
    .await;

    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(json["error"], "too many requests");
    assert_eq!(json["detail"], "try again shortly");
}

/// `Retry-After` is part of the 429 contract, not decoration: a client that
/// cannot read the delay retries immediately and extends its own lockout. The
/// header is set on a separate code path from the status, so it needs its own
/// assertion rather than riding along with the body test above.
#[tokio::test]
async fn too_many_requests_sets_retry_after_header() {
    let response = AppError::TooManyRequests {
        message: "slow down".to_owned(),
        retry_after_seconds: 34,
    }
    .into_response();

    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(
        response
            .headers()
            .get(axum::http::header::RETRY_AFTER)
            .expect("a 429 must carry Retry-After"),
        "34"
    );
}

/// Every other variant must leave the header off entirely — a stray
/// `Retry-After` on a 4xx tells a client to retry something it should not.
#[tokio::test]
async fn other_errors_omit_retry_after_header() {
    let response = AppError::Conflict("already exists".to_owned()).into_response();

    assert!(
        response
            .headers()
            .get(axum::http::header::RETRY_AFTER)
            .is_none(),
        "only 429 should carry Retry-After"
    );
}

#[tokio::test]
async fn internal_returns_500_without_detail() {
    let (status, json) =
        error_response(AppError::Internal(anyhow::anyhow!("secret details"))).await;

    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(json["error"], "internal server error");
    // Internal errors must not leak detail to the client.
    assert!(json.get("detail").is_none());
}

#[tokio::test]
async fn database_returns_500_without_detail() {
    let db_err = sqlx::Error::RowNotFound;
    let (status, json) = error_response(AppError::Database(db_err)).await;

    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(json["error"], "internal server error");
    assert!(json.get("detail").is_none());
}

#[tokio::test]
async fn anyhow_converts_to_internal() {
    let err: AppError = anyhow::anyhow!("something broke").into();
    let (status, _) = error_response(err).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn error_includes_request_id_when_set() {
    let (_, json) = request_context::with_request_id(
        "test-req-id-456".to_owned(),
        error_response(AppError::NotFound("missing".to_owned())),
    )
    .await;

    assert_eq!(json["request_id"], "test-req-id-456");
}

#[tokio::test]
async fn error_omits_request_id_when_not_set() {
    let (_, json) = error_response(AppError::NotFound("missing".to_owned())).await;

    // request_id should be absent (skip_serializing_if = "Option::is_none").
    assert!(
        json.get("request_id").is_none(),
        "request_id should be absent when no task-local is set"
    );
}
