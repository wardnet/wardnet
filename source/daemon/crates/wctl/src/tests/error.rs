use wardnet_common::api::ApiError;

use crate::error::CliError;

fn api(status: u16) -> CliError {
    CliError::Api {
        status,
        body: ApiError {
            error: "boom".to_owned(),
            detail: None,
            request_id: None,
        },
    }
}

#[test]
fn exit_codes_are_stable() {
    assert_eq!(CliError::Usage(String::new()).exit_code(), 2);
    assert_eq!(CliError::Config(String::new()).exit_code(), 3);
    assert_eq!(CliError::Connection(String::new()).exit_code(), 4);
    assert_eq!(CliError::Io(String::new()).exit_code(), 7);
    assert_eq!(CliError::Protocol(String::new()).exit_code(), 1);
}

#[test]
fn api_status_maps_to_category() {
    assert_eq!(api(401).exit_code(), 5);
    assert_eq!(api(403).exit_code(), 5);
    assert_eq!(api(404).exit_code(), 6);
    assert_eq!(api(400).exit_code(), 1);
    assert_eq!(api(500).exit_code(), 1);
}

#[test]
fn api_display_includes_detail() {
    let err = CliError::Api {
        status: 400,
        body: ApiError {
            error: "bad request".to_owned(),
            detail: Some("missing field".to_owned()),
            request_id: None,
        },
    };
    assert_eq!(
        err.to_string(),
        "daemon returned 400: bad request (missing field)"
    );
}
