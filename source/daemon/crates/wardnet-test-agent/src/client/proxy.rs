//! `POST /proxy` -- forwards an HTTP request from a LAN client to the
//! daemon and returns the daemon's status + body verbatim.
//!
//! The daemon identifies self-service callers (`/api/devices/me`,
//! `PUT /api/devices/me/rule`, ...) by the **source IP** of the incoming
//! TCP connection -- no proxy headers are trusted (see the daemon's
//! `ClientIp` extractor). A LAN client therefore cannot exercise those
//! flows from the test runner: the runner sits on the management network
//! and its packets carry the wrong source address.
//!
//! This handler closes that gap. It runs on the LAN-side client agent, so
//! a request it makes to the daemon originates from *this container's* LAN
//! address. `source_ip` pins the outgoing socket to a specific local
//! address -- a client holds both its docker-IPAM IP and its DHCP lease on
//! the same interface, and only the lease is a discovered device, so specs
//! bind the lease to be classified as that device.

use serde::{Deserialize, Serialize};

use super::models::ClientError;

/// Default daemon base URL: the daemon owns `10.91.0.1` on the simulated
/// LAN (`wardnet_lan`), so every client can reach the plain-HTTP API there.
const DEFAULT_TARGET: &str = "http://10.91.0.1:7411";

fn default_target() -> String {
    DEFAULT_TARGET.to_owned()
}

fn default_method() -> String {
    "GET".to_owned()
}

/// Parameters for a single proxied request.
#[derive(Debug)]
pub struct ProxyArgs {
    /// HTTP method (`GET`, `PUT`, `POST`, `PATCH`, `DELETE`, ...).
    pub method: String,
    /// Path on the daemon, e.g. `/api/devices/me`.
    pub path: String,
    /// Daemon base URL. Defaults to [`DEFAULT_TARGET`].
    pub target: String,
    /// Local address to bind the outgoing socket to. Binding the caller's
    /// DHCP-leased IP makes the daemon classify the request as that device.
    pub source_ip: Option<String>,
    /// Optional JSON request body.
    pub body: Option<serde_json::Value>,
}

/// The daemon's response, surfaced back to the spec verbatim.
#[derive(Debug, Serialize)]
pub struct ProxyResponse {
    /// The daemon's HTTP status code. A 4xx/5xx from the daemon (e.g. the
    /// 403 an admin-locked device gets) is a *successful* proxy call --
    /// only transport failures surface as the agent's own 500.
    pub status: u16,
    /// The daemon's response body, parsed as JSON when possible so specs
    /// can assert on fields; otherwise the raw text under a JSON string.
    pub body: serde_json::Value,
}

/// Forward one request to the daemon and capture its status + body.
pub async fn run(args: ProxyArgs) -> Result<ProxyResponse, ClientError> {
    let method = reqwest::Method::from_bytes(args.method.to_uppercase().as_bytes())
        .map_err(|e| ClientError::new(format!("invalid HTTP method '{}': {e}", args.method)))?;

    let mut builder = reqwest::Client::builder();
    if let Some(ip) = &args.source_ip {
        let addr: std::net::IpAddr = ip
            .parse()
            .map_err(|e| ClientError::new(format!("invalid source_ip '{ip}': {e}")))?;
        builder = builder.local_address(addr);
    }
    let client = builder
        .build()
        .map_err(|e| ClientError::new(format!("failed to build HTTP client: {e}")))?;

    let url = format!("{}{}", args.target, args.path);
    let mut request = client.request(method, &url);
    if let Some(body) = &args.body {
        request = request.json(body);
    }

    let response = request
        .send()
        .await
        .map_err(|e| ClientError::new(format!("proxy request to {url} failed: {e}")))?;

    let status = response.status().as_u16();
    let text = response
        .text()
        .await
        .map_err(|e| ClientError::new(format!("failed to read proxy response body: {e}")))?;

    // Prefer structured JSON so specs can index fields; fall back to a raw
    // string for empty or non-JSON bodies rather than erroring.
    let body = match serde_json::from_str::<serde_json::Value>(&text) {
        Ok(value) => value,
        Err(_) => serde_json::Value::String(text),
    };

    Ok(ProxyResponse { status, body })
}

/// Request body for the serve-mode `POST /proxy` route.
#[derive(Debug, Deserialize)]
pub struct ProxyRequest {
    #[serde(default = "default_method")]
    pub method: String,
    pub path: String,
    #[serde(default = "default_target")]
    pub target: String,
    #[serde(default)]
    pub source_ip: Option<String>,
    #[serde(default)]
    pub body: Option<serde_json::Value>,
}

impl From<ProxyRequest> for ProxyArgs {
    fn from(req: ProxyRequest) -> Self {
        Self {
            method: req.method,
            path: req.path,
            target: req.target,
            source_ip: req.source_ip,
            body: req.body,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn invalid_method_is_rejected() {
        let err = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(run(ProxyArgs {
                method: "bad method".to_owned(),
                path: "/api/info".to_owned(),
                target: DEFAULT_TARGET.to_owned(),
                source_ip: None,
                body: None,
            }));
        assert!(err.is_err());
    }
}
