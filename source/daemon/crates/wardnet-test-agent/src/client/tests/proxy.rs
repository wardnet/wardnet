//! Tests for the `/proxy` request parsing and method validation.

use crate::client::proxy::{DEFAULT_TARGET, ProxyArgs, ProxyRequest, run};

#[test]
fn proxy_request_applies_defaults() {
    let req: ProxyRequest =
        serde_json::from_str(r#"{"path":"/api/devices/me"}"#).expect("valid body");
    assert_eq!(req.method, "GET");
    assert_eq!(req.target, DEFAULT_TARGET);
    assert!(req.source_ip.is_none());
    assert!(req.body.is_none());
}

#[test]
fn proxy_request_round_trips_fields() {
    let req: ProxyRequest = serde_json::from_str(
        r#"{"method":"put","path":"/api/devices/me/rule","source_ip":"10.91.0.123",
            "body":{"target":{"type":"direct"}}}"#,
    )
    .expect("valid body");
    let args: ProxyArgs = req.into();
    assert_eq!(args.method, "put");
    assert_eq!(args.source_ip.as_deref(), Some("10.91.0.123"));
    assert_eq!(args.body.unwrap()["target"]["type"], "direct");
}

#[tokio::test]
async fn invalid_method_is_rejected() {
    let result = run(ProxyArgs {
        method: "bad method".to_owned(),
        path: "/api/info".to_owned(),
        target: DEFAULT_TARGET.to_owned(),
        source_ip: None,
        body: None,
    })
    .await;
    assert!(result.is_err());
}
