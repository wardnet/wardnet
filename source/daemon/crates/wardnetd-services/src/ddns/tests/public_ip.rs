//! Public-IP discovery tests: first valid global IPv4 wins; failures fall
//! through to the next endpoint; non-global answers are rejected.

use std::net::Ipv4Addr;

use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::ddns::public_ip::discover_from;

async fn echo_server(status: u16, body: &str) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/"))
        .respond_with(ResponseTemplate::new(status).set_body_string(body))
        .mount(&server)
        .await;
    server
}

#[tokio::test]
async fn returns_first_global_ip() {
    let server = echo_server(200, "1.2.3.4\n").await;
    let endpoints = [server.uri()];
    let refs: Vec<&str> = endpoints.iter().map(String::as_str).collect();

    let ip = discover_from(&reqwest::Client::new(), &refs).await.unwrap();
    assert_eq!(ip, "1.2.3.4".parse::<Ipv4Addr>().unwrap());
}

#[tokio::test]
async fn falls_back_on_failure() {
    let bad = echo_server(500, "").await;
    let good = echo_server(200, "9.9.9.9").await;
    let endpoints = [bad.uri(), good.uri()];
    let refs: Vec<&str> = endpoints.iter().map(String::as_str).collect();

    let ip = discover_from(&reqwest::Client::new(), &refs).await.unwrap();
    assert_eq!(ip.to_string(), "9.9.9.9");
}

#[tokio::test]
async fn rejects_private_ip() {
    let server = echo_server(200, "192.168.1.1").await;
    let endpoints = [server.uri()];
    let refs: Vec<&str> = endpoints.iter().map(String::as_str).collect();

    assert!(discover_from(&reqwest::Client::new(), &refs).await.is_err());
}
