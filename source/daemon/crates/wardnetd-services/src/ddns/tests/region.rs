//! Region selection tests: lowest-latency reachable region wins; unhealthy
//! endpoints are skipped; all-unreachable is an error.
//!
//! Each region's `health_url` points at its wiremock server's `/v1/health`,
//! while `ddns_base_url` is the server root — mirroring the production split
//! between the plain-HTTP `:81` health probe and the `:443` control base.

use std::time::Duration;

use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::ddns::region::{RegionEndpoint, select_best};

async fn health_server(delay: Option<Duration>, status: u16) -> MockServer {
    let server = MockServer::start().await;
    let mut template = ResponseTemplate::new(status);
    if let Some(delay) = delay {
        template = template.set_delay(delay);
    }
    Mock::given(method("GET"))
        .and(path("/v1/health"))
        .respond_with(template)
        .mount(&server)
        .await;
    server
}

/// A catalog entry whose health probe targets `server`'s `/v1/health`.
fn entry(slug: &str, server: &MockServer) -> RegionEndpoint {
    RegionEndpoint {
        slug: slug.to_owned(),
        ddns_base_url: server.uri(),
        health_url: format!("{}/v1/health", server.uri()),
    }
}

#[tokio::test]
async fn picks_lowest_latency() {
    let slow = health_server(Some(Duration::from_millis(300)), 200).await;
    let fast = health_server(None, 200).await;

    let entries = vec![entry("slow", &slow), entry("fast", &fast)];
    let chosen = select_best(&reqwest::Client::new(), &entries)
        .await
        .unwrap();
    assert_eq!(chosen.slug, "fast");
    assert_eq!(chosen.ddns_base_url, fast.uri());
}

#[tokio::test]
async fn skips_unhealthy() {
    let unhealthy = health_server(None, 500).await;
    let healthy = health_server(None, 200).await;

    let entries = vec![entry("unhealthy", &unhealthy), entry("healthy", &healthy)];
    let chosen = select_best(&reqwest::Client::new(), &entries)
        .await
        .unwrap();
    assert_eq!(chosen.slug, "healthy");
}

#[tokio::test]
async fn errors_when_none_reachable() {
    let unhealthy = health_server(None, 503).await;
    let entries = vec![entry("unhealthy", &unhealthy)];
    assert!(
        select_best(&reqwest::Client::new(), &entries)
            .await
            .is_err()
    );
}
