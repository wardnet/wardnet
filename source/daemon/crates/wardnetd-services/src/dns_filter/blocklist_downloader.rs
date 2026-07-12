//! HTTP fetcher + refresh path for DNS filter blocklists.
//!
//! Moved from `dns/blocklist_downloader.rs` as part of issue #221. After
//! the rename, blocklists are profile-scoped — the refresh helper now
//! talks to [`DnsFilterRepository`] and emits
//! [`WardnetEvent::DnsFilterBlocklistUpdated`].

use wardnet_common::dns::Blocklist;
use wardnet_common::event::WardnetEvent;
use wardnetd_data::repository::DnsFilterRepository;

use crate::dns::filter_parser::{self, ParsedRule};
use crate::event::EventPublisher;
use crate::jobs::ProgressReporter;

/// Abstraction over HTTP client for downloading blocklists.
#[async_trait::async_trait]
pub trait BlocklistFetcher: Send + Sync {
    async fn fetch(&self, url: &str) -> anyhow::Result<String>;
}

/// Production HTTP fetcher using `reqwest`.
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

/// Fetch a blocklist, parse, bulk-replace, and emit
/// [`WardnetEvent::DnsFilterBlocklistUpdated`]. The DNS filter runner
/// rebuilds the affected per-source [`crate::dns_filter::DnsFilter`] in
/// response.
pub async fn refresh_blocklist(
    blocklist: &Blocklist,
    repo: &dyn DnsFilterRepository,
    fetcher: &dyn BlocklistFetcher,
    events: &dyn EventPublisher,
    reporter: Option<&ProgressReporter>,
) -> anyhow::Result<u64> {
    if let Some(r) = reporter {
        r.set_progress(5).await;
    }

    let body = match fetcher.fetch(&blocklist.url).await {
        Ok(b) => b,
        Err(e) => {
            let msg = format!("download failed: {e}");
            tracing::error!(blocklist_id = %blocklist.id, error = %e, "blocklist download failed");
            let _ = repo.set_blocklist_error(blocklist.id, Some(&msg)).await;
            return Err(e);
        }
    };

    if let Some(r) = reporter {
        r.set_progress(60).await;
    }

    let domains = parse_blocklist_body(&body);
    let count = domains.len() as u64;

    if count == 0 {
        let msg =
            "parsed 0 domains from response (check the URL - it may redirect to an HTML page)"
                .to_owned();
        tracing::error!(blocklist_id = %blocklist.id, "{msg}");
        let _ = repo.set_blocklist_error(blocklist.id, Some(&msg)).await;
        return Err(anyhow::anyhow!(msg));
    }

    if let Some(r) = reporter {
        r.set_progress(80).await;
    }

    if let Err(e) = repo.replace_blocklist_domains(blocklist.id, &domains).await {
        let msg = format!("failed to store domains: {e}");
        tracing::error!(blocklist_id = %blocklist.id, error = %e, "failed to store blocklist domains");
        let _ = repo.set_blocklist_error(blocklist.id, Some(&msg)).await;
        return Err(e);
    }

    if let Err(e) = repo.set_blocklist_error(blocklist.id, None).await {
        tracing::warn!(blocklist_id = %blocklist.id, error = %e, "failed to clear blocklist error");
    }

    events.publish(WardnetEvent::DnsFilterBlocklistUpdated {
        blocklist_id: blocklist.id,
        entry_count: count,
        timestamp: chrono::Utc::now(),
    });

    if let Some(r) = reporter {
        r.set_progress(100).await;
    }

    tracing::info!(
        blocklist_id = %blocklist.id,
        name = %blocklist.name,
        domains = count,
        "blocklist refreshed",
    );

    Ok(count)
}

/// Parse a blocklist body into deduplicated, lowercased domains.
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
