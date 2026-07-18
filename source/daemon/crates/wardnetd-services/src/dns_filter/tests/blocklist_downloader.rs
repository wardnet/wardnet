//! Tests for the blocklist downloader: body parsing, the refresh
//! pipeline (fetch → parse → bulk-replace → event) against a real
//! `SqliteDnsFilterRepository`, and the `reqwest` fetcher against a
//! wiremock server.

use std::sync::Arc;
use std::time::Duration;

use sqlx::SqlitePool;
use sqlx::sqlite::SqlitePoolOptions;
use uuid::Uuid;
use wardnet_common::dns::Blocklist;
use wardnet_common::event::WardnetEvent;
use wardnet_common::jobs::{JobKind, JobStatus};
use wardnetd_data::repository::dns_filter::BlocklistRow;
use wardnetd_data::repository::{DnsFilterRepository, SqliteDnsFilterRepository};

use crate::dns_filter::blocklist_downloader::{
    BlocklistFetcher, HttpBlocklistFetcher, parse_blocklist_body, refresh_blocklist,
};
use crate::event::{BroadcastEventBus, EventPublisher};
use crate::jobs::{JobService, JobServiceExt, JobServiceImpl};

async fn test_pool() -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("../wardnetd-data/migrations")
        .run(&pool)
        .await
        .unwrap();
    pool
}

/// Seed a profile + blocklist row and return the stored blocklist.
async fn seed_blocklist(repo: &SqliteDnsFilterRepository) -> Blocklist {
    let profile_id = Uuid::new_v4();
    repo.create_profile(profile_id, "Test profile", None)
        .await
        .unwrap();
    let id = Uuid::new_v4();
    repo.create_blocklist(&BlocklistRow {
        id: id.to_string(),
        profile_id: profile_id.to_string(),
        name: "Test list".to_owned(),
        url: "https://blocklist.example/hosts".to_owned(),
        enabled: true,
        cron_schedule: "0 3 * * *".to_owned(),
    })
    .await
    .unwrap();
    repo.get_blocklist(id).await.unwrap().expect("seeded row")
}

/// Fetcher returning a canned body or a canned error.
struct StaticFetcher(Result<String, String>);

#[async_trait::async_trait]
impl BlocklistFetcher for StaticFetcher {
    async fn fetch(&self, _url: &str) -> anyhow::Result<String> {
        match &self.0 {
            Ok(body) => Ok(body.clone()),
            Err(e) => Err(anyhow::anyhow!("{e}")),
        }
    }
}

// ── parse_blocklist_body ───────────────────────────────────────────────────

#[test]
fn parse_extracts_and_dedupes_domain_blocks() {
    let body = "! comment\n\
                ||ads.example.com^\n\
                ||ads.example.com^\n\
                ||tracker.example.net^\n\
                \n\
                @@||allowed.example.org^\n";
    let mut domains = parse_blocklist_body(body);
    domains.sort();
    // Allow rules and comments are ignored; duplicates collapse.
    assert_eq!(domains, ["ads.example.com", "tracker.example.net"]);
}

#[test]
fn parse_returns_empty_for_html_ish_body() {
    assert!(parse_blocklist_body("<html><body>Not Found</body></html>").is_empty());
}

// ── refresh_blocklist ──────────────────────────────────────────────────────

#[tokio::test]
async fn refresh_replaces_domains_clears_error_and_emits_event() {
    let repo = SqliteDnsFilterRepository::new(test_pool().await);
    let blocklist = seed_blocklist(&repo).await;
    // Pre-existing error must be cleared by a successful refresh.
    repo.set_blocklist_error(blocklist.id, Some("stale failure"))
        .await
        .unwrap();

    let events = BroadcastEventBus::new(16);
    let mut rx = events.subscribe();
    let fetcher = StaticFetcher(Ok("||ads.example.com^\n||tracker.example.net^\n".to_owned()));

    let count = refresh_blocklist(&blocklist, &repo, &fetcher, &events, None)
        .await
        .unwrap();
    assert_eq!(count, 2);

    let mut stored = repo.load_blocklist_domains(blocklist.id).await.unwrap();
    stored.sort();
    assert_eq!(stored, ["ads.example.com", "tracker.example.net"]);

    let after = repo.get_blocklist(blocklist.id).await.unwrap().unwrap();
    assert_eq!(after.last_error, None);

    match rx.try_recv().unwrap() {
        WardnetEvent::DnsFilterBlocklistUpdated {
            blocklist_id,
            entry_count,
            ..
        } => {
            assert_eq!(blocklist_id, blocklist.id);
            assert_eq!(entry_count, 2);
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[tokio::test]
async fn refresh_download_failure_records_error_and_bails() {
    let repo = SqliteDnsFilterRepository::new(test_pool().await);
    let blocklist = seed_blocklist(&repo).await;
    let events = BroadcastEventBus::new(16);
    let mut rx = events.subscribe();
    let fetcher = StaticFetcher(Err("connection refused".to_owned()));

    let err = refresh_blocklist(&blocklist, &repo, &fetcher, &events, None)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("connection refused"));

    let after = repo.get_blocklist(blocklist.id).await.unwrap().unwrap();
    let stored_err = after.last_error.expect("error recorded");
    assert!(stored_err.contains("download failed"), "got: {stored_err}");
    assert!(rx.try_recv().is_err(), "no event on failure");
}

#[tokio::test]
async fn refresh_zero_domains_records_error_and_bails() {
    let repo = SqliteDnsFilterRepository::new(test_pool().await);
    let blocklist = seed_blocklist(&repo).await;
    let events = BroadcastEventBus::new(16);
    // A body that parses to no domains (e.g. an HTML error page behind a
    // redirect) must not wipe the stored list silently.
    let fetcher = StaticFetcher(Ok("<html>moved</html>".to_owned()));

    let err = refresh_blocklist(&blocklist, &repo, &fetcher, &events, None)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("parsed 0 domains"));

    let after = repo.get_blocklist(blocklist.id).await.unwrap().unwrap();
    assert!(after.last_error.unwrap().contains("parsed 0 domains"));
}

#[tokio::test]
async fn refresh_reports_progress_through_job_reporter() {
    let repo = Arc::new(SqliteDnsFilterRepository::new(test_pool().await));
    let blocklist = seed_blocklist(&repo).await;
    let jobs = JobServiceImpl::new();

    let job_id = jobs
        .dispatch(JobKind::BlocklistRefresh, {
            let repo = repo.clone();
            move |reporter| async move {
                let events = BroadcastEventBus::new(16);
                let fetcher = StaticFetcher(Ok("||ads.example.com^\n".to_owned()));
                refresh_blocklist(
                    &blocklist,
                    repo.as_ref(),
                    &fetcher,
                    &events,
                    Some(&reporter),
                )
                .await
                .map(|_| ())
            }
        })
        .await;

    // Poll until the spawned job reaches a terminal state.
    let mut job = jobs.get(job_id).await.expect("job registered");
    for _ in 0..100 {
        if matches!(job.status, JobStatus::Succeeded | JobStatus::Failed) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
        job = jobs.get(job_id).await.expect("job still tracked");
    }
    assert_eq!(job.status, JobStatus::Succeeded, "error: {:?}", job.error);
    assert_eq!(job.percentage_done, 100);
}

// ── HttpBlocklistFetcher ───────────────────────────────────────────────────

#[tokio::test]
async fn http_fetcher_returns_body_on_success() {
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_string("||ads.example.com^\n"))
        .mount(&server)
        .await;

    let fetcher = HttpBlocklistFetcher::new();
    let body = fetcher.fetch(&server.uri()).await.unwrap();
    assert_eq!(body, "||ads.example.com^\n");
}

#[tokio::test]
async fn http_fetcher_errors_on_non_success_status() {
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;

    let fetcher = HttpBlocklistFetcher::new();
    let err = fetcher.fetch(&server.uri()).await.unwrap_err();
    assert!(err.to_string().contains("HTTP 404"), "got: {err}");
}
