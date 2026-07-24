use reqwest::StatusCode;

use crate::client::{Client, api_error};
use crate::error::CliError;

#[test]
fn api_error_parses_structured_body() {
    let body = br#"{"error":"not found","detail":"device x","request_id":"r1"}"#;
    let err = api_error(StatusCode::NOT_FOUND, body);
    match err {
        CliError::Api { status, body } => {
            assert_eq!(status, 404);
            assert_eq!(body.error, "not found");
            assert_eq!(body.detail.as_deref(), Some("device x"));
        }
        other => panic!("expected Api, got {other:?}"),
    }
}

#[test]
fn api_error_falls_back_to_status_text() {
    let err = api_error(StatusCode::BAD_GATEWAY, b"<html>oops</html>");
    match err {
        CliError::Api { status, body } => {
            assert_eq!(status, 502);
            assert_eq!(body.error, "bad gateway");
            assert_eq!(body.detail.as_deref(), Some("<html>oops</html>"));
        }
        other => panic!("expected Api, got {other:?}"),
    }
}

#[test]
fn url_join_has_no_double_slash() {
    let client = Client::new("http://127.0.0.1:7411", "t").unwrap();
    assert_eq!(
        client.url("/api/system/status"),
        "http://127.0.0.1:7411/api/system/status"
    );
}
