use crate::dns::filter_parser::{self, ParsedRule};

/// Abstraction over HTTP client for downloading blocklists.
/// Uses a trait so tests can inject a mock.
#[async_trait::async_trait]
pub trait BlocklistFetcher: Send + Sync {
    async fn fetch(&self, url: &str) -> anyhow::Result<String>;
}

/// Production HTTP fetcher using reqwest.
pub struct HttpBlocklistFetcher {
    client: reqwest::Client,
}

impl Default for HttpBlocklistFetcher {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpBlocklistFetcher {
    #[must_use]
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .user_agent("wardnet-dns/0.1")
            .build()
            .expect("failed to build HTTP client");
        Self { client }
    }
}

#[async_trait::async_trait]
impl BlocklistFetcher for HttpBlocklistFetcher {
    async fn fetch(&self, url: &str) -> anyhow::Result<String> {
        let resp = self.client.get(url).send().await?;
        let status = resp.status();
        if !status.is_success() {
            anyhow::bail!("HTTP {status} for {url}");
        }
        Ok(resp.text().await?)
    }
}

/// Parse a blocklist body into deduplicated, lowercased domains.
///
/// Lines that parse to a plain `DomainBlock` (no modifiers) are included. Lines
/// that fail to parse or produce complex rules are silently skipped — this is
/// expected for hosts-file headers, comments, and regex rules in standard
/// blocklists.
#[must_use]
pub fn parse_blocklist_body(body: &str) -> Vec<String> {
    let mut domains = std::collections::HashSet::new();
    for line in body.lines() {
        match filter_parser::parse_line(line) {
            Ok(Some(ParsedRule::DomainBlock {
                domain,
                modifiers,
                allow: false,
            })) if modifiers.is_empty() => {
                domains.insert(domain);
            }
            _ => {}
        }
    }
    domains.into_iter().collect()
}
