use wardnet_common::dns::Blocklist;
use wardnet_common::event::WardnetEvent;
use wardnetd_data::repository::DnsRepository;

use crate::dns::filter_parser::{self, ParsedRule};
use crate::event::EventPublisher;
use crate::jobs::ProgressReporter;

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

/// Fetch a blocklist from its upstream URL, parse the body, and bulk-replace
/// the stored domains. Clears the stored `last_error` on success, records it
/// on failure. On success publishes a [`WardnetEvent::DnsBlocklistUpdated`]
/// so the DNS runner rebuilds its in-memory filter.
///
/// The optional [`ProgressReporter`] is used by the manual (job-dispatched)
/// path; the periodic cron path passes `None`. Progress is reported at
/// coarse milestones (fetch, parse, store, publish) — byte-level progress
/// during the HTTP read is a future improvement.
///
/// Errors returned here propagate to the job executor, which records them as
/// the job's terminal error. In parallel, the helper also stamps the
/// blocklist's `last_error` column in the DB so the UI row shows the failure
/// even after the job is GC'd.
pub async fn refresh_blocklist(
    blocklist: &Blocklist,
    dns_repo: &dyn DnsRepository,
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
            let _ = dns_repo.set_blocklist_error(blocklist.id, Some(&msg)).await;
            return Err(e);
        }
    };

    if let Some(r) = reporter {
        r.set_progress(60).await;
    }

    let domains = parse_blocklist_body(&body);
    let count = domains.len() as u64;

    // A blocklist response with zero parseable domains is almost always a
    // user error (wrong URL, HTML landing page, unsupported format, a 302
    // that reqwest silently followed to an unrelated page). Fail loudly
    // rather than wiping the currently-stored domains — preserving prior
    // state keeps the filter working if the upstream is transiently broken.
    if count == 0 {
        let msg =
            "parsed 0 domains from response (check the URL — it may redirect to an HTML page)"
                .to_owned();
        tracing::error!(blocklist_id = %blocklist.id, "{msg}");
        let _ = dns_repo.set_blocklist_error(blocklist.id, Some(&msg)).await;
        return Err(anyhow::anyhow!(msg));
    }

    if let Some(r) = reporter {
        r.set_progress(80).await;
    }

    if let Err(e) = dns_repo
        .replace_blocklist_domains(blocklist.id, &domains)
        .await
    {
        let msg = format!("failed to store domains: {e}");
        tracing::error!(blocklist_id = %blocklist.id, error = %e, "failed to store blocklist domains");
        let _ = dns_repo.set_blocklist_error(blocklist.id, Some(&msg)).await;
        return Err(e);
    }

    if let Err(e) = dns_repo.set_blocklist_error(blocklist.id, None).await {
        tracing::warn!(blocklist_id = %blocklist.id, error = %e, "failed to clear blocklist error");
    }

    events.publish(WardnetEvent::DnsBlocklistUpdated {
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
