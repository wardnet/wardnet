//! Region selection tests: lowest-latency reachable bridge wins; unhealthy
//! endpoints are skipped; all-unreachable is an error.

use std::time::Duration;

use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::ddns::region::select_best;

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

#[tokio::test]
async fn picks_lowest_latency() {
    let slow = health_server(Some(Duration::from_millis(300)), 200).await;
    let fast = health_server(None, 200).await;

    let entries = vec![
        ("slow".to_owned(), slow.uri()),
        ("fast".to_owned(), fast.uri()),
    ];
    let chosen = select_best(&reqwest::Client::new(), &entries)
        .await
        .unwrap();
    assert_eq!(chosen.slug, "fast");
}

#[tokio::test]
async fn skips_unhealthy() {
    let unhealthy = health_server(None, 500).await;
    let healthy = health_server(None, 200).await;

    let entries = vec![
        ("unhealthy".to_owned(), unhealthy.uri()),
        ("healthy".to_owned(), healthy.uri()),
    ];
    let chosen = select_best(&reqwest::Client::new(), &entries)
        .await
        .unwrap();
    assert_eq!(chosen.slug, "healthy");
}

#[tokio::test]
async fn errors_when_none_reachable() {
    let unhealthy = health_server(None, 503).await;
    let entries = vec![("unhealthy".to_owned(), unhealthy.uri())];
    assert!(
        select_best(&reqwest::Client::new(), &entries)
            .await
            .is_err()
    );
}
